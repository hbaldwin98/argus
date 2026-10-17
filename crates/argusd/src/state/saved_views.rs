//! The views agents kept: saving, refreshing, marking stale and deleting
//! them, what an agent and a client may do to them, and the telling of
//! every change to the clients watching.
//!
//! The store has the rows; the daemon holds them too, so that the check run
//! on every change to the tree (has a checkout moved past the commit a view
//! was built at?) reads memory. What clients are sent is derived from a
//! row and the tree: the repository its board key names now, whether it is
//! stale, which features still resolve. `Held::told` remembers what each
//! client was last sent, so a tree that changes none of it sends nothing,
//! and a change to a row or to what it resolves to is sent once.
//!
//! A view whose repository is not in the tree is not sent, and is sent when
//! the repository comes back. It is kept in the store meanwhile.
//!
//! Stale is the person's or an agent's mark, or the checkout the view was
//! saved from being on another commit than the view was built at. Whether a
//! file named in the sources changed is not read: that would be a diff per
//! view on every tick, and the head moving is the signal Argus already has
//! for free. A save clears the mark and records the head it saw.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use argus_protocol::views::press::relay;
use argus_protocol::views::requests::refresh_saved;
use argus_protocol::{
    ArtifactScope, FeatureRef, PaneId, RepositoryId, SavedView, SavedViewChange, SavedViewEntry,
    SavedViewRecord, SavedViewWrite, ViewPress, ViewScope, WriteScope,
};
use tokio::sync::broadcast;

use crate::store::StoredView;

use super::*;

/// The rows, and what the clients were last told of them.
#[derive(Default)]
pub(super) struct Held {
    pub(super) rows: Vec<Row>,
    told: HashMap<String, Told>,
    next_rev: u64,
}

pub(super) struct Row {
    pub(super) stored: StoredView,
    /// Changes whenever the row does, so a row is sent again exactly when
    /// it changed.
    rev: u64,
}

/// Everything a client's copy of a view depends on besides its body.
#[derive(Clone, PartialEq)]
struct Told {
    rev: u64,
    scope: ViewScope,
    features: Vec<FeatureRef>,
    stale: bool,
}

impl Held {
    pub(super) fn load(rows: Vec<StoredView>) -> Held {
        let mut held = Held::default();
        for stored in rows {
            held.put(stored);
        }
        held
    }

    pub(super) fn put(&mut self, stored: StoredView) {
        self.next_rev += 1;
        let row = Row { stored, rev: self.next_rev };
        match self.rows.iter().position(|r| r.stored.id == row.stored.id) {
            Some(at) => self.rows[at] = row,
            None => self.rows.push(row),
        }
    }

    fn get(&self, id: &str) -> Option<&Row> {
        self.rows.iter().find(|r| r.stored.id == id)
    }
}

/// What a row resolves to against the tree, read once per push.
struct Resolver {
    repositories: HashMap<String, Option<RepositoryId>>,
    heads: HashMap<PathBuf, String>,
}

impl Resolver {
    fn repository(&self, board: &str) -> Option<RepositoryId> {
        self.repositories.get(board).copied().flatten()
    }

    /// Whether the checkout a view was saved from is no longer on the
    /// commit it was built at. Unknown (no commit recorded, the checkout
    /// gone, its status not read yet) is not stale: nothing is known.
    fn moved(&self, stored: &StoredView) -> bool {
        let (Some(built), Some(checkout)) = (&stored.built_commit, &stored.checkout) else {
            return false;
        };
        self.heads.get(&PathBuf::from(checkout)).is_some_and(|head| head != built)
    }

    /// What a client is told of the row, or `None` while its repository is
    /// not in the tree.
    fn told(&self, row: &Row) -> Option<Told> {
        let stored = &row.stored;
        let scope = match (&stored.board, stored.feature) {
            (None, _) => ViewScope::Global,
            (Some(board), None) => ViewScope::Repository { repository: self.repository(board)? },
            (Some(board), Some(number)) => ViewScope::Feature { repository: self.repository(board)?, number },
        };
        let features = stored
            .attached
            .iter()
            .filter_map(|(board, number)| Some(FeatureRef { repository: self.repository(board)?, number: *number }))
            .collect();
        Some(Told { rev: row.rev, scope, features, stale: stored.stale || self.moved(stored) })
    }
}

fn wire(row: &Row, told: &Told) -> SavedView {
    let stored = &row.stored;
    SavedView {
        id: stored.id.clone(),
        scope: told.scope,
        features: told.features.clone(),
        recipe: stored.recipe.clone(),
        sources: stored.sources.clone(),
        built_at: stored.built_at,
        built_commit: stored.built_commit.clone(),
        stale: told.stale,
        view: stored.view.clone(),
    }
}

/// What changed among the saved views, for the connections watching them
/// to say as they say it.
#[derive(Debug, Clone)]
pub enum SavedViewNews {
    Changed(Box<SavedView>),
    Removed { id: String },
}

impl Daemon {
    /// The saved views as a client is replayed them, and the receiver of
    /// every change after. Told under the lock, as `watch_views` does, so
    /// the replay and what follows it do not cross.
    pub fn watch_saved_views(&self) -> (Vec<SavedView>, broadcast::Receiver<SavedViewNews>) {
        self.push_saved_views();
        let held = self.saved_views.lock().unwrap();
        let all = held
            .rows
            .iter()
            .filter_map(|row| held.told.get(&row.stored.id).map(|told| wire(row, told)))
            .collect();
        (all, self.pushes.subscribe_saved_views())
    }

    /// Tells the watchers what changed since they were last told: a row
    /// saved, marked or deleted, or a tree that moved a view's repository
    /// or the checkout it was built in.
    pub(super) fn push_saved_views(&self) {
        let boards: Vec<String> = {
            let held = self.saved_views.lock().unwrap();
            if held.rows.is_empty() && held.told.is_empty() {
                return;
            }
            let mut seen = Vec::new();
            for row in &held.rows {
                let stored = &row.stored;
                let boards = stored.board.iter().chain(stored.attached.iter().map(|(board, _)| board));
                seen.extend(boards.cloned());
            }
            seen
        };
        let resolver = self.resolver(boards.into_iter());

        let mut held = self.saved_views.lock().unwrap();
        let held = &mut *held;
        let mut current: HashMap<String, Told> = HashMap::new();
        for row in &held.rows {
            if let Some(told) = resolver.told(row) {
                current.insert(row.stored.id.clone(), told);
            }
        }
        let gone: Vec<String> = held.told.keys().filter(|id| !current.contains_key(*id)).cloned().collect();
        for id in gone {
            held.told.remove(&id);
            self.pushes.saved_view(held, SavedViewNews::Removed { id });
        }
        for row in &held.rows {
            let Some(told) = current.remove(&row.stored.id) else { continue };
            if held.told.get(&row.stored.id) == Some(&told) {
                continue;
            }
            let news = SavedViewNews::Changed(Box::new(wire(row, &told)));
            held.told.insert(row.stored.id.clone(), told);
            self.pushes.saved_view(held, news);
        }
    }

    fn resolver(&self, boards: impl Iterator<Item = String>) -> Resolver {
        let mut repositories = HashMap::new();
        for board in boards {
            if let std::collections::hash_map::Entry::Vacant(slot) = repositories.entry(board) {
                let found = self.repositories_keyed(slot.key()).first().copied();
                slot.insert(found);
            }
        }
        let inner = self.inner.lock().unwrap();
        let heads = tree::checkouts(&inner.projects)
            .filter_map(|c| Some((c.path.clone(), c.git.as_ref()?.last_commit.as_ref()?.oid.clone())))
            .collect();
        Resolver { repositories, heads }
    }

    /// An agent's `saved-view` request: the view saved, listed, read,
    /// deleted or marked stale. An agent reaches the views of its own
    /// repository and the ones that belong to none; a person reaches all.
    ///
    /// The refusal is the limit's own words, for the agent to act on. The
    /// answer is JSON for the helper to print.
    pub fn saved_view_for_agent(
        self: &Arc<Self>,
        pane: PaneId,
        change: SavedViewChange,
    ) -> anyhow::Result<serde_json::Value> {
        let address = self.address_for_agent(pane, ArtifactScope::RepositoryBranch)?;
        let board = address.key.clone();
        let reaches = |stored: &StoredView| stored.board.as_ref().is_none_or(|b| *b == board);
        match change {
            SavedViewChange::Save { view } => {
                let entry = self.save_view_for_agent(pane, &address, *view)?;
                Ok(serde_json::to_value(entry)?)
            }
            SavedViewChange::List => {
                let resolver = self.resolver(std::iter::empty());
                let held = self.saved_views.lock().unwrap();
                let entries: Vec<SavedViewEntry> = held
                    .rows
                    .iter()
                    .filter(|row| reaches(&row.stored))
                    .map(|row| entry(&row.stored, &board, &resolver))
                    .collect();
                Ok(serde_json::to_value(entries)?)
            }
            SavedViewChange::Get { id } => {
                let resolver = self.resolver(std::iter::empty());
                let held = self.saved_views.lock().unwrap();
                let row = held
                    .get(id.trim())
                    .filter(|row| reaches(&row.stored))
                    .ok_or_else(|| anyhow::anyhow!("no saved view {:?} that this pane can reach", id.trim()))?;
                let stored = &row.stored;
                let record = SavedViewRecord {
                    id: stored.id.clone(),
                    scope: write_scope(stored, &board),
                    features: attached_to(stored, &board),
                    recipe: stored.recipe.clone(),
                    sources: stored.sources.clone(),
                    built_at: stored.built_at,
                    built_commit: stored.built_commit.clone(),
                    stale: stored.stale || resolver.moved(stored),
                    view: stored.view.clone(),
                };
                Ok(serde_json::to_value(record)?)
            }
            SavedViewChange::Delete { id } => {
                let id = id.trim();
                let reachable = self.saved_views.lock().unwrap().get(id).map(|row| reaches(&row.stored));
                match reachable {
                    // Already gone is what the agent asked for.
                    None => {}
                    Some(false) => anyhow::bail!("saved view {id:?} belongs to another repository"),
                    Some(true) => {
                        self.delete_saved_view(id)?;
                    }
                }
                Ok(serde_json::Value::Null)
            }
            SavedViewChange::Stale { id } => {
                let id = id.trim();
                let reachable = self.saved_views.lock().unwrap().get(id).map(|row| reaches(&row.stored));
                anyhow::ensure!(reachable == Some(true), "no saved view {id:?} that this pane can reach");
                self.mark_saved_view_stale(id).map_err(|e| anyhow::anyhow!("{e}"))?;
                Ok(serde_json::Value::Null)
            }
        }
    }

    fn save_view_for_agent(
        &self,
        pane: PaneId,
        address: &super::board::address::BoardAddress,
        write: SavedViewWrite,
    ) -> anyhow::Result<SavedViewEntry> {
        let write = write.checked().map_err(|e| anyhow::anyhow!("{e}"))?;
        let board = &address.key;
        let checkout = address.checkout()?.to_path_buf();

        // Every feature it names must be on this repository's board, so a
        // typo is told rather than kept as a link to nothing.
        let wanted = match write.scope {
            WriteScope::Feature { number } => Some(number),
            _ => None,
        };
        if wanted.is_some() || !write.features.is_empty() {
            let known: Vec<i64> = self.store.features(board)?.into_iter().map(|f| f.number).collect();
            for number in wanted.iter().chain(&write.features) {
                anyhow::ensure!(known.contains(number), "this repository has no feature #{number}");
            }
        }
        let (view_board, feature) = match write.scope {
            WriteScope::Global => (None, None),
            WriteScope::Repository => (Some(board.clone()), None),
            WriteScope::Feature { number } => (Some(board.clone()), Some(number)),
        };

        let mut held = self.saved_views.lock().unwrap();
        let mut attached: Vec<(String, i64)> = Vec::new();
        if let Some(old) = held.get(&write.view.id) {
            // An id is the store's, not a repository's; saving another
            // repository's view over its own is how one agent would wipe
            // out the work of another.
            anyhow::ensure!(
                old.stored.board == view_board,
                "the id {:?} is used by a view that belongs somewhere else; choose another id, or delete it first",
                write.view.id
            );
            attached = old.stored.attached.iter().filter(|(b, _)| b != board).cloned().collect();
        }
        attached.extend(write.features.iter().map(|n| (board.clone(), *n)));
        let stored = StoredView {
            id: write.view.id.clone(),
            board: view_board,
            feature,
            attached,
            recipe: write.recipe.filter(|r| !r.trim().is_empty()),
            sources: write.sources,
            built_at: super::board::decisions::now(),
            built_commit: crate::git::head_oid(&checkout),
            checkout: Some(checkout.to_string_lossy().into_owned()),
            stale: false,
            saved_by: Some(pane),
            view: write.view,
        };
        self.store.save_view(&stored)?;
        let answer = entry(&stored, board, &self.resolver(std::iter::empty()));
        held.put(stored);
        drop(held);
        self.push_saved_views();
        Ok(answer)
    }

    /// The person's delete from a client, or the agent's. Whether there
    /// was a view.
    pub fn delete_saved_view(&self, id: &str) -> anyhow::Result<bool> {
        let mut held = self.saved_views.lock().unwrap();
        let existed = self.store.delete_saved_view(id)?;
        held.rows.retain(|row| row.stored.id != id);
        drop(held);
        self.push_saved_views();
        Ok(existed)
    }

    /// Says a saved view is out of date. The next save clears it.
    pub fn mark_saved_view_stale(&self, id: &str) -> Result<(), String> {
        let mut held = self.saved_views.lock().unwrap();
        let Some(at) = held.rows.iter().position(|r| r.stored.id == id) else {
            return Err("That view is no longer saved.".into());
        };
        self.store.mark_saved_view_stale(id, true).map_err(|e| e.to_string())?;
        let mut stored = held.rows[at].stored.clone();
        stored.stale = true;
        held.put(stored);
        drop(held);
        self.push_saved_views();
        Ok(())
    }

    /// Asks the agent that last saved a view to build it again. The view
    /// stays as it is until that agent saves it anew. The reason it could
    /// not be asked is a sentence for the person.
    pub fn refresh_saved_view(&self, id: &str) -> Result<(), String> {
        let (pane, text) = self.relay_target(id, |stored| {
            refresh_saved(&stored.view, stored.recipe.as_deref(), &stored.sources)
        })?;
        if !self.notify(pane, &text) {
            return Err("The agent that saved it is not running, so it could not be asked.".into());
        }
        Ok(())
    }

    /// Tells the agent that last saved a view which of its actions the
    /// person pressed, against what that view offers, as `press_view` does.
    pub fn press_saved_view(&self, id: &str, press: ViewPress) -> Result<(), String> {
        let (pane, text) = self.relay_target(id, |stored| relay(&stored.view, press))?;
        let text = text.map_err(|e| e.to_string())?;
        if !self.notify(pane, &text) {
            return Err("The agent that saved it is not running, so it could not be told.".into());
        }
        Ok(())
    }

    /// The pane that last saved the view, and what `compose` makes of it.
    fn relay_target<T>(&self, id: &str, compose: impl FnOnce(&StoredView) -> T) -> Result<(PaneId, T), String> {
        let held = self.saved_views.lock().unwrap();
        let row = held.get(id).ok_or("That view is no longer saved.")?;
        let pane = row.stored.saved_by.ok_or(
            "No agent has saved it since Argus started, so there is no one to ask. Ask an agent to save it again.",
        )?;
        Ok((pane, compose(&row.stored)))
    }
}

fn write_scope(stored: &StoredView, board: &str) -> WriteScope {
    match (&stored.board, stored.feature) {
        (Some(b), Some(number)) if b == board => WriteScope::Feature { number },
        (Some(_), _) => WriteScope::Repository,
        (None, _) => WriteScope::Global,
    }
}

fn attached_to(stored: &StoredView, board: &str) -> Vec<i64> {
    stored.attached.iter().filter(|(b, _)| b == board).map(|(_, n)| *n).collect()
}

fn entry(stored: &StoredView, board: &str, resolver: &Resolver) -> SavedViewEntry {
    SavedViewEntry {
        id: stored.id.clone(),
        title: stored.view.title.clone(),
        scope: write_scope(stored, board),
        stale: stored.stale || resolver.moved(stored),
        built_at: stored.built_at,
    }
}

