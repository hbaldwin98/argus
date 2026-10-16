//! What the daemon publishes for its connections to push: one channel per
//! topic, each as deep as its bursts.
//!
//! Every push goes through here, so a send site elsewhere that names a
//! `broadcast::Sender` is one this module does not know about. A send with
//! no receiver is not an error: no client need be connected.

use argus_protocol::{
    AgentTelemetry, DecisionBoard, DiagramList, PaneId, ProjectInfo, ServerMsg, TaskList, WorkspaceInfo,
};
use tokio::sync::broadcast;

use super::views::ViewEvent;
use super::ExtensionNews;

/// Trees come at structural changes and each poll, never at pane output.
const TREE_DEPTH: usize = 32;
/// A report comes with every tool call, so its bursts are the deepest.
const TELEMETRY_DEPTH: usize = 64;
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

pub struct Pushes {
    tree: broadcast::Sender<Vec<ProjectInfo>>,
    /// An agent's merged telemetry after each report. Apart from the tree
    /// because it changes on every tool call, and a client that can take
    /// one pane's record should not be sent every pane's.
    telemetry: broadcast::Sender<(PaneId, AgentTelemetry)>,
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
            telemetry: broadcast::channel(TELEMETRY_DEPTH).0,
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

    pub(super) fn tree(&self, tree: Vec<ProjectInfo>) {
        let _ = self.tree.send(tree);
    }

    pub(super) fn telemetry(&self, pane: PaneId, telemetry: AgentTelemetry) {
        let _ = self.telemetry.send((pane, telemetry));
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

    pub(super) fn subscribe_tree(&self) -> broadcast::Receiver<Vec<ProjectInfo>> {
        self.tree.subscribe()
    }

    pub(super) fn subscribe_telemetry(&self) -> broadcast::Receiver<(PaneId, AgentTelemetry)> {
        self.telemetry.subscribe()
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
