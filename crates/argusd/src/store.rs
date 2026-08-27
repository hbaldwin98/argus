//! Runtime state that outlives a daemon run, in one transactional place.
//!
//! The dividing line is ownership. `projects.toml` says what exists and is
//! the user's to edit, comments and all; this says what happened while
//! Argus was running and is Argus's to rewrite. Everything on this side of
//! the line used to be its own file — `session.json`, `excluded-repos`,
//! `open-workspace`, and appended `[[project]]` blocks — each with its own
//! format, its own partial-write story, and its own compatibility ladder.
//! Review state, notes, and boards would each have added another.
//!
//! SQLite in WAL mode, so a write is a transaction rather than a rewrite of
//! everything, and a reader is never blocked by one. Schema changes go
//! through [`migrate`], keyed on `user_version`.

use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;

use anyhow::{Context, Result};
use argus_protocol::{PaneKind, PaneStatus, ReviewAnchor, ReviewComment, MAX_REVIEW_COMMENTS};
use rusqlite::{Connection, OptionalExtension};

/// One pane worth starting again, as it stood when the daemon stopped.
///
/// Checkouts are identified by path and projects by name, because ids are
/// handed out fresh on every start and mean nothing across one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPane {
    pub checkout_path: PathBuf,
    pub kind: PaneKind,
    /// What the row said when the daemon stopped — which, since an agent
    /// can rename its own pane, is not necessarily what to start.
    pub title: String,
    /// The agent template to spawn.
    pub template: Option<String>,
    pub status: PaneStatus,
    pub note: Option<String>,
    /// The harness's stable conversation identity.
    pub harness_session_id: Option<String>,
    /// The harness the pane was running under, which is what decides who
    /// may claim a checkout's last conversation on restore.
    pub harness: Option<String>,
}

impl SessionPane {
    /// What to start again.
    pub fn template(&self) -> &str {
        self.template.as_deref().unwrap_or(&self.title)
    }
}

/// A project the user added at runtime rather than by editing the config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectOverlay {
    pub name: String,
    pub root: PathBuf,
    pub workspace: String,
}

/// Everything the store has to say about what the panel should show,
/// read in one go at startup. Collected into a struct because the caller
/// folds all of it into one tree and a half-read overlay would show a
/// project without its repositories.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Overlays {
    /// Projects added at runtime, each with the extra repositories added
    /// to it after the fact.
    pub projects: Vec<(ProjectOverlay, Vec<PathBuf>)>,
    /// Config-declared projects the user removed from the panel.
    pub hidden: Vec<String>,
    /// Extra repositories added to config-declared projects, by name.
    pub repos: Vec<(String, PathBuf)>,
    pub workspaces: Vec<String>,
    pub excluded: Vec<PathBuf>,
    pub open_workspace: Option<String>,
}

/// Set to anything to start clean. An escape hatch for the case where a
/// restore is the problem — a template that now fails on launch, say.
pub const NO_RESTORE: &str = "ARGUS_NO_RESTORE";

/// The current schema version. Bump it and add an arm to [`migrate`].
const SCHEMA_VERSION: i64 = 2;

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Store")
    }
}

pub struct Store {
    /// One connection, serialized. The daemon's writes are small and rare —
    /// a pane opening, a project being added — and a connection pool would
    /// buy contention handling nothing here contends for.
    conn: StdMutex<Connection>,
}

impl Store {
    /// Opens the store beside the config, creating and migrating it.
    pub fn open() -> Result<Self> {
        let dir = argus_protocol::config_dir();
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let path = dir.join("runtime.db");
        let store = Self::open_at(&path)
            .with_context(|| format!("opening the runtime store at {}", path.display()))?;
        store.import_legacy_files(&dir)?;
        Ok(store)
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// A store that exists only for as long as it is held. Tests get one of
    /// these so a daemon built in a test can never write over the real
    /// user's state, which is what the old opt-in `persist` flag was for.
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        // WAL survives the connection, so this is a no-op after the first
        // open; it is set every time because a store restored from a backup
        // may arrive in rollback mode. It has no effect on an in-memory
        // database, which has nowhere to put the log.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // Durable across a process crash, which is the failure that loses
        // panes. Only a power loss can lose the last commit, and the cost
        // of that is one stale pane row.
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let store = Self {
            conn: StdMutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let mut conn = self.conn();
        let from: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if from > SCHEMA_VERSION {
            anyhow::bail!(
                "the runtime store is version {from}, newer than this Argus understands ({SCHEMA_VERSION}); \
                 upgrade Argus or move runtime.db aside"
            );
        }
        if from == SCHEMA_VERSION {
            return Ok(());
        }
        let tx = conn.transaction()?;
        if from < 1 {
            tx.execute_batch(SCHEMA_V1)?;
        }
        if from < 2 {
            tx.execute_batch(SCHEMA_V2)?;
        }
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
        Ok(())
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A panic while holding the connection leaves the store poisoned but
        // structurally fine — SQLite rolled back whatever was open. Recover
        // rather than cascade the panic into every later write.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- panes -------------------------------------------------------

    /// Replaces the recorded panes with exactly these, in this order.
    ///
    /// A whole-table swap rather than a diff because the tree is the truth
    /// and this follows it: a pane that closed has no row to update.
    pub fn save_panes(&self, panes: &[SessionPane]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM pane", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO pane
                   (seq, checkout_path, kind, title, template, status, note, harness, harness_session_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for (seq, p) in panes.iter().enumerate() {
                stmt.execute(rusqlite::params![
                    seq as i64,
                    path_text(&p.checkout_path),
                    encode(&p.kind)?,
                    p.title,
                    p.template,
                    encode(&p.status)?,
                    p.note,
                    p.harness,
                    p.harness_session_id,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn panes(&self) -> Result<Vec<SessionPane>> {
        if std::env::var_os(NO_RESTORE).is_some() {
            tracing::info!("{NO_RESTORE} is set; starting with nothing running");
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT checkout_path, kind, title, template, status, note, harness, harness_session_id
               FROM pane ORDER BY seq",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SessionPane {
                checkout_path: PathBuf::from(r.get::<_, String>(0)?),
                kind: decode_row(&r.get::<_, String>(1)?, 1)?,
                title: r.get(2)?,
                template: r.get(3)?,
                status: decode_row(&r.get::<_, String>(4)?, 4)?,
                note: r.get(5)?,
                harness: r.get(6)?,
                harness_session_id: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---- review comments ---------------------------------------------

    pub fn add_review_comment(
        &self,
        checkout_path: &Path,
        anchor: ReviewAnchor,
        body: String,
    ) -> Result<ReviewComment> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO review_comment (checkout_path, anchor, body) VALUES (?1, ?2, ?3)",
            rusqlite::params![path_text(checkout_path), encode(&anchor)?, body],
        )?;
        Ok(ReviewComment {
            id: conn.last_insert_rowid() as u64,
            anchor,
            body,
        })
    }

    /// The newest bounded window, returned oldest-first so it reads as a
    /// conversation rather than in reverse database order.
    pub fn review_comments(&self, checkout_path: &Path) -> Result<Vec<ReviewComment>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, anchor, body FROM (
                 SELECT id, anchor, body FROM review_comment
                  WHERE checkout_path = ?1 ORDER BY id DESC LIMIT ?2
             ) ORDER BY id",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![path_text(checkout_path), MAX_REVIEW_COMMENTS as i64],
            |r| {
                Ok(ReviewComment {
                    id: r.get::<_, i64>(0)? as u64,
                    anchor: decode_row(&r.get::<_, String>(1)?, 1)?,
                    body: r.get(2)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---- project overlays --------------------------------------------

    pub fn add_project(&self, overlay: &ProjectOverlay) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO project_overlay (root, name, workspace) VALUES (?1, ?2, ?3)
             ON CONFLICT(root) DO UPDATE SET name = excluded.name, workspace = excluded.workspace",
            rusqlite::params![
                path_text(&overlay.root),
                overlay.name,
                overlay.workspace
            ],
        )?;
        // Adding a project back is the undo for having removed it.
        tx.execute(
            "DELETE FROM project_hidden WHERE name = ?1",
            [&overlay.name],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn project_overlays(&self) -> Result<Vec<ProjectOverlay>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT name, root, workspace FROM project_overlay ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| {
            Ok(ProjectOverlay {
                name: r.get(0)?,
                root: PathBuf::from(r.get::<_, String>(1)?),
                workspace: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Takes a project out of the panel for good.
    ///
    /// A project the user added is simply dropped. One the config declares
    /// cannot be — the file is the user's, and taking a row out of the
    /// panel is not permission to edit it — so it is recorded as hidden
    /// instead. Which of the two this is answers itself: an overlay is
    /// identified by its root, so a root with a row here is one Argus added
    /// and a root without one came from the config.
    ///
    /// Extra repositories go with it either way. Repositories under a
    /// project that is no longer shown describe nothing.
    pub fn remove_project(&self, name: &str, root: Option<&Path>) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let dropped = match root {
            Some(root) => tx.execute(
                "DELETE FROM project_overlay WHERE root = ?1",
                [path_text(root)],
            )?,
            None => 0,
        };
        tx.execute("DELETE FROM repo_overlay WHERE project = ?1", [name])?;
        if dropped == 0 {
            tx.execute(
                "INSERT OR IGNORE INTO project_hidden (name) VALUES (?1)",
                [name],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn hidden_projects(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT name FROM project_hidden")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn add_repo(&self, project: &str, path: &Path) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO repo_overlay (project, path) VALUES (?1, ?2)",
            rusqlite::params![project, path_text(path)],
        )?;
        Ok(())
    }

    /// Extra repository paths for one project, in the order they were added.
    pub fn repos_for(&self, project: &str) -> Result<Vec<PathBuf>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare("SELECT path FROM repo_overlay WHERE project = ?1 ORDER BY rowid")?;
        let rows = stmt.query_map([project], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---- excluded repositories ---------------------------------------

    pub fn exclude_repo(&self, path: &Path) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO repo_excluded (path) VALUES (?1)",
            [path_text(path)],
        )?;
        Ok(())
    }

    pub fn excluded_repos(&self) -> Result<Vec<PathBuf>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT path FROM repo_excluded")?;
        let rows = stmt.query_map([], |r| Ok(PathBuf::from(r.get::<_, String>(0)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Replaces the exclusion set with exactly these paths — how exclusions
    /// under a removed project are forgotten.
    pub fn set_excluded_repos(&self, paths: &[PathBuf]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM repo_excluded", [])?;
        {
            let mut stmt = tx.prepare("INSERT INTO repo_excluded (path) VALUES (?1)")?;
            for p in paths {
                stmt.execute([path_text(p)])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ---- workspaces ---------------------------------------------------

    /// Declaring a workspace is what makes an empty one exist at all: a
    /// workspace with no projects has nothing else to imply it.
    pub fn add_workspace(&self, name: &str) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO workspace_overlay (name) VALUES (?1)",
            [name],
        )?;
        Ok(())
    }

    pub fn workspace_overlays(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT name FROM workspace_overlay ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---- UI state ------------------------------------------------------

    /// The workspace that was open when the daemon last exited. The caller
    /// resolves it against the workspaces that actually exist — the name may
    /// since have been removed from the config.
    pub fn open_workspace(&self) -> Result<Option<String>> {
        Ok(self.ui_get("open_workspace")?.filter(|s| !s.is_empty()))
    }

    pub fn set_open_workspace(&self, name: &str) -> Result<()> {
        self.ui_set("open_workspace", name)
    }

    fn ui_get(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn();
        Ok(conn
            .query_row("SELECT value FROM ui_state WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    fn ui_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO ui_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    // ---- startup read ---------------------------------------------------

    /// Everything the tree needs from the store, in one call.
    ///
    /// Repositories are gathered per project here rather than left to a
    /// later lookup so that one failure can be reported once, at startup,
    /// instead of silently costing one project its rows.
    pub fn overlays(&self) -> Result<Overlays> {
        let mut projects = Vec::new();
        for overlay in self.project_overlays()? {
            let repos = self.repos_for(&overlay.name)?;
            projects.push((overlay, repos));
        }
        let mut repos = Vec::new();
        {
            let conn = self.conn();
            let mut stmt =
                conn.prepare("SELECT project, path FROM repo_overlay ORDER BY rowid")?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, PathBuf::from(r.get::<_, String>(1)?)))
            })?;
            for row in rows {
                repos.push(row?);
            }
        }
        // A repository listed under an added project is already carried by
        // that project; leaving it here too would install it twice.
        let added: Vec<&str> = projects.iter().map(|(p, _)| p.name.as_str()).collect();
        repos.retain(|(project, _)| !added.contains(&project.as_str()));

        Ok(Overlays {
            projects,
            hidden: self.hidden_projects()?,
            repos,
            workspaces: self.workspace_overlays()?,
            excluded: self.excluded_repos()?,
            open_workspace: self.open_workspace()?,
        })
    }

    // ---- one-time import ------------------------------------------------

    /// Folds the files this store replaced into it, once, on the first run
    /// that finds them. Each is renamed rather than deleted: an import that
    /// gets something wrong should be recoverable by hand, and a rename is
    /// also what stops the next run from importing it again.
    ///
    /// Runtime-added `[[project]]` blocks are deliberately not imported.
    /// They are already in `projects.toml`, they still load from there, and
    /// pulling them into the store would show every one of them twice.
    fn import_legacy_files(&self, dir: &Path) -> Result<()> {
        self.import_session_json(&dir.join("session.json"))?;
        self.import_excluded_repos(&dir.join("excluded-repos"))?;
        self.import_open_workspace(&dir.join("open-workspace"))?;
        Ok(())
    }

    fn import_session_json(&self, path: &Path) -> Result<()> {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Ok(());
        };
        match legacy::parse_session(&raw) {
            Ok(panes) => {
                self.save_panes(&panes)?;
                tracing::info!("imported {} panes from {}", panes.len(), path.display());
                retire(path);
            }
            Err(e) => {
                // Leave it in place. A file we could not read is one a later
                // version might, and overwriting it costs the user their
                // workspace for nothing.
                tracing::warn!("could not import {}: {e}", path.display());
            }
        }
        Ok(())
    }

    fn import_excluded_repos(&self, path: &Path) -> Result<()> {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Ok(());
        };
        let paths: Vec<PathBuf> = raw
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect();
        for p in &paths {
            self.exclude_repo(p)?;
        }
        tracing::info!("imported {} exclusions from {}", paths.len(), path.display());
        retire(path);
        Ok(())
    }

    fn import_open_workspace(&self, path: &Path) -> Result<()> {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Ok(());
        };
        let name = raw.trim();
        if !name.is_empty() {
            self.set_open_workspace(name)?;
        }
        retire(path);
        Ok(())
    }
}

/// Moves an imported file aside. Best-effort: failing to rename it only
/// means the next start imports it again over identical rows.
fn retire(path: &Path) {
    let to = path.with_extension("imported");
    if let Err(e) = std::fs::rename(path, &to) {
        tracing::warn!("could not move {} aside after import: {e}", path.display());
    }
}

/// Paths go in with forward slashes so a checkout recorded on one run
/// matches the same directory written the other way on the next.
fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Enums are stored as their serde JSON, so a variant that grows a field —
/// `Exited { code }` already has one — needs no bespoke column mapping and
/// no migration.
fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

fn decode_row<T: serde::de::DeserializeOwned>(raw: &str, column: usize) -> rusqlite::Result<T> {
    serde_json::from_str(raw)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(e)))
}

const SCHEMA_V1: &str = r#"
CREATE TABLE pane (
    seq                INTEGER PRIMARY KEY,
    checkout_path      TEXT NOT NULL,
    kind               TEXT NOT NULL,
    title              TEXT NOT NULL,
    template           TEXT,
    status             TEXT NOT NULL,
    note               TEXT,
    harness            TEXT,
    harness_session_id TEXT
);

-- Keyed on the root rather than the name: what the user added is a
-- directory, and two directories that happen to share a basename are two
-- projects. The name is what they are called, not what they are.
CREATE TABLE project_overlay (
    root      TEXT PRIMARY KEY,
    name      TEXT NOT NULL,
    workspace TEXT NOT NULL
);

CREATE TABLE project_hidden (
    name TEXT PRIMARY KEY
);

CREATE TABLE repo_overlay (
    project TEXT NOT NULL,
    path    TEXT NOT NULL,
    PRIMARY KEY (project, path)
);

CREATE TABLE repo_excluded (
    path TEXT PRIMARY KEY
);

CREATE TABLE workspace_overlay (
    name TEXT PRIMARY KEY
);

CREATE TABLE ui_state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

const SCHEMA_V2: &str = r#"
CREATE TABLE review_comment (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    checkout_path TEXT NOT NULL,
    anchor        TEXT NOT NULL,
    body          TEXT NOT NULL
);
CREATE INDEX review_comment_checkout ON review_comment (checkout_path, id);
"#;

/// Reading `session.json` one last time.
///
/// The `serde(default)` shims live here rather than on the live type: the
/// store's columns are nullable, so the current code has no legacy shape to
/// describe, and every default below exists only to read a file written by
/// a version that predates one field.
mod legacy {
    use super::SessionPane;
    use argus_protocol::{PaneKind, PaneStatus};
    use serde::Deserialize;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    struct Session {
        #[serde(default)]
        panes: Vec<Pane>,
    }

    #[derive(Deserialize)]
    struct Pane {
        checkout_path: PathBuf,
        kind: PaneKind,
        title: String,
        #[serde(default)]
        template: Option<String>,
        #[serde(default = "idle")]
        status: PaneStatus,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        harness_session_id: Option<String>,
        #[serde(default)]
        harness: Option<String>,
    }

    fn idle() -> PaneStatus {
        PaneStatus::Idle
    }

    pub fn parse_session(raw: &str) -> Result<Vec<SessionPane>, serde_json::Error> {
        let s: Session = serde_json::from_str(raw)?;
        Ok(s.panes
            .into_iter()
            .map(|p| SessionPane {
                checkout_path: p.checkout_path,
                kind: p.kind,
                title: p.title,
                template: p.template,
                status: p.status,
                note: p.note,
                harness_session_id: p.harness_session_id,
                harness: p.harness,
            })
            .collect())
    }
}

/// Panes worth starting again.
///
/// Editors are left out on purpose: one belongs to the floating window it
/// opened in, and reopening a file nobody asked for would be noise. A pane
/// whose checkout is no longer configured is dropped too.
pub fn restorable<'a>(
    panes: &'a [SessionPane],
    known: &'a [PathBuf],
) -> impl Iterator<Item = &'a SessionPane> {
    panes.iter().filter(move |p| {
        p.kind != PaneKind::Editor && known.iter().any(|k| same_path(k, &p.checkout_path))
    })
}

/// Compares paths by their canonical form where possible, so a checkout
/// recorded as `C:\src\x` still matches one configured as `C:/src/x`.
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::in_memory().unwrap()
    }

    fn pane(path: &str, kind: PaneKind, title: &str) -> SessionPane {
        SessionPane {
            checkout_path: PathBuf::from(path),
            kind,
            title: title.to_string(),
            template: None,
            status: PaneStatus::Idle,
            note: None,
            harness_session_id: None,
            harness: None,
        }
    }

    fn anchor(line: u32) -> ReviewAnchor {
        ReviewAnchor {
            commit: None,
            base: argus_protocol::ReviewBase::Unstaged,
            path: "src/main.rs".to_string(),
            old_path: None,
            old_start: None,
            old_end: None,
            new_start: Some(line),
            new_end: Some(line),
            text: vec!["+changed".to_string()],
        }
    }

    #[test]
    fn a_pane_survives_a_round_trip() {
        let s = store();
        let mut agent = pane("/repo", PaneKind::Agent, "fixing the pty deadlock");
        agent.template = Some("claude".into());
        agent.status = PaneStatus::NeedsReview;
        agent.note = Some("ready to inspect".into());
        agent.harness = Some("claude".into());
        agent.harness_session_id = Some("session-123".into());
        let want = vec![agent, pane("/repo", PaneKind::Shell, "shell")];

        s.save_panes(&want).unwrap();
        assert_eq!(s.panes().unwrap(), want);
    }

    #[test]
    fn review_comments_survive_a_round_trip_in_order() {
        let s = store();
        let first = s
            .add_review_comment(Path::new("/repo"), anchor(4), "first".to_string())
            .unwrap();
        let second = s
            .add_review_comment(Path::new("/repo"), anchor(9), "second".to_string())
            .unwrap();

        assert_eq!(s.review_comments(Path::new("/repo")).unwrap(), [first, second]);
        assert!(s.review_comments(Path::new("/other")).unwrap().is_empty());
    }

    #[test]
    fn only_the_newest_review_comments_are_returned() {
        let s = store();
        for line in 1..=(MAX_REVIEW_COMMENTS as u32 + 5) {
            s.add_review_comment(Path::new("/repo"), anchor(line), format!("comment {line}"))
                .unwrap();
        }

        let comments = s.review_comments(Path::new("/repo")).unwrap();
        assert_eq!(comments.len(), MAX_REVIEW_COMMENTS);
        assert_eq!(comments.first().unwrap().anchor.new_start, Some(6));
        assert_eq!(comments.last().unwrap().anchor.new_start, Some(105));
    }

    #[test]
    fn saving_replaces_rather_than_appends() {
        // The tree is the truth and the table follows it, so a pane that
        // closed has no row to update — it simply stops being written.
        let s = store();
        s.save_panes(&[pane("/old", PaneKind::Shell, "shell")])
            .unwrap();
        s.save_panes(&[pane("/new", PaneKind::Agent, "claude")])
            .unwrap();

        let back = s.panes().unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].checkout_path, PathBuf::from("/new"));
    }

    #[test]
    fn panes_come_back_in_the_order_they_were_saved() {
        let s = store();
        let want: Vec<SessionPane> = ["a", "b", "c"]
            .iter()
            .map(|t| pane("/repo", PaneKind::Shell, t))
            .collect();
        s.save_panes(&want).unwrap();
        let titles: Vec<String> = s.panes().unwrap().into_iter().map(|p| p.title).collect();
        assert_eq!(titles, ["a", "b", "c"]);
    }

    #[test]
    fn an_exit_code_survives_the_round_trip() {
        // The one status carrying data, and the reason statuses are stored
        // as their serde form rather than a name.
        let s = store();
        let mut p = pane("/repo", PaneKind::Agent, "claude");
        p.status = PaneStatus::Exited { code: Some(3) };
        s.save_panes(&[p]).unwrap();
        assert_eq!(
            s.panes().unwrap()[0].status,
            PaneStatus::Exited { code: Some(3) }
        );
    }

    #[test]
    fn the_escape_hatch_reads_nothing_back() {
        let s = store();
        s.save_panes(&[pane("/repo", PaneKind::Shell, "shell")])
            .unwrap();
        std::env::set_var(NO_RESTORE, "1");
        let out = s.panes();
        std::env::remove_var(NO_RESTORE);
        assert!(out.unwrap().is_empty());
    }

    #[test]
    fn editors_are_not_restored() {
        // An editor belongs to the window it opened in; reopening a file
        // nobody asked for is noise.
        let known = vec![PathBuf::from("/repo")];
        let panes = vec![
            pane("/repo", PaneKind::Editor, "a.rs"),
            pane("/repo", PaneKind::Agent, "claude"),
        ];
        let out: Vec<&SessionPane> = restorable(&panes, &known).collect();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, PaneKind::Agent);
    }

    #[test]
    fn a_pane_whose_checkout_is_gone_is_dropped() {
        let known = vec![PathBuf::from("/still-here")];
        let panes = vec![
            pane("/gone", PaneKind::Shell, "shell"),
            pane("/still-here", PaneKind::Shell, "shell"),
        ];
        let out: Vec<&SessionPane> = restorable(&panes, &known).collect();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].checkout_path, PathBuf::from("/still-here"));
    }

    #[test]
    fn paths_match_across_separator_and_case_differences() {
        let dir = tempfile::tempdir().unwrap();
        let forward = PathBuf::from(dir.path().to_string_lossy().replace('\\', "/"));
        let known = vec![dir.path().to_path_buf()];
        let panes = vec![SessionPane {
            checkout_path: forward,
            ..pane("/unused", PaneKind::Shell, "shell")
        }];
        assert_eq!(restorable(&panes, &known).count(), 1);
    }

    #[test]
    fn a_renamed_agent_comes_back_as_the_template_it_was() {
        // Regression: an agent that renamed its own pane to what it was
        // working on would otherwise be looked up as a template by that
        // name, and never come back at all.
        let mut p = pane("/repo", PaneKind::Agent, "fixing the pty deadlock");
        p.template = Some("claude".into());
        assert_eq!(p.template(), "claude");
    }

    #[test]
    fn two_directories_sharing_a_name_are_two_projects() {
        let s = store();
        s.add_project(&ProjectOverlay {
            name: "src".into(),
            root: PathBuf::from("/home/me/src"),
            workspace: "default".into(),
        })
        .unwrap();
        s.add_project(&ProjectOverlay {
            name: "src".into(),
            root: PathBuf::from("/elsewhere"),
            workspace: "work".into(),
        })
        .unwrap();

        let out = s.project_overlays().unwrap();
        assert_eq!(out.len(), 2, "two directories are two projects");
        assert_eq!(out[1].root, PathBuf::from("/elsewhere"));
        assert_eq!(out[1].workspace, "work");
    }

    #[test]
    fn re_adding_the_same_directory_updates_it_in_place() {
        let s = store();
        for workspace in ["default", "work"] {
            s.add_project(&ProjectOverlay {
                name: "src".into(),
                root: PathBuf::from("/home/me/src"),
                workspace: workspace.into(),
            })
            .unwrap();
        }
        let out = s.project_overlays().unwrap();
        assert_eq!(out.len(), 1, "one directory is one project");
        assert_eq!(out[0].workspace, "work");
    }

    #[test]
    fn removing_a_config_project_hides_it_instead_of_dropping_it() {
        // `projects.toml` is the user's file, so the only way to take a
        // project it declares out of the panel is to record the removal.
        let s = store();
        s.remove_project("declared", Some(Path::new("/declared"))).unwrap();
        assert_eq!(
            s.hidden_projects().unwrap(),
            ["declared"],
            "no overlay row for that root means the config declared it"
        );
    }

    #[test]
    fn removing_an_added_project_leaves_nothing_behind() {
        let s = store();
        s.add_project(&ProjectOverlay {
            name: "added".into(),
            root: PathBuf::from("/added"),
            workspace: "default".into(),
        })
        .unwrap();
        s.add_repo("added", Path::new("/added/repo")).unwrap();

        s.remove_project("added", Some(Path::new("/added"))).unwrap();

        assert!(s.project_overlays().unwrap().is_empty());
        assert!(s.repos_for("added").unwrap().is_empty());
        assert!(
            s.hidden_projects().unwrap().is_empty(),
            "there is no config block to hide"
        );
    }

    #[test]
    fn adding_a_project_back_unhides_it() {
        let s = store();
        s.remove_project("src", None).unwrap();
        s.add_project(&ProjectOverlay {
            name: "src".into(),
            root: PathBuf::from("/src"),
            workspace: "default".into(),
        })
        .unwrap();
        assert!(s.hidden_projects().unwrap().is_empty());
    }

    #[test]
    fn a_repo_is_only_added_to_a_project_once() {
        let s = store();
        s.add_repo("src", Path::new("/src/a")).unwrap();
        s.add_repo("src", Path::new("/src/a")).unwrap();
        s.add_repo("src", Path::new("/src/b")).unwrap();
        assert_eq!(s.repos_for("src").unwrap().len(), 2);
        assert!(s.repos_for("other").unwrap().is_empty());
    }

    #[test]
    fn exclusions_can_be_rewritten_wholesale() {
        // How exclusions under a removed project are forgotten.
        let s = store();
        s.exclude_repo(Path::new("/a")).unwrap();
        s.exclude_repo(Path::new("/b")).unwrap();
        s.set_excluded_repos(&[PathBuf::from("/b")]).unwrap();
        assert_eq!(s.excluded_repos().unwrap(), [PathBuf::from("/b")]);
    }

    #[test]
    fn the_open_workspace_is_remembered() {
        let s = store();
        assert_eq!(s.open_workspace().unwrap(), None);
        s.set_open_workspace("work").unwrap();
        s.set_open_workspace("home").unwrap();
        assert_eq!(s.open_workspace().unwrap().as_deref(), Some("home"));
    }

    #[test]
    fn an_empty_workspace_exists_because_it_was_declared() {
        let s = store();
        s.add_workspace("empty").unwrap();
        s.add_workspace("empty").unwrap();
        assert_eq!(s.workspace_overlays().unwrap(), ["empty"]);
    }

    #[test]
    fn overlays_carry_each_added_projects_own_repositories() {
        let s = store();
        s.add_project(&ProjectOverlay {
            name: "added".into(),
            root: PathBuf::from("/added"),
            workspace: "default".into(),
        })
        .unwrap();
        s.add_repo("added", Path::new("/added/extra")).unwrap();
        s.add_repo("declared", Path::new("/declared/extra")).unwrap();

        let o = s.overlays().unwrap();
        assert_eq!(o.projects.len(), 1);
        assert_eq!(o.projects[0].1, [PathBuf::from("/added/extra")]);
        assert_eq!(
            o.repos,
            [("declared".to_string(), PathBuf::from("/declared/extra"))],
            "an added project's repositories must not also be installed loose"
        );
    }

    #[test]
    fn a_store_reopens_with_its_contents_and_in_wal_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime.db");
        {
            let s = Store::open_at(&path).unwrap();
            s.save_panes(&[pane("/repo", PaneKind::Agent, "claude")])
                .unwrap();
        }
        let s = Store::open_at(&path).unwrap();
        assert_eq!(s.panes().unwrap().len(), 1);

        let mode: String = s
            .conn()
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn a_store_from_a_newer_argus_is_refused_rather_than_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime.db");
        {
            let s = Store::open_at(&path).unwrap();
            s.conn()
                .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                .unwrap();
        }
        let err = Store::open_at(&path).unwrap_err().to_string();
        assert!(err.contains("newer than this Argus understands"), "{err}");
    }

    #[test]
    fn migrating_an_already_current_store_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime.db");
        let s = Store::open_at(&path).unwrap();
        s.save_panes(&[pane("/repo", PaneKind::Shell, "shell")])
            .unwrap();
        s.migrate().unwrap();
        assert_eq!(s.panes().unwrap().len(), 1);
    }

    #[test]
    fn migrating_a_v1_store_preserves_existing_state_and_adds_comments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute(
                "INSERT INTO pane
                   (seq, checkout_path, kind, title, template, status, note, harness, harness_session_id)
                 VALUES (0, '/repo', ?1, 'claude', NULL, ?2, NULL, NULL, NULL)",
                rusqlite::params![encode(&PaneKind::Agent).unwrap(), encode(&PaneStatus::Idle).unwrap()],
            )
            .unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
        }

        let s = Store::open_at(&path).unwrap();
        assert_eq!(s.panes().unwrap().len(), 1);
        let saved = s
            .add_review_comment(Path::new("/repo"), anchor(3), "persist this".to_string())
            .unwrap();
        assert_eq!(s.review_comments(Path::new("/repo")).unwrap(), [saved]);
    }

    #[test]
    fn a_session_file_is_imported_once_and_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("session.json");
        std::fs::write(
            &session,
            r#"{"panes":[{"checkout_path":"/repo","kind":"Agent","title":"claude"}]}"#,
        )
        .unwrap();

        let s = Store::open_at(&dir.path().join("runtime.db")).unwrap();
        s.import_legacy_files(dir.path()).unwrap();

        let panes = s.panes().unwrap();
        assert_eq!(panes.len(), 1);
        assert_eq!(
            panes[0].status,
            PaneStatus::Idle,
            "a file older than status persistence restores as idle"
        );
        assert!(!session.exists(), "an imported file is moved aside");
        assert!(dir.path().join("session.imported").exists());

        // A second start must not import it again over newer rows.
        s.save_panes(&[]).unwrap();
        s.import_legacy_files(dir.path()).unwrap();
        assert!(s.panes().unwrap().is_empty());
    }

    #[test]
    fn a_broken_session_file_is_left_untouched_for_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path().join("session.json");
        std::fs::write(&session, b"{ incomplete").unwrap();

        let s = Store::open_at(&dir.path().join("runtime.db")).unwrap();
        s.import_legacy_files(dir.path()).unwrap();

        assert!(s.panes().unwrap().is_empty());
        assert_eq!(
            std::fs::read(&session).unwrap(),
            b"{ incomplete",
            "a file we could not read is one a later version might"
        );
    }

    #[test]
    fn the_older_side_files_are_imported_too() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("excluded-repos"), "/a\n\n/b\n").unwrap();
        std::fs::write(dir.path().join("open-workspace"), "work\n").unwrap();

        let s = Store::open_at(&dir.path().join("runtime.db")).unwrap();
        s.import_legacy_files(dir.path()).unwrap();

        let mut excluded = s.excluded_repos().unwrap();
        excluded.sort();
        assert_eq!(excluded, [PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(s.open_workspace().unwrap().as_deref(), Some("work"));
    }
}
