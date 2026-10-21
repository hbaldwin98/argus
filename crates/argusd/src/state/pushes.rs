//! What the daemon publishes for its connections to push: one channel per
//! topic, each as deep as its bursts, and the latest board of each key for
//! a connection that fell behind them.
//!
//! Every push goes through here, so a send site elsewhere that names a
//! `broadcast::Sender` is one this module does not know about. A send with
//! no receiver is not an error: no client need be connected.

use std::collections::VecDeque;
use std::sync::Mutex as StdMutex;

use argus_protocol::{
    AgentTelemetry, DecisionBoard, DiagramList, PaneId, ProjectId, ProjectInfo, RepositoryId, TaskList,
    WorkspaceInfo,
};
use tokio::sync::broadcast;

use super::browsers::{self, BrowserNews};
use super::views::saved::{self, SavedViewNews};
use super::views::{self, ViewNews};
use super::ExtensionNews;

/// The 32 trees and 64 reports the two channels held before they were
/// one: a tree comes at a structural change or a poll, and a report with
/// every tool call.
pub(crate) const TREE_DEPTH: usize = 128;
const WORKSPACES_DEPTH: usize = 32;
/// The three channels of 32 the boards had before they were one.
pub(crate) const BOARDS_DEPTH: usize = 64;
/// How many boards' latest pushes are kept for a connection that fell
/// behind. A board is one feature's or one project's, so this is more than
/// the person has open at once.
const REMEMBERED_BOARDS: usize = 64;
/// A tick carries nothing, and a watcher that misses some reads the focus
/// whole anyway.
const FOCUS_DEPTH: usize = 16;
/// A pane dropped closes all its views at once.
pub(crate) const VIEWS_DEPTH: usize = 64;
const SAVED_VIEWS_DEPTH: usize = 64;
pub(crate) const EXTENSIONS_DEPTH: usize = 64;
/// Shallow on purpose: a watcher this far behind is sent each browser's
/// latest picture rather than the backlog.
const BROWSERS_DEPTH: usize = 16;

/// The tree, or one pane's merged telemetry, on one channel. A report is
/// apart from the tree because it changes on every tool call, and a client
/// that can take one pane's record should not be sent every pane's; it is
/// on the tree's channel so that the two keep the order they were read in.
#[derive(Debug, Clone)]
pub enum TreeNews {
    Tree(Vec<ProjectInfo>),
    Telemetry { pane: PaneId, telemetry: Box<AgentTelemetry> },
}

/// A feature board's part, pushed whole. Pushed at every client; one
/// reading another repository's or feature's drops it.
#[derive(Debug, Clone)]
pub enum BoardNews {
    Decisions(Box<DecisionBoard>),
    Tasks(Box<TaskList>),
    Diagrams(Box<DiagramList>),
}

/// Which board a push is the latest of: a later push of the same key
/// replaces it whole.
#[derive(PartialEq, Eq)]
enum BoardKey {
    Decisions(Vec<RepositoryId>, Option<ProjectId>),
    Tasks(Vec<RepositoryId>, Option<String>),
    Diagrams(Vec<RepositoryId>, Option<String>),
}

impl BoardNews {
    fn key(&self) -> BoardKey {
        match self {
            BoardNews::Decisions(board) => BoardKey::Decisions(board.repositories.clone(), board.project),
            BoardNews::Tasks(list) => BoardKey::Tasks(list.repositories.clone(), list.feature.clone()),
            BoardNews::Diagrams(list) => BoardKey::Diagrams(list.repositories.clone(), list.feature.clone()),
        }
    }
}

pub struct Pushes {
    tree: broadcast::Sender<TreeNews>,
    /// Held while a tree or a report is read and sent, so a tree taken
    /// before a report can never be sent after it and wind the numbers
    /// back: the channel alone orders the sends, not the reads. Taken
    /// before `Daemon::inner` and never while it is held, which the
    /// snapshot and the merge each lock for themselves.
    tree_order: StdMutex<()>,
    workspaces: broadcast::Sender<Vec<WorkspaceInfo>>,
    /// Whole boards rather than one decision or task each: a client
    /// watching a tree being built needs the tree, and a board is small
    /// enough that sending the whole of it is cheaper than teaching both
    /// sides how to splice a node into one.
    boards: broadcast::Sender<BoardNews>,
    /// The latest push of each board, oldest first, for a connection that
    /// fell behind: the daemon cannot know which boards a client shows, and
    /// a client drops those it does not. Written, and each push sent, under
    /// this lock, so the record's order is the channel's. Workspace boards
    /// are never pushed, so never kept.
    latest_boards: StdMutex<VecDeque<BoardNews>>,
    /// A tick whenever a tree is pushed or a client's view changes or
    /// goes, for the connections watching the focus to read it again.
    focus: broadcast::Sender<()>,
    views: broadcast::Sender<ViewNews>,
    saved_views: broadcast::Sender<SavedViewNews>,
    extensions: broadcast::Sender<ExtensionNews>,
    browsers: broadcast::Sender<BrowserNews>,
}

impl Pushes {
    pub(super) fn new() -> Pushes {
        Pushes {
            tree: broadcast::channel(TREE_DEPTH).0,
            tree_order: StdMutex::new(()),
            workspaces: broadcast::channel(WORKSPACES_DEPTH).0,
            boards: broadcast::channel(BOARDS_DEPTH).0,
            latest_boards: StdMutex::new(VecDeque::new()),
            focus: broadcast::channel(FOCUS_DEPTH).0,
            views: broadcast::channel(VIEWS_DEPTH).0,
            saved_views: broadcast::channel(SAVED_VIEWS_DEPTH).0,
            extensions: broadcast::channel(EXTENSIONS_DEPTH).0,
            browsers: broadcast::channel(BROWSERS_DEPTH).0,
        }
    }

    /// The tree `snapshot` takes, read and sent in `tree_order`; then a
    /// tick of the focus, since a tree can move where the person is. Each
    /// focus watcher reads the tree again on that tick, a cost on this hot
    /// path that is small at the one or two windows that watch.
    pub(super) fn tree(&self, snapshot: impl FnOnce() -> Vec<ProjectInfo>) {
        {
            let _order = self.tree_order.lock().unwrap();
            let _ = self.tree.send(TreeNews::Tree(snapshot()));
        }
        self.focus_moved();
    }

    /// The record `merge` folds in, merged and sent in `tree_order`;
    /// `None` sends nothing.
    pub(super) fn telemetry(&self, merge: impl FnOnce() -> Option<(PaneId, AgentTelemetry)>) {
        let _order = self.tree_order.lock().unwrap();
        if let Some((pane, telemetry)) = merge() {
            let _ = self.tree.send(TreeNews::Telemetry { pane, telemetry: Box::new(telemetry) });
        }
    }

    pub(super) fn workspaces(&self, all: Vec<WorkspaceInfo>) {
        let _ = self.workspaces.send(all);
    }

    /// Kept as its key's latest, and sent while that is held.
    pub(super) fn board(&self, board: BoardNews) {
        let mut latest = self.latest_boards.lock().unwrap();
        let key = board.key();
        latest.retain(|kept| kept.key() != key);
        if latest.len() == REMEMBERED_BOARDS {
            latest.pop_front();
        }
        latest.push_back(board.clone());
        let _ = self.boards.send(board);
    }

    pub(super) fn focus_moved(&self) {
        let _ = self.focus.send(());
    }

    /// Sent under the views lock, whose guard `_held` is, so a watcher's
    /// replay and the changes after it do not cross.
    pub(super) fn view(&self, _held: &views::Views, news: ViewNews) {
        let _ = self.views.send(news);
    }

    /// Sent under the saved-views lock, for the same reason.
    pub(super) fn saved_view(&self, _held: &saved::Held, news: SavedViewNews) {
        let _ = self.saved_views.send(news);
    }

    pub(super) fn extension(&self, news: ExtensionNews) {
        let _ = self.extensions.send(news);
    }

    /// The tree as `snapshot` reads it, and the receiver of every tree and
    /// report after it, both taken in `tree_order`: nothing published
    /// between them is missed, and none of what the receiver holds is older
    /// than the tree.
    pub(crate) fn follow_tree(
        &self,
        snapshot: impl FnOnce() -> Vec<ProjectInfo>,
    ) -> (Vec<ProjectInfo>, broadcast::Receiver<TreeNews>) {
        let _order = self.tree_order.lock().unwrap();
        let rx = self.tree.subscribe();
        (snapshot(), rx)
    }

    pub(crate) fn subscribe_workspaces(&self) -> broadcast::Receiver<Vec<WorkspaceInfo>> {
        self.workspaces.subscribe()
    }

    /// The latest push of each board, oldest first, and the receiver of
    /// every push after them, taken under one lock: nothing pushed between
    /// them is missed, and none of what the receiver holds is older than
    /// what was replayed. A lagged receiver is never caught up from: its
    /// backlog holds older boards of keys the replay has sent newer.
    pub(crate) fn follow_boards(&self) -> (Vec<BoardNews>, broadcast::Receiver<BoardNews>) {
        let latest = self.latest_boards.lock().unwrap();
        (latest.iter().cloned().collect(), self.boards.subscribe())
    }

    /// Every push from now on, with no replay: for a connection that asks
    /// for the boards it shows, which needs no copy of every kept one.
    pub(crate) fn subscribe_boards(&self) -> broadcast::Receiver<BoardNews> {
        self.boards.subscribe()
    }

    pub(crate) fn subscribe_focus(&self) -> broadcast::Receiver<()> {
        self.focus.subscribe()
    }

    /// Sent under the browsers lock, for the same reason.
    pub(super) fn browser(&self, _held: &browsers::Browsers, news: BrowserNews) {
        let _ = self.browsers.send(news);
    }

    /// Only under the browsers lock: see `Daemon::watch_browsers`.
    pub(super) fn subscribe_browsers(&self) -> broadcast::Receiver<BrowserNews> {
        self.browsers.subscribe()
    }

    /// Only under the views lock: see `Daemon::watch_views`.
    pub(super) fn subscribe_views(&self) -> broadcast::Receiver<ViewNews> {
        self.views.subscribe()
    }

    /// Only under the saved-views lock: see `Daemon::watch_saved_views`.
    pub(super) fn subscribe_saved_views(&self) -> broadcast::Receiver<SavedViewNews> {
        self.saved_views.subscribe()
    }

    pub(crate) fn subscribe_extensions(&self) -> broadcast::Receiver<ExtensionNews> {
        self.extensions.subscribe()
    }
}

/// A test's view of one kind of news on a channel that carries several:
/// the other kinds are skipped, so a test written against a channel of its
/// own reads the same after channels merge.
#[cfg(test)]
pub(crate) struct Pushed<N, T> {
    rx: broadcast::Receiver<N>,
    pick: fn(N) -> Option<T>,
}

#[cfg(test)]
impl<N: Clone, T> Pushed<N, T> {
    pub(crate) fn new(rx: broadcast::Receiver<N>, pick: fn(N) -> Option<T>) -> Self {
        Pushed { rx, pick }
    }

    pub(crate) fn try_recv(&mut self) -> Result<T, broadcast::error::TryRecvError> {
        loop {
            if let Some(news) = (self.pick)(self.rx.try_recv()?) {
                return Ok(news);
            }
        }
    }

    pub(crate) async fn recv(&mut self) -> Result<T, broadcast::error::RecvError> {
        loop {
            if let Some(news) = (self.pick)(self.rx.recv().await?) {
                return Ok(news);
            }
        }
    }
}

#[cfg(test)]
impl Pushes {
    /// A board pushed the way state pushes one, for a test outside state.
    pub(crate) fn push_board(&self, board: BoardNews) {
        self.board(board);
    }
}

#[cfg(test)]
impl super::Daemon {
    pub(crate) fn subscribe_workspaces(&self) -> broadcast::Receiver<Vec<WorkspaceInfo>> {
        self.pushes.subscribe_workspaces()
    }

    pub(crate) fn subscribe_extensions(&self) -> broadcast::Receiver<ExtensionNews> {
        self.pushes.subscribe_extensions()
    }

    pub(crate) fn subscribe_tree(&self) -> Pushed<TreeNews, Vec<ProjectInfo>> {
        let (_, rx) = self.pushes.follow_tree(Vec::new);
        Pushed::new(rx, |news| match news {
            TreeNews::Tree(tree) => Some(tree),
            TreeNews::Telemetry { .. } => None,
        })
    }

    pub(crate) fn subscribe_telemetry(&self) -> Pushed<TreeNews, (PaneId, AgentTelemetry)> {
        let (_, rx) = self.pushes.follow_tree(Vec::new);
        Pushed::new(rx, |news| match news {
            TreeNews::Telemetry { pane, telemetry } => Some((pane, *telemetry)),
            TreeNews::Tree(_) => None,
        })
    }

    pub(crate) fn subscribe_decisions(&self) -> Pushed<BoardNews, DecisionBoard> {
        Pushed::new(self.pushes.subscribe_boards(), |news| match news {
            BoardNews::Decisions(board) => Some(*board),
            _ => None,
        })
    }

    pub(crate) fn subscribe_tasks(&self) -> Pushed<BoardNews, TaskList> {
        Pushed::new(self.pushes.subscribe_boards(), |news| match news {
            BoardNews::Tasks(list) => Some(*list),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests;
