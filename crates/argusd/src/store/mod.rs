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

use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;

use anyhow::{Context, Result};
use argus_protocol::{
    checked_task_body, slugify, Decision, DecisionWrite, Feature, FeatureEvent, FeatureMove,
    FeatureState, FeatureWrite, PaneKind, PaneStatus, ReviewAnchor, ReviewComment, Task,
    TaskCounts, TaskState, TaskWrite, MAX_REVIEW_COMMENTS,
};
use rusqlite::{Connection, OptionalExtension};

use crate::paths::same_path;

mod boards;
mod legacy;
mod panel;
mod reviews;
mod schema;
mod session;

use schema::{
    SCHEMA_V1, SCHEMA_V10, SCHEMA_V11, SCHEMA_V12, SCHEMA_V13, SCHEMA_V14, SCHEMA_V2, SCHEMA_V3,
    SCHEMA_V4, SCHEMA_V5, SCHEMA_V6, SCHEMA_V7, SCHEMA_V8, SCHEMA_V9,
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
const SCHEMA_VERSION: i64 = 14;

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
        let mut stmt = tx.prepare("SELECT slug FROM feature WHERE project = ?1 ORDER BY at, slug")?;
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
            slug = format!("{old_slug}-{suffix}");
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
        tx.execute(
            "UPDATE artifact_feature_scope SET slug = ?1
             WHERE artifact_scope = ?2 AND slug = ?3",
            rusqlite::params![slug, old_key, old_slug],
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
    /// A feature whose origin checkout is gone, or no longer a Git
    /// repository, is left where it is — better an old feature invisible to
    /// the current view than one silently filed under the wrong repository.
    fn migrate_legacy_project_features(tx: &rusqlite::Transaction<'_>) -> Result<()> {
        let mut stmt = tx.prepare(
            "SELECT project, slug, origin_checkout FROM feature
             WHERE project NOT LIKE ?1 AND project NOT LIKE ?2
             ORDER BY project, at, slug",
        )?;
        let rows = stmt
            .query_map(["repository\0%", "workspace\0%"], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        for (old_key, old_slug, origin_checkout) in rows {
            let Some(origin_checkout) = origin_checkout else {
                continue;
            };
            let Some(common) = crate::git::repository_common_dir(Path::new(&origin_checkout))
            else {
                continue;
            };
            let new_key = format!("repository\0{}", common.to_string_lossy());
            Self::move_feature_slug(tx, &old_key, &old_slug, &new_key)?;
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
