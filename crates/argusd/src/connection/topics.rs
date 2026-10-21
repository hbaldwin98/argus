//! What a connection is pushed: for each topic, what joining it sends, what
//! each change sends, and how a client that fell behind is brought level —
//! each forwarded on a task of its own, one way for all of them.
//!
//! Order is kept within a topic and nowhere else: several tasks share the
//! queue to the writer, so the `Consoles` that follows a `Tree` may have
//! another topic's message between them, and no client relies on it.

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use argus_protocol::{ServerMsg, WorkspaceInfo, CONSOLES, PANE_TELEMETRY};
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::mpsc;

use super::Peer;
use crate::state::{BoardNews, BrowserNews, Daemon, ExtensionNews, SavedViewNews, TreeNews, ViewNews, ViewerId};

/// One thing a client can be pushed. The capability it needs is checked
/// where it is joined; a client that never listed one never joins.
pub(super) trait Topic: Send + 'static {
    type News: Clone + Send + 'static;
    /// What joining sends, and the receiver of every change after it, taken
    /// so that no change falls between them.
    fn join(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<Self::News>);
    /// What one change sends this client; often nothing.
    fn news(&mut self, daemon: &Daemon, news: Self::News) -> Vec<ServerMsg>;
    /// What a client that missed changes is sent to be level again, and the
    /// receiver it follows from there: never the lagged one, whose backlog
    /// would follow fresh state with stale. No default: every topic says.
    fn caught_up(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<Self::News>);
}

/// The topics a connection has joined, each forwarded on its own task,
/// all stopped when the connection goes.
pub(super) struct Topics {
    daemon: Arc<Daemon>,
    out: mpsc::UnboundedSender<ServerMsg>,
    running: HashMap<TypeId, tokio::task::JoinHandle<()>>,
}

impl Topics {
    pub(super) fn new(daemon: Arc<Daemon>, out: mpsc::UnboundedSender<ServerMsg>) -> Topics {
        Topics { daemon, out, running: HashMap::new() }
    }

    /// Joins `topic`. What joining sends is queued before this returns, so
    /// it lands ahead of anything sent after. Joining a topic again replaces
    /// it: the old forwarder is stopped first, then the new one joins.
    pub(super) fn join<T: Topic>(&mut self, mut topic: T) {
        if let Some(old) = self.running.remove(&TypeId::of::<T>()) {
            old.abort();
        }
        let (sent, rx) = topic.join(&self.daemon);
        for msg in sent {
            let _ = self.out.send(msg);
        }
        let forwarder = forward(topic, self.daemon.clone(), self.out.clone(), rx);
        self.running.insert(TypeId::of::<T>(), tokio::spawn(forwarder));
    }

    /// Leaves `T` if it was joined: a client that greets again without
    /// the capability a topic needs is pushed none of it from then on.
    pub(super) fn leave<T: Topic>(&mut self) {
        if let Some(old) = self.running.remove(&TypeId::of::<T>()) {
            old.abort();
        }
    }

    #[cfg(test)]
    pub(super) fn joined<T: Topic>(&self) -> bool {
        self.running.contains_key(&TypeId::of::<T>())
    }

    /// Stops forwarding every topic: before a shutdown's ack is queued, so
    /// nothing is pushed after it, and when the connection goes.
    pub(super) fn stop(&mut self) {
        for (_, task) in self.running.drain() {
            task.abort();
        }
    }
}

impl Drop for Topics {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn forward<T: Topic>(
    mut topic: T,
    daemon: Arc<Daemon>,
    out: mpsc::UnboundedSender<ServerMsg>,
    mut rx: broadcast::Receiver<T::News>,
) {
    loop {
        let sent = match rx.recv().await {
            Ok(news) => topic.news(&daemon, news),
            Err(RecvError::Lagged(_)) => {
                let (sent, fresh) = topic.caught_up(&daemon);
                rx = fresh;
                sent
            }
            // The daemon holds every sender for as long as it runs.
            Err(RecvError::Closed) => return,
        };
        for msg in sent {
            if out.send(msg).is_err() {
                return;
            }
        }
    }
}

/// The tree and each pane's telemetry, with the consoles beside every tree
/// for a client that takes them. Joined before the greeting, so what the
/// client can take is read as each change is sent.
pub(super) struct Tree {
    pub(super) peer: Peer,
}

impl Tree {
    fn sent(&self, daemon: &Daemon, tree: Vec<argus_protocol::ProjectInfo>) -> Vec<ServerMsg> {
        let mut sent = vec![ServerMsg::Tree(tree)];
        if self.peer.can(CONSOLES) {
            sent.push(ServerMsg::Consoles(daemon.consoles()));
        }
        sent
    }
}

impl Topic for Tree {
    type News = TreeNews;

    fn join(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<TreeNews>) {
        let (tree, rx) = daemon.pushes().follow_tree(|| daemon.snapshot());
        (self.sent(daemon, tree), rx)
    }

    fn news(&mut self, daemon: &Daemon, news: TreeNews) -> Vec<ServerMsg> {
        match news {
            TreeNews::Tree(tree) => self.sent(daemon, tree),
            TreeNews::Telemetry { pane, telemetry } if self.peer.can(PANE_TELEMETRY) => {
                vec![ServerMsg::PaneTelemetry { pane, telemetry: *telemetry }]
            }
            // A client that cannot take one pane's record is sent them all
            // in a tree.
            TreeNews::Telemetry { .. } => vec![ServerMsg::Tree(daemon.snapshot())],
        }
    }

    /// The tree, which holds every record.
    fn caught_up(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<TreeNews>) {
        self.join(daemon)
    }
}

/// Every workspace with its rollups, whole each time.
pub(super) struct Workspaces;

impl Topic for Workspaces {
    type News = Vec<WorkspaceInfo>;

    fn join(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<Vec<WorkspaceInfo>>) {
        let rx = daemon.pushes().subscribe_workspaces();
        (vec![ServerMsg::Workspaces(daemon.workspaces())], rx)
    }

    fn news(&mut self, _: &Daemon, all: Vec<WorkspaceInfo>) -> Vec<ServerMsg> {
        vec![ServerMsg::Workspaces(all)]
    }

    fn caught_up(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<Vec<WorkspaceInfo>>) {
        self.join(daemon)
    }
}

/// Every feature board's pushes. Every client is told, and one reading
/// another repository's or feature's board drops it by the repositories it
/// names: the view a client reports is for saying where the person is,
/// never for deciding what it is sent.
pub(super) struct Boards;

impl Topic for Boards {
    type News = BoardNews;

    /// Nothing: a client asks for the boards it shows.
    fn join(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<BoardNews>) {
        (Vec::new(), daemon.pushes().subscribe_boards())
    }

    fn news(&mut self, _: &Daemon, board: BoardNews) -> Vec<ServerMsg> {
        vec![board_msg(board)]
    }

    /// Each board's latest push.
    fn caught_up(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<BoardNews>) {
        let (latest, rx) = daemon.pushes().follow_boards();
        (latest.into_iter().map(board_msg).collect(), rx)
    }
}

fn board_msg(board: BoardNews) -> ServerMsg {
    match board {
        BoardNews::Decisions(board) => ServerMsg::Decisions(board),
        BoardNews::Tasks(list) => ServerMsg::Tasks(list),
        BoardNews::Diagrams(list) => ServerMsg::SequenceDiagrams(list),
    }
}

/// Where the person is working, for a client that asked. `sent` is what it
/// was last told, so a tick that moves nothing it names sends nothing.
#[derive(Default)]
pub(super) struct Focus {
    sent: Option<argus_protocol::Focus>,
}

impl Focus {
    fn now(daemon: &Daemon) -> Option<argus_protocol::Focus> {
        daemon.focus_in(&daemon.snapshot())
    }

    fn changed(&mut self, focus: Option<argus_protocol::Focus>) -> Vec<ServerMsg> {
        if focus == self.sent {
            return Vec::new();
        }
        self.sent = focus.clone();
        vec![ServerMsg::Focus(focus.map(Box::new))]
    }
}

impl Topic for Focus {
    type News = ();

    /// Sent whatever it is: the answer to asking.
    fn join(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<()>) {
        let rx = daemon.pushes().subscribe_focus();
        let focus = Focus::now(daemon);
        self.sent = focus.clone();
        (vec![ServerMsg::Focus(focus.map(Box::new))], rx)
    }

    fn news(&mut self, daemon: &Daemon, _: ()) -> Vec<ServerMsg> {
        self.changed(Focus::now(daemon))
    }

    /// The same as a change: the focus is read whole either way.
    fn caught_up(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<()>) {
        let rx = daemon.pushes().subscribe_focus();
        (self.changed(Focus::now(daemon)), rx)
    }
}

/// The views agents show, for a client that asked.
pub(super) struct Views;

impl Topic for Views {
    type News = ViewNews;

    /// Every view as it stands, replayed under the lock its changes are
    /// sent under.
    fn join(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<ViewNews>) {
        let (shown, rx) = daemon.watch_views();
        (shown.into_iter().map(|view| ServerMsg::ViewShown(Box::new(view))).collect(), rx)
    }

    fn news(&mut self, _: &Daemon, news: ViewNews) -> Vec<ServerMsg> {
        vec![match news {
            ViewNews::Shown(view) => ServerMsg::ViewShown(view),
            ViewNews::Closed { pane, id } => ServerMsg::ViewClosed { pane, id },
        }]
    }

    /// The replay again. A view closed in the gap is the one thing it
    /// cannot say, and the next change to it will.
    fn caught_up(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<ViewNews>) {
        self.join(daemon)
    }
}

/// The browsers agents' panes run, for a client that listed `BROWSERS`
/// and asked: where each is, and the pictures of those it draws.
pub(super) struct Browsers {
    pub(super) showing: std::collections::HashSet<argus_protocol::PaneId>,
}

impl Browsers {
    fn wanted(&self, news: &BrowserNews) -> bool {
        match news {
            BrowserNews::Frame(frame) => self.showing.contains(&frame.pane),
            BrowserNews::Page(_) | BrowserNews::Closed { .. } => true,
        }
    }
}

impl Topic for Browsers {
    type News = BrowserNews;

    fn join(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<BrowserNews>) {
        let (now, rx) = daemon.watch_browsers();
        (now.into_iter().filter(|news| self.wanted(news)).map(BrowserNews::message).collect(), rx)
    }

    fn news(&mut self, _: &Daemon, news: BrowserNews) -> Vec<ServerMsg> {
        if !self.wanted(&news) {
            return Vec::new();
        }
        vec![news.message()]
    }

    /// The replay again: each browser's latest picture, never the
    /// pictures missed, which are already stale.
    fn caught_up(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<BrowserNews>) {
        self.join(daemon)
    }
}

/// The saved views, for a client that listed `SAVED_VIEWS` and asked.
pub(super) struct SavedViews;

impl Topic for SavedViews {
    type News = SavedViewNews;

    fn join(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<SavedViewNews>) {
        let (all, rx) = daemon.watch_saved_views();
        (vec![ServerMsg::SavedViews(all)], rx)
    }

    fn news(&mut self, _: &Daemon, news: SavedViewNews) -> Vec<ServerMsg> {
        vec![match news {
            SavedViewNews::Changed(view) => ServerMsg::SavedViewChanged(view),
            SavedViewNews::Removed { id } => ServerMsg::SavedViewRemoved { id },
        }]
    }

    /// The list whole, which replaces what the client held, so a removal
    /// it missed goes with it.
    fn caught_up(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<SavedViewNews>) {
        self.join(daemon)
    }
}

/// What the extensions publish, for a client that greeted listing
/// `EXTENSIONS`, and the dialogs put to this one.
pub(super) struct Extensions {
    pub(super) viewer: ViewerId,
}

fn extensions_whole(daemon: &Daemon) -> Vec<ServerMsg> {
    vec![
        ServerMsg::ExtensionViews(daemon.extension_views()),
        ServerMsg::ExtensionCommands(daemon.extension_commands()),
    ]
}

impl Topic for Extensions {
    type News = ExtensionNews;

    fn join(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<ExtensionNews>) {
        let rx = daemon.pushes().subscribe_extensions();
        (extensions_whole(daemon), rx)
    }

    fn news(&mut self, daemon: &Daemon, news: ExtensionNews) -> Vec<ServerMsg> {
        match news {
            ExtensionNews::Views => vec![ServerMsg::ExtensionViews(daemon.extension_views())],
            ExtensionNews::Commands => vec![ServerMsg::ExtensionCommands(daemon.extension_commands())],
            ExtensionNews::Notice { extension, text } => vec![ServerMsg::ExtensionNotice { extension, text }],
            ExtensionNews::Dialog { viewer, extension, id, dialog, prompted } if viewer == self.viewer => {
                vec![ServerMsg::ExtensionDialog { extension, id, dialog: *dialog, prompted }]
            }
            ExtensionNews::Withdrawn { viewer, id } if viewer == self.viewer => {
                vec![ServerMsg::ExtensionDialogWithdrawn { id }]
            }
            ExtensionNews::Dialog { .. } | ExtensionNews::Withdrawn { .. } => Vec::new(),
        }
    }

    /// The views and commands whole, and every dialog still put to this
    /// client again, oldest first. One withdrawn in the gap stays on the
    /// client until it is answered, and the answer is refused.
    fn caught_up(&mut self, daemon: &Daemon) -> (Vec<ServerMsg>, broadcast::Receiver<ExtensionNews>) {
        let (mut sent, rx) = self.join(daemon);
        for news in daemon.dialogs_put_to(self.viewer) {
            sent.extend(self.news(daemon, news));
        }
        (sent, rx)
    }
}

#[cfg(test)]
mod tests;
