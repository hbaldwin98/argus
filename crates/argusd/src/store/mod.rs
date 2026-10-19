//! Runtime state that outlives a daemon run, in one transactional place.
//!
//! The dividing line is ownership. `projects.toml` says what exists and is
//! the user's to edit, comments and all; this says what happened while
//! Argus was running and is Argus's to rewrite. Everything on this side of
//! the line used to be its own file — `session.json`, `excluded-repos`,
//! `open-workspace`, and appended `[[project]]` blocks — each with its own
//! format, its own partial-write story, and its own compatibility ladder.
//! Review state and boards would each have added another.
//!
//! SQLite in WAL mode, so a write is a transaction rather than a rewrite of
//! everything, and a reader is never blocked by one. Schema changes go
//! through [`migrate`], keyed on `user_version`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;

use anyhow::{Context, Result};
use argus_protocol::{
    checked_task_body, checked_task_line, numbered_slug, slugify, Capture, CapturedDecision, CapturedTask,
    Consequence, Criterion, Decision, DecisionState, DecisionText, DecisionWrite, Feature,
    FeatureEvent, FeatureMove, FeatureState, FeatureWrite, PaneId, PaneKind, PaneStatus,
    ReviewAnchor, ReviewComment, Task, TaskCounts, TaskNote, TaskPlace, TaskState, TaskWrite,
    MapDraft, TrackerLink, TrackerMap, MAX_FEATURE_LINKS,
    MAX_CRITERION_BYTES, MAX_DECISION_BYTES, MAX_REVIEW_COMMENTS, MAX_TASK_CRITERIA,
    MAX_TASK_NOTES, MAX_TASK_NOTE_BYTES,
};
use rusqlite::{Connection, OptionalExtension};

use crate::system::paths::same_path;

mod boards;
mod capture;
mod legacy;
mod panel;
mod reconcile;
mod reviews;
mod saved_views;
mod schema;
mod seen_sessions;
mod session;

pub use saved_views::StoredView;
pub use seen_sessions::{SeenSession, Sighting};

use schema::{
    SCHEMA_V1, SCHEMA_V10, SCHEMA_V11, SCHEMA_V12, SCHEMA_V13, SCHEMA_V14, SCHEMA_V15, SCHEMA_V16,
    SCHEMA_V17, SCHEMA_V18, SCHEMA_V19, SCHEMA_V2, SCHEMA_V20, SCHEMA_V21, SCHEMA_V22, SCHEMA_V23,
    SCHEMA_V24, SCHEMA_V25, SCHEMA_V26, SCHEMA_V27, SCHEMA_V28,
    SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6, SCHEMA_V7, SCHEMA_V8, SCHEMA_V9,
};

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
    /// Folders its scan skips beyond the built-in ones: the project's
    /// `exclude`, as it was added.
    pub exclude: Vec<String>,
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

/// For a test of [`NO_RESTORE`]: whether this process started with it set.
/// If not, runs the test named `test` again in a process that did, and
/// fails unless that run passed. The environment is the whole process's,
/// and `cargo test` runs a crate's tests on threads of one: set in a test,
/// it emptied every pane read beside it, and a lock only the tests that set
/// it took could not stop that.
#[cfg(test)]
pub(crate) fn rerun_with_no_restore(test: &str) -> bool {
    if std::env::var_os(NO_RESTORE).is_some() {
        return true;
    }
    // A test binary re-running itself, not a command the daemon starts.
    #[allow(clippy::disallowed_methods)]
    let run = std::process::Command::new(std::env::current_exe().unwrap())
        .args([test, "--exact", "--test-threads=1"])
        .env(NO_RESTORE, "1")
        .output()
        .expect("the test binary should run again");
    let out = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success() && out.contains("1 passed"),
        "{test} with {NO_RESTORE} set:\n{out}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    false
}

/// The current schema version. Bump it and add an arm to [`migrate`].
const SCHEMA_VERSION: i64 = 28;

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
        // Only here, on the one open a daemon makes of its own file: a test
        // reopening a store at a path is checking what survived.
        store.forget_panes()?;
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
        if from < 7 {
            tx.execute_batch(SCHEMA_V7)?;
        }
        if from < 8 {
            tx.execute_batch(SCHEMA_V8)?;
        }
        if from < 9 {
            tx.execute_batch(SCHEMA_V9)?;
        }
        if from < 10 {
            tx.execute_batch(SCHEMA_V10)?;
        }
        if from < 11 {
            tx.execute_batch(SCHEMA_V11)?;
        }
        if from < 12 {
            tx.execute_batch(SCHEMA_V12)?;
            Self::migrate_repository_boards(&tx)?;
        }
        if from < 13 {
            tx.execute_batch(SCHEMA_V13)?;
            Self::normalize_repository_keys(&tx)?;
        }
        if from < 14 {
            tx.execute_batch(SCHEMA_V14)?;
            Self::migrate_legacy_project_features(&tx)?;
        }
        if from < 15 {
            tx.execute_batch(SCHEMA_V15)?;
        }
        if from < 16 {
            tx.execute_batch(SCHEMA_V16)?;
        }
        if from < 17 {
            tx.execute_batch(SCHEMA_V17)?;
        }
        if from < 18 {
            tx.execute_batch(SCHEMA_V18)?;
        }
        if from < 19 {
            tx.execute_batch(SCHEMA_V19)?;
        }
        if from < 20 {
            tx.execute_batch(SCHEMA_V20)?;
        }
        if from < 21 {
            tx.execute_batch(SCHEMA_V21)?;
        }
        if from < 22 {
            tx.execute_batch(SCHEMA_V22)?;
        }
        if from < 23 {
            tx.execute_batch(SCHEMA_V23)?;
        }
        if from < 24 {
            tx.execute_batch(SCHEMA_V24)?;
        }
        if from < 25 {
            tx.execute_batch(SCHEMA_V25)?;
        }
        if from < 26 {
            tx.execute_batch(SCHEMA_V26)?;
        }
        if from < 27 {
            tx.execute_batch(SCHEMA_V27)?;
        }
        if from < 28 {
            tx.execute_batch(SCHEMA_V28)?;
            Self::migrate_legacy_project_features(&tx)?;
        }
        tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        tx.commit()?;
        Ok(())
    }

    fn migrate_repository_boards(tx: &rusqlite::Transaction<'_>) -> Result<()> {
        let prefix = "repository\0";
        for old_key in Self::repository_keys(tx, prefix)? {
            let Some((repository_key, _branch)) = old_key.rsplit_once('\0') else {
                continue;
            };
            if repository_key == prefix.trim_end_matches('\0') {
                continue;
            }
            Self::rekey_repository_project(tx, &old_key, repository_key)?;
        }
        Ok(())
    }

    /// Repairs the identity `migrate_repository_boards` gave a repository
    /// board: it kept whichever checkout's Git directory the key already
    /// carried, which for a linked worktree is private to that worktree
    /// rather than the directory every worktree shares. Recomputes each
    /// key's directory through the same resolution the running daemon now
    /// uses, so a board written from one worktree is the same board read
    /// from another.
    fn normalize_repository_keys(tx: &rusqlite::Transaction<'_>) -> Result<()> {
        let prefix = "repository\0";
        for old_key in Self::repository_keys(tx, prefix)? {
            let Some(dir) = old_key.strip_prefix(prefix) else {
                continue;
            };
            let common = crate::git::resolve_commondir(Path::new(dir));
            let new_key = format!("{prefix}{}", common.to_string_lossy());
            if new_key == old_key {
                continue;
            }
            Self::rekey_repository_project(tx, &old_key, &new_key)?;
        }
        Ok(())
    }

    /// Every project/artifact-scope key in use that starts with `prefix`,
    /// across the tables a repository board's rows live in.
    fn repository_keys(tx: &rusqlite::Transaction<'_>, prefix: &str) -> Result<Vec<String>> {
        let mut stmt = tx.prepare(
            "SELECT project FROM feature WHERE project LIKE ?1
             UNION SELECT project FROM decision WHERE project LIKE ?1
             UNION SELECT project FROM task WHERE project LIKE ?1
             UNION SELECT project FROM feature_event WHERE project LIKE ?1
             UNION SELECT artifact_scope FROM artifact_feature_scope WHERE artifact_scope LIKE ?1
             ORDER BY 1",
        )?;
        let keys = stmt
            .query_map([format!("{prefix}%")], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(keys)
    }

    /// Moves every row filed under `old_key` to `new_key`, resolving slug
    /// collisions the same way a live rename would: the incoming feature
    /// keeps its slug where the destination board has no such slug yet, and
    /// otherwise gets a numbered suffix.
    fn rekey_repository_project(
        tx: &rusqlite::Transaction<'_>,
        old_key: &str,
        new_key: &str,
    ) -> Result<()> {
        if new_key == old_key {
            return Ok(());
        }
        let mut stmt =
            tx.prepare("SELECT slug FROM feature WHERE project = ?1 ORDER BY at, slug")?;
        let slugs = stmt
            .query_map([old_key], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for old_slug in slugs {
            Self::move_feature_slug(tx, old_key, &old_slug, new_key)?;
        }
        tx.execute(
            "UPDATE decision SET project = ?1 WHERE project = ?2",
            rusqlite::params![new_key, old_key],
        )?;
        tx.execute(
            "INSERT INTO artifact_feature_scope (artifact_scope, checkout_path, slug)
             SELECT ?1, checkout_path, slug FROM artifact_feature_scope WHERE artifact_scope = ?2
             ON CONFLICT(artifact_scope, checkout_path) DO UPDATE SET slug = excluded.slug",
            rusqlite::params![new_key, old_key],
        )?;
        tx.execute(
            "DELETE FROM artifact_feature_scope WHERE artifact_scope = ?1",
            [old_key],
        )?;
        Ok(())
    }

    /// Moves one feature's row and everything filed under its slug —
    /// decisions, tasks, events, and scope entries — from `old_key` to
    /// `new_key`, keeping the slug where the destination has no such slug
    /// yet and otherwise giving it a numbered suffix. Rows at `old_key` not
    /// tied to this slug (another feature, or an unfiled decision) are left
    /// where they are: the caller does not know they belong to `new_key`
    /// too, only that this one feature does.
    fn move_feature_slug(
        tx: &rusqlite::Transaction<'_>,
        old_key: &str,
        old_slug: &str,
        new_key: &str,
    ) -> Result<()> {
        let mut slug = old_slug.to_string();
        for suffix in 2.. {
            let taken: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM feature WHERE project = ?1 AND slug = ?2)",
                rusqlite::params![new_key, slug],
                |row| row.get(0),
            )?;
            if !taken {
                break;
            }
            slug = numbered_slug(old_slug, suffix);
        }
        tx.execute(
            "UPDATE feature SET project = ?1, slug = ?2 WHERE project = ?3 AND slug = ?4",
            rusqlite::params![new_key, slug, old_key, old_slug],
        )?;
        for (table, feature_column) in [
            ("decision", "feature"),
            ("task", "feature"),
            ("feature_event", "slug"),
        ] {
            tx.execute(
                &format!(
                    "UPDATE {table} SET project = ?1, {feature_column} = ?2 \
                     WHERE project = ?3 AND {feature_column} = ?4"
                ),
                rusqlite::params![new_key, slug, old_key, old_slug],
            )?;
        }
        // Moved to `new_key` with the feature rather than renamed where it
        // stands: left at `old_key` under its new slug, the pointer is
        // matched again when the next feature moved off that board was
        // renamed to the slug this one just took. A checkout already pointing
        // at a feature on `new_key` keeps that pointer.
        tx.execute(
            "INSERT INTO artifact_feature_scope (artifact_scope, checkout_path, slug)
             SELECT ?1, checkout_path, ?2 FROM artifact_feature_scope
             WHERE artifact_scope = ?3 AND slug = ?4
             ON CONFLICT(artifact_scope, checkout_path) DO NOTHING",
            rusqlite::params![new_key, slug, old_key, old_slug],
        )?;
        tx.execute(
            "DELETE FROM artifact_feature_scope WHERE artifact_scope = ?1 AND slug = ?2",
            rusqlite::params![old_key, old_slug],
        )?;
        tx.execute(
            "INSERT INTO artifact_feature_scope (artifact_scope, checkout_path, slug)
             SELECT ?1, checkout_path, ?2 FROM feature_scope
             WHERE project = ?3 AND slug = ?4
             ON CONFLICT(artifact_scope, checkout_path) DO NOTHING",
            rusqlite::params![new_key, slug, old_key, old_slug],
        )?;
        Ok(())
    }

    /// A feature written before repository scoping (`5b3812e`) was filed
    /// under its project's plain display name — the only key that existed
    /// then — and nothing ever moved it when repository-scoped keys took
    /// over as the only ones the client reads. It didn't go anywhere, but a
    /// project spanning several repositories has one shared name and many
    /// Git directories, so there is no single repository key to rename that
    /// name to; each feature has to be relocated on its own, using the
    /// repository its `origin_checkout` was in.
    ///
    /// A worktree is often deleted once its feature is done, so where the
    /// origin checkout is gone the checkouts the feature's decisions were
    /// made in place it instead, when they agree on one repository. An
    /// unfiled decision goes by its own checkout, or failing that to the one
    /// repository everything else under the name went to. Whatever is still
    /// ambiguous is left where it is — better invisible to the current view
    /// than silently filed under the wrong repository.
    ///
    /// Run again at `SCHEMA_V28`: before then a feature was placed by its
    /// origin checkout alone, and an unfiled decision never moved.
    fn migrate_legacy_project_features(tx: &rusqlite::Transaction<'_>) -> Result<()> {
        let strings = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> Result<Vec<String>> {
            let mut stmt = tx.prepare(sql)?;
            let rows = stmt
                .query_map(params, |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        };
        let board_of = |checkout: &str| {
            crate::git::repository_common_dir(Path::new(checkout))
                .map(|common| format!("repository\0{}", common.to_string_lossy()))
        };
        let one = |boards: std::collections::BTreeSet<String>| {
            (boards.len() == 1).then(|| boards.into_iter().next()).flatten()
        };

        let old_keys = strings(
            "SELECT project FROM feature WHERE project NOT LIKE ?1 AND project NOT LIKE ?2
             UNION SELECT project FROM decision WHERE project NOT LIKE ?1 AND project NOT LIKE ?2",
            &[&"repository\0%", &"workspace\0%"],
        )?;
        for old_key in old_keys {
            let mut placed = std::collections::BTreeSet::new();

            let mut stmt =
                tx.prepare("SELECT slug, origin_checkout FROM feature WHERE project = ?1 ORDER BY at, slug")?;
            let features = stmt
                .query_map([&old_key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(stmt);
            for (old_slug, origin_checkout) in features {
                let decided_in = || -> Result<_> {
                    let checkouts = strings(
                        "SELECT DISTINCT checkout FROM decision
                         WHERE project = ?1 AND feature = ?2 AND checkout IS NOT NULL",
                        &[&old_key, &old_slug],
                    )?;
                    Ok(one(checkouts.iter().filter_map(|c| board_of(c)).collect()))
                };
                let new_key = match origin_checkout.as_deref().and_then(board_of) {
                    Some(key) => Some(key),
                    None => decided_in()?,
                };
                if let Some(new_key) = new_key {
                    Self::move_feature_slug(tx, &old_key, &old_slug, &new_key)?;
                    placed.insert(new_key);
                }
            }

            let mut stmt =
                tx.prepare("SELECT id, checkout FROM decision WHERE project = ?1 AND feature IS NULL")?;
            let unfiled = stmt
                .query_map([&old_key], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(stmt);
            let mut homeless = Vec::new();
            for (id, checkout) in unfiled {
                match checkout.as_deref().and_then(board_of) {
                    Some(new_key) => {
                        tx.execute(
                            "UPDATE decision SET project = ?1 WHERE id = ?2",
                            rusqlite::params![new_key, id],
                        )?;
                        placed.insert(new_key);
                    }
                    None => homeless.push(id),
                }
            }
            if let Some(new_key) = one(placed) {
                for id in homeless {
                    tx.execute(
                        "UPDATE decision SET project = ?1 WHERE id = ?2",
                        rusqlite::params![new_key, id],
                    )?;
                }
            }
        }
        Ok(())
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A panic while holding the connection leaves the store poisoned but
        // structurally fine — SQLite rolled back whatever was open. Recover
        // rather than cascade the panic into every later write.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Runs one statement that has to touch a row, and fails with `missing`
    /// when it touched none. A rename, rewrite or removal by id that
    /// matched nothing would otherwise read as having worked.
    fn update_one(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
        missing: impl FnOnce() -> String,
    ) -> Result<()> {
        if self.conn().execute(sql, params)? == 0 {
            anyhow::bail!(missing());
        }
        Ok(())
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
