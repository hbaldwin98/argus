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
use argus_protocol::{
    slugify, Decision, DecisionWrite, Feature, FeatureWrite, PaneKind, PaneStatus, ReviewAnchor,
    ReviewComment, TodoAudit, MAX_REVIEW_COMMENTS,
};
use rusqlite::{Connection, OptionalExtension};

use crate::paths::same_path;

mod legacy;
mod schema;

use schema::{SCHEMA_V1, SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6};

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

/// What a note is filed under, once ids are out of the picture.
///
/// The client speaks in `NoteTarget`, which is ids; the store speaks in
/// this, which is what those ids referred to. The daemon translates, and is
/// the only place that knows both.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NoteKey {
    Project(String),
    Checkout(PathBuf),
}

impl NoteKey {
    pub fn checkout(path: &Path) -> NoteKey {
        NoteKey::Checkout(path.to_path_buf())
    }

    fn scope(&self) -> &'static str {
        match self {
            NoteKey::Project(_) => "project",
            NoteKey::Checkout(_) => "checkout",
        }
    }

    fn key(&self) -> String {
        match self {
            NoteKey::Project(name) => name.clone(),
            NoteKey::Checkout(path) => path_text(path),
        }
    }

    /// A row from a store that may be newer than this code. An unknown
    /// scope is dropped rather than guessed at.
    fn from_row(scope: &str, key: String) -> Option<NoteKey> {
        match scope {
            "project" => Some(NoteKey::Project(key)),
            "checkout" => Some(NoteKey::Checkout(PathBuf::from(key))),
            _ => None,
        }
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

/// How many of a note's agent-change records travel with it. See
/// [`Store::note_audit`].
const MAX_NOTE_AUDIT: i64 = 20;

/// The current schema version. Bump it and add an arm to [`migrate`].
const SCHEMA_VERSION: i64 = 6;

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
    ///
    /// Tests that do not need the file should use [`Self::in_memory`]: this
    /// path is the process-global config directory unless `ARGUS_CONFIG_DIR`
    /// is set, so two callers without a private directory share one file
    /// and will lock. Tests that do need the file point that variable at a
    /// temp dir and serialize against each other.
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
        if from < 3 {
            tx.execute_batch(SCHEMA_V3)?;
        }
        if from < 4 {
            tx.execute_batch(SCHEMA_V4)?;
        }
        if from < 5 {
            tx.execute_batch(SCHEMA_V5)?;
        }
        if from < 6 {
            tx.execute_batch(SCHEMA_V6)?;
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

    // ---- notes -------------------------------------------------------

    /// This note's body, or `None` if it has never been written.
    pub fn note(&self, key: &NoteKey) -> Result<Option<String>> {
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT body FROM note WHERE scope = ?1 AND key = ?2",
                rusqlite::params![key.scope(), key.key()],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Writes a note, or removes it when the body has nothing left in it.
    ///
    /// Emptying a note is how a note is deleted: there is no separate
    /// delete, because "select all, backspace, save" is what a person
    /// actually does when they mean to be rid of one.
    pub fn set_note(&self, key: &NoteKey, body: &str) -> Result<()> {
        let conn = self.conn();
        if body.trim().is_empty() {
            conn.execute(
                "DELETE FROM note WHERE scope = ?1 AND key = ?2",
                rusqlite::params![key.scope(), key.key()],
            )?;
            return Ok(());
        }
        conn.execute(
            "INSERT INTO note (scope, key, body) VALUES (?1, ?2, ?3)
             ON CONFLICT(scope, key) DO UPDATE SET body = excluded.body",
            rusqlite::params![key.scope(), key.key(), body],
        )?;
        Ok(())
    }

    /// Every note there is, for folding counts into a tree snapshot.
    ///
    /// One query rather than one per row: the tree is rebuilt on every
    /// change, and a note lookup per checkout would put a statement per
    /// checkout on a path that already walks the whole tree.
    pub fn notes(&self) -> Result<Vec<(NoteKey, String)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT scope, key, body FROM note")?;
        let rows = stmt.query_map([], |r| {
            let scope: String = r.get(0)?;
            let key: String = r.get(1)?;
            Ok((NoteKey::from_row(&scope, key), r.get::<_, String>(2)?))
        })?;
        Ok(rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter_map(|(k, body)| k.map(|k| (k, body)))
            .collect())
    }

    /// Writes a note on an agent's behalf and records that it did, in one
    /// transaction.
    ///
    /// The two halves are useless apart: a body that grew a line nobody
    /// can account for is what the audit exists to prevent, and a record
    /// of a change that never landed is a lie about the note. So this is
    /// one commit rather than a call to [`Self::set_note`] followed by a
    /// second write that may not happen.
    pub fn set_note_as_agent(
        &self,
        key: &NoteKey,
        body: &str,
        entry: &TodoAudit,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO note (scope, key, body) VALUES (?1, ?2, ?3)
             ON CONFLICT(scope, key) DO UPDATE SET body = excluded.body",
            rusqlite::params![key.scope(), key.key(), body],
        )?;
        tx.execute(
            "INSERT INTO note_audit (at, scope, key, session, action, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                entry.at,
                key.scope(),
                key.key(),
                entry.session,
                entry.action,
                entry.detail
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Records one decision on a project's board and returns its id.
    ///
    /// One transaction, because superseding is two writes: the new row,
    /// and the mark on the row it replaces. A board that had gained the
    /// reversal but not the mark would show two live decisions
    /// contradicting each other.
    ///
    /// A superseding decision takes the place of the one it replaces —
    /// its parent, not its children. It is a different answer to the same
    /// question, so it belongs where that question was asked.
    pub fn add_decision(
        &self,
        project: &str,
        write: &DecisionWrite,
        feature: Option<&str>,
        at: i64,
        session: Option<&str>,
        checkout: Option<&str>,
    ) -> Result<i64> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let parent = match write.supersedes {
            Some(old) => tx
                .query_row(
                    "SELECT parent FROM decision WHERE id = ?1 AND project = ?2",
                    rusqlite::params![old, project],
                    |r| r.get::<_, Option<i64>>(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("there is no decision {old} on this board"))?,
            None => write.under,
        };
        if let Some(under) = write.under {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM decision WHERE id = ?1 AND project = ?2)",
                rusqlite::params![under, project],
                |r| r.get(0),
            )?;
            if !exists {
                anyhow::bail!("there is no decision {under} on this board");
            }
        }
        tx.execute(
            "INSERT INTO decision
                 (project, parent, at, session, checkout, feature, chose, over_, because)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                project,
                parent,
                at,
                session,
                checkout,
                feature,
                write.chose,
                write.over,
                write.because
            ],
        )?;
        let id = tx.last_insert_rowid();
        if let Some(old) = write.supersedes {
            tx.execute(
                "UPDATE decision SET superseded_by = ?1 WHERE id = ?2 AND project = ?3",
                rusqlite::params![id, old, project],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }

    /// One project's board, oldest first. Unbounded on purpose: a decision
    /// tree with the old half cut off is a tree with no roots, which is
    /// the part that explains the rest.
    pub fn decisions(&self, project: &str) -> Result<Vec<Decision>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, parent, at, session, checkout, feature, chose, over_, because,
                    superseded_by
             FROM decision WHERE project = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(rusqlite::params![project], |r| {
            Ok(Decision {
                id: r.get(0)?,
                parent: r.get(1)?,
                at: r.get(2)?,
                session: r.get(3)?,
                checkout: r.get(4)?,
                feature: r.get(5)?,
                chose: r.get(6)?,
                over: r.get(7)?,
                because: r.get(8)?,
                superseded_by: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// What agents have done to one note, newest first and bounded.
    ///
    /// Bounded because this is read to answer "where did this line come
    /// from", not to reconstruct a history: the oldest records matter least
    /// and the note is open on screen while they are read.
    pub fn note_audit(&self, key: &NoteKey) -> Result<Vec<TodoAudit>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT at, session, action, detail FROM note_audit
             WHERE scope = ?1 AND key = ?2 ORDER BY id DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![key.scope(), key.key(), MAX_NOTE_AUDIT],
            |r| {
                Ok(TodoAudit {
                    at: r.get(0)?,
                    session: r.get(1)?,
                    action: r.get(2)?,
                    detail: r.get(3)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---- features ----------------------------------------------------

    /// Opens a feature and returns it with the slug it was given.
    ///
    /// The slug is derived from the title and made unique by suffix inside
    /// the transaction: two agents opening the same-sounding feature on two
    /// branches at the same moment must neither collide on one key nor end
    /// up quietly filing decisions on the same board.
    pub fn add_feature(
        &self,
        project: &str,
        write: &FeatureWrite,
        origin_checkout: Option<&str>,
        origin_branch: Option<&str>,
        at: i64,
        session: Option<&str>,
    ) -> Result<Feature> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let base = slugify(&write.title);
        let mut slug = base.clone();
        for n in 2.. {
            let taken: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
                rusqlite::params![project, slug],
                |r| r.get(0),
            )?;
            if !taken {
                break;
            }
            slug = format!("{base}-{n}");
        }
        let feature = Feature {
            slug,
            title: write.title.clone(),
            body: write.body.clone().unwrap_or_default(),
            origin_checkout: origin_checkout.map(str::to_string),
            origin_branch: origin_branch.map(str::to_string),
            at,
            session: session.map(str::to_string),
        };
        tx.execute(
            "INSERT INTO feature
                 (project, slug, title, body, origin_checkout, origin_branch, at, session)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                project,
                feature.slug,
                feature.title,
                feature.body,
                feature.origin_checkout,
                feature.origin_branch,
                feature.at,
                feature.session
            ],
        )?;
        tx.commit()?;
        Ok(feature)
    }

    /// One project's features, oldest first.
    pub fn features(&self, project: &str) -> Result<Vec<Feature>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT slug, title, body, origin_checkout, origin_branch, at, session
             FROM feature WHERE project = ?1 ORDER BY at, slug",
        )?;
        let rows = stmt.query_map(rusqlite::params![project], |r| {
            Ok(Feature {
                slug: r.get(0)?,
                title: r.get(1)?,
                body: r.get(2)?,
                origin_checkout: r.get(3)?,
                origin_branch: r.get(4)?,
                at: r.get(5)?,
                session: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Adds a paragraph to a feature's document, and answers with the
    /// document as it now stands.
    ///
    /// The document is the one part of a feature that is not append-only in
    /// the decision-board sense: it is prose both sides write, so it grows
    /// rather than being superseded. Bounded, because a brief that has
    /// grown past a screen has become the design document it was meant to
    /// point at.
    pub fn append_to_feature(&self, project: &str, slug: &str, text: &str) -> Result<String> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let body: String = tx
            .query_row(
                "SELECT body FROM feature WHERE project = ?1 AND slug = ?2",
                rusqlite::params![project, slug],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("there is no feature {slug} on this project"))?;
        let body = if body.trim().is_empty() {
            text.trim().to_string()
        } else {
            format!("{}\n\n{}", body.trim_end(), text.trim())
        };
        if body.len() > argus_protocol::MAX_FEATURE_BODY_BYTES {
            anyhow::bail!("this feature's document is full; what is left belongs in the checkout");
        }
        tx.execute(
            "UPDATE feature SET body = ?1 WHERE project = ?2 AND slug = ?3",
            rusqlite::params![body, project, slug],
        )?;
        tx.commit()?;
        Ok(body)
    }

    /// Points a checkout at a feature, which is what decisions recorded
    /// from it are filed under afterwards.
    pub fn set_feature_scope(&self, checkout: &Path, project: &str, slug: &str) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
            rusqlite::params![project, slug],
            |r| r.get(0),
        )?;
        if !exists {
            anyhow::bail!("there is no feature {slug} on this project");
        }
        tx.execute(
            "INSERT INTO feature_scope (checkout_path, project, slug) VALUES (?1, ?2, ?3)
             ON CONFLICT(checkout_path) DO UPDATE SET project = excluded.project,
                                                      slug = excluded.slug",
            rusqlite::params![path_text(checkout), project, slug],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The feature a checkout was last pointed at, if it is still one of
    /// this project's.
    pub fn feature_scope(&self, checkout: &Path, project: &str) -> Result<Option<String>> {
        let conn = self.conn();
        Ok(conn
            .query_row(
                "SELECT slug FROM feature_scope WHERE checkout_path = ?1 AND project = ?2",
                rusqlite::params![path_text(checkout), project],
                |r| r.get::<_, String>(0),
            )
            .optional()?)
    }

    // ---- project overlays --------------------------------------------

    pub fn add_project(&self, overlay: &ProjectOverlay) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO project_overlay (root, name, workspace) VALUES (?1, ?2, ?3)
             ON CONFLICT(root) DO UPDATE SET name = excluded.name, workspace = excluded.workspace",
            rusqlite::params![path_text(&overlay.root), overlay.name, overlay.workspace],
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
    /// One the user added is dropped; one the config declares is recorded
    /// as hidden instead, since taking a row out of the panel is not
    /// permission to edit their file. Which it is answers itself: an
    /// overlay is keyed by root, so a root with a row here is one Argus
    /// added. Extra repositories go with it either way.
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
            let mut stmt = conn.prepare("SELECT project, path FROM repo_overlay ORDER BY rowid")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    PathBuf::from(r.get::<_, String>(1)?),
                ))
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
    serde_json::from_str(raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(e))
    })
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

#[cfg(test)]
mod tests;
