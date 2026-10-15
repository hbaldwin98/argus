//! Saved views as stored: one row per view, and the features it is attached
//! to.
//!
//! A view is read and written whole, so its body and its sources are JSON
//! in a column each. What the store cannot hold is the repository the
//! protocol names: a `RepositoryId` is handed out fresh on every start, so a
//! view is filed under its repository's board key (the same string the
//! features of that repository are filed under) and the daemon turns the key
//! into an id when it tells a client.

use anyhow::{Context, Result};
use argus_protocol::{AgentView, PaneId, ViewSource, MAX_SAVED_VIEWS};
use rusqlite::OptionalExtension;

use super::Store;

/// A saved view as the store holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredView {
    pub id: String,
    /// The board key of the repository it belongs to; `None` for a view
    /// that belongs to none.
    pub board: Option<String>,
    /// The number of the feature it is scoped to, on `board`.
    pub feature: Option<i64>,
    /// Features it is attached to, by board key and number.
    pub attached: Vec<(String, i64)>,
    pub recipe: Option<String>,
    pub sources: Vec<ViewSource>,
    pub built_at: i64,
    pub built_commit: Option<String>,
    /// The checkout the saving pane was in: where the commit it was built
    /// at is compared against HEAD.
    pub checkout: Option<String>,
    /// Marked out of date by a person or an agent. The checkout moving on
    /// is not stored; it is read off the tree.
    pub stale: bool,
    /// The pane that last saved it, which a refresh is asked of. Cleared
    /// when the daemon starts, as pane ids mean nothing across a restart.
    pub saved_by: Option<PaneId>,
    pub view: AgentView,
}

impl Store {
    /// Every saved view, oldest build first.
    pub fn saved_views(&self) -> Result<Vec<StoredView>> {
        let conn = self.conn();
        let mut attached: std::collections::HashMap<String, Vec<(String, i64)>> = Default::default();
        let mut stmt =
            conn.prepare("SELECT view, board, number FROM saved_view_feature ORDER BY board, number")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))? {
            let (view, board, number) = row?;
            attached.entry(view).or_default().push((board, number));
        }
        let mut stmt = conn.prepare(
            "SELECT id, board, feature, recipe, sources, body, built_at, built_commit, checkout, stale,
                    saved_by_pane
             FROM saved_view ORDER BY built_at, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, bool>(9)?,
                r.get::<_, Option<i64>>(10)?,
            ))
        })?;
        let mut views = Vec::new();
        for row in rows {
            let (id, board, feature, recipe, sources, body, built_at, built_commit, checkout, stale, pane) = row?;
            // A body this build cannot read is one a newer Argus wrote;
            // leaving it out of the list leaves it in the file.
            let Ok(view) = serde_json::from_str::<AgentView>(&body) else {
                tracing::warn!("saved view {id} is not a view this Argus can read");
                continue;
            };
            views.push(StoredView {
                attached: attached.remove(&id).unwrap_or_default(),
                sources: serde_json::from_str(&sources).unwrap_or_default(),
                saved_by: pane.map(|p| PaneId(p as u64)),
                id,
                board,
                feature,
                recipe,
                built_at,
                built_commit,
                checkout,
                stale,
                view,
            });
        }
        Ok(views)
    }

    /// Keeps `view`, in the place of the one with its id. The store holds
    /// at most [`MAX_SAVED_VIEWS`]; a new id past it is refused.
    pub fn save_view(&self, view: &StoredView) -> Result<()> {
        let body = serde_json::to_string(&view.view).context("encoding the view")?;
        let sources = serde_json::to_string(&view.sources).context("encoding the sources")?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let held: bool = tx
            .query_row("SELECT 1 FROM saved_view WHERE id = ?1", [&view.id], |_| Ok(true))
            .optional()?
            .unwrap_or(false);
        if !held {
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM saved_view", [], |r| r.get(0))?;
            anyhow::ensure!(
                (count as usize) < MAX_SAVED_VIEWS,
                "{MAX_SAVED_VIEWS} views are saved already; delete one before saving another"
            );
        }
        tx.execute(
            "INSERT INTO saved_view
                 (id, board, feature, recipe, sources, body, built_at, built_commit, checkout, stale,
                  saved_by_pane)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                 board = ?2, feature = ?3, recipe = ?4, sources = ?5, body = ?6, built_at = ?7,
                 built_commit = ?8, checkout = ?9, stale = ?10, saved_by_pane = ?11",
            rusqlite::params![
                view.id,
                view.board,
                view.feature,
                view.recipe,
                sources,
                body,
                view.built_at,
                view.built_commit,
                view.checkout,
                view.stale,
                view.saved_by.map(|p| p.0 as i64),
            ],
        )?;
        tx.execute("DELETE FROM saved_view_feature WHERE view = ?1", [&view.id])?;
        for (board, number) in &view.attached {
            tx.execute(
                "INSERT OR IGNORE INTO saved_view_feature (view, board, number) VALUES (?1, ?2, ?3)",
                rusqlite::params![view.id, board, number],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Whether a view was kept under this id to delete.
    pub fn delete_saved_view(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM saved_view_feature WHERE view = ?1", [id])?;
        let changed = tx.execute("DELETE FROM saved_view WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(changed > 0)
    }

    /// Says a view is out of date, or not. Whether a view was there.
    pub fn mark_saved_view_stale(&self, id: &str, stale: bool) -> Result<bool> {
        let changed = self.conn().execute("UPDATE saved_view SET stale = ?2 WHERE id = ?1", rusqlite::params![id, stale])?;
        Ok(changed > 0)
    }
}

/// What removing feature `number` of `board` does to the saved views: the
/// ones scoped to it fall back to their repository, and the ones attached
/// to it let go. Part of the transaction that removes the feature, so a
/// view never names a feature that is gone.
pub(super) fn release_feature(tx: &rusqlite::Transaction<'_>, board: &str, number: i64) -> Result<()> {
    tx.execute(
        "UPDATE saved_view SET feature = NULL WHERE board = ?1 AND feature = ?2",
        rusqlite::params![board, number],
    )?;
    tx.execute(
        "DELETE FROM saved_view_feature WHERE board = ?1 AND number = ?2",
        rusqlite::params![board, number],
    )?;
    Ok(())
}
