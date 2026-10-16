//! What the daemon publishes for its connections to push: one channel per
//! topic, each as deep as its bursts.
//!
//! Every push goes through here, so a send site elsewhere that names a
//! `broadcast::Sender` is one this module does not know about. A send with
//! no receiver is not an error: no client need be connected.

use std::sync::Mutex as StdMutex;

use argus_protocol::{
    AgentTelemetry, DecisionBoard, DiagramList, PaneId, ProjectInfo, ServerMsg, TaskList, WorkspaceInfo,
};
use tokio::sync::broadcast;

use super::views::ViewEvent;
use super::ExtensionNews;

/// The 32 trees and 64 reports the two channels held before they were
/// one: a tree comes at a structural change or a poll, and a report with
/// every tool call.
const TREE_DEPTH: usize = 128;
const WORKSPACES_DEPTH: usize = 32;
const DECISIONS_DEPTH: usize = 32;
const TASKS_DEPTH: usize = 32;
const DIAGRAMS_DEPTH: usize = 32;
/// A tick carries nothing, and a watcher that misses some reads the focus
/// whole anyway.
const FOCUS_DEPTH: usize = 16;
/// A pane dropped closes all its views at once.
const VIEWS_DEPTH: usize = 64;
const SAVED_VIEWS_DEPTH: usize = 64;
const EXTENSIONS_DEPTH: usize = 64;

/// The tree, or one pane's merged telemetry, on one channel. A report is
/// apart from the tree because it changes on every tool call, and a client
/// that can take one pane's record should not be sent every pane's; it is
/// on the tree's channel so that the two keep the order they were read in.
#[derive(Debug, Clone)]
pub enum TreeNews {
    Tree(Vec<ProjectInfo>),
    Telemetry { pane: PaneId, telemetry: Box<AgentTelemetry> },
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
    /// Whole boards rather than one decision each: a client watching a
    /// tree being built needs the tree, and a board is small enough that
    /// sending the whole of it is cheaper than teaching both sides how to
    /// splice a node into one.
    decisions: broadcast::Sender<DecisionBoard>,
    /// One feature's task list, whole for the same reason.
    tasks: broadcast::Sender<TaskList>,
    diagrams: broadcast::Sender<DiagramList>,
    /// A tick whenever a client's view changes or goes, for the
    /// connections watching the focus to read it again.
    focus: broadcast::Sender<()>,
    views: broadcast::Sender<ViewEvent>,
    saved_views: broadcast::Sender<ServerMsg>,
    extensions: broadcast::Sender<ExtensionNews>,
}

impl Pushes {
    pub(super) fn new() -> Pushes {
        Pushes {
            tree: broadcast::channel(TREE_DEPTH).0,
            tree_order: StdMutex::new(()),
            workspaces: broadcast::channel(WORKSPACES_DEPTH).0,
            decisions: broadcast::channel(DECISIONS_DEPTH).0,
            tasks: broadcast::channel(TASKS_DEPTH).0,
            diagrams: broadcast::channel(DIAGRAMS_DEPTH).0,
            focus: broadcast::channel(FOCUS_DEPTH).0,
            views: broadcast::channel(VIEWS_DEPTH).0,
            saved_views: broadcast::channel(SAVED_VIEWS_DEPTH).0,
            extensions: broadcast::channel(EXTENSIONS_DEPTH).0,
        }
    }

    /// The tree `snapshot` takes, read and sent in `tree_order`.
    pub(super) fn tree(&self, snapshot: impl FnOnce() -> Vec<ProjectInfo>) {
        let _order = self.tree_order.lock().unwrap();
        let _ = self.tree.send(TreeNews::Tree(snapshot()));
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

    pub(super) fn decisions(&self, board: DecisionBoard) {
        let _ = self.decisions.send(board);
    }

    pub(super) fn tasks(&self, list: TaskList) {
        let _ = self.tasks.send(list);
    }

    pub(super) fn diagrams(&self, list: DiagramList) {
        let _ = self.diagrams.send(list);
    }

    pub(super) fn focus_moved(&self) {
        let _ = self.focus.send(());
    }

    /// Sent under the views lock, so a watcher's replay and the changes
    /// after it do not cross.
    pub(super) fn view(&self, event: ViewEvent) {
        let _ = self.views.send(event);
    }

    /// Sent under the saved-views lock, for the same reason.
    pub(super) fn saved_view(&self, msg: ServerMsg) {
        let _ = self.saved_views.send(msg);
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

    pub(super) fn subscribe_workspaces(&self) -> broadcast::Receiver<Vec<WorkspaceInfo>> {
        self.workspaces.subscribe()
    }

    pub(super) fn subscribe_decisions(&self) -> broadcast::Receiver<DecisionBoard> {
        self.decisions.subscribe()
    }

    pub(super) fn subscribe_tasks(&self) -> broadcast::Receiver<TaskList> {
        self.tasks.subscribe()
    }

    pub(super) fn subscribe_diagrams(&self) -> broadcast::Receiver<DiagramList> {
        self.diagrams.subscribe()
    }

    pub(super) fn subscribe_focus(&self) -> broadcast::Receiver<()> {
        self.focus.subscribe()
    }

    /// Only under the views lock: see `Daemon::watch_views`.
    pub(super) fn subscribe_views(&self) -> broadcast::Receiver<ViewEvent> {
        self.views.subscribe()
    }

    /// Only under the saved-views lock: see `Daemon::watch_saved_views`.
    pub(super) fn subscribe_saved_views(&self) -> broadcast::Receiver<ServerMsg> {
        self.saved_views.subscribe()
    }

    pub(super) fn subscribe_extensions(&self) -> broadcast::Receiver<ExtensionNews> {
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
impl super::Daemon {
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
}

#[cfg(test)]
mod tests;
