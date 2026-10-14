//! What this window holds of the daemon — the tree, the pane on screen,
//! the connection's state — and what each message and gesture changes.
//!
//! Plain Rust with no Tauri in it: each daemon message and each gesture the
//! page reports goes in, and out come the messages to send the daemon. The
//! page's events are taken afterwards (`events`, `take_frame`), so however
//! many things changed in between, the page hears each once.
//!
//! The client never predicts: the rail changes when the daemon's tree does,
//! and a shell asked for is shown only once the daemon has said it made it
//! and a tree holds it. Which pane is on screen is this window's own choice,
//! as the terminal client's selection is. The Feature stage's own state is
//! `feature_stage`'s; the session tells it which repository it is about.

use std::collections::HashSet;

use argus_protocol::{
    CheckoutId, ClientMsg, Created, Hello, HeldTree, PaneId, ProjectId, ProjectInfo, RepositoryId,
    ServerMsg, View, FOCUS,
};

use crate::feature_stage::{FeatureStage, Scope};
use crate::features::Layout;
use crate::keys::{self, PageKey};
use crate::page::{Connection, Event, Status};
use crate::settings::Settings;
use crate::rail::{self, Choices};
use crate::screen::Screen;

/// The most spawn requests held unanswered. A daemon older than the answer
/// never sends one, and a request nobody will answer should not be carried
/// for the rest of the session.
const MAX_ASKED: usize = 32;

pub struct Session {
    tree: Vec<ProjectInfo>,
    /// Whether any daemon has sent a tree yet.
    loaded: bool,
    daemon: Option<Hello>,
    connection: Connection,
    detail: Option<String>,
    alarm: Option<String>,
    project: Option<ProjectId>,
    /// The repository the rail last had chosen, whose features the Feature
    /// stage reads.
    repository: Option<RepositoryId>,
    toggled: HashSet<RepositoryId>,
    features: FeatureStage,
    shown: Option<PaneId>,
    screen: Option<Screen>,
    /// The rows and columns the page has room for.
    capacity: Option<(u16, u16)>,
    /// The size last asked for the shown pane on this connection.
    sized: Option<(u16, u16)>,
    last_request: u64,
    /// Spawns asked for and not yet answered.
    asked: Vec<u64>,
    /// A pane the daemon made for us that no tree has carried yet.
    made: Option<PaneId>,
    focused: bool,
    /// The view last reported, so an unchanged one is not sent again.
    reported: Option<View>,
    status_changed: bool,
    rail_changed: bool,
    passing: Vec<Event>,
    out: Vec<ClientMsg>,
}

impl Default for Session {
    fn default() -> Self {
        Session::new(Settings::default())
    }
}

impl Session {
    pub fn new(settings: Settings) -> Session {
        Session {
            tree: Vec::new(),
            loaded: false,
            daemon: None,
            connection: Connection::Connecting,
            detail: None,
            alarm: None,
            project: None,
            repository: None,
            toggled: HashSet::new(),
            features: FeatureStage::new(settings),
            shown: None,
            screen: None,
            capacity: None,
            sized: None,
            last_request: 0,
            asked: Vec::new(),
            made: None,
            focused: true,
            reported: None,
            status_changed: true,
            rail_changed: true,
            passing: Vec::new(),
            out: Vec::new(),
        }
    }

    // ---- The connection ----------------------------------------------------

    /// Trying to reach the daemon. The last tree stays on screen meanwhile;
    /// `detail` is what the last failed try said.
    pub fn connecting(&mut self, detail: Option<String>) {
        self.connection = Connection::Connecting;
        self.detail = detail;
        self.daemon = None;
        self.status_changed = true;
    }

    /// Someone stopped the daemon: nothing reconnects until the page asks.
    pub fn stopped(&mut self) {
        self.connection = Connection::Stopped;
        self.daemon = None;
        self.status_changed = true;
    }

    pub fn is_stopped(&self) -> bool {
        self.connection == Connection::Stopped
    }

    /// A daemon answered. `daemon` is its greeting, `None` from one that
    /// said nothing or predates the greeting; `alarm` says it speaks another
    /// protocol. The `opening` is what it sent before its greeting.
    pub fn connected(
        &mut self,
        daemon: Option<Hello>,
        alarm: Option<String>,
        opening: Vec<ServerMsg>,
    ) -> Vec<ClientMsg> {
        self.connection = Connection::Connected;
        self.detail = None;
        self.alarm = alarm.or_else(|| daemon.as_ref().and_then(another_build));
        self.daemon = daemon;
        self.status_changed = true;
        // A new connection knows nothing this window asked of the last.
        self.sized = None;
        self.reported = None;
        self.asked.clear();
        self.features.forget_asked();
        for msg in opening {
            self.apply(msg);
        }
        if let Some(pane) = self.shown {
            self.screen = Some(Screen::new(pane));
            self.out.push(ClientMsg::Subscribe { pane });
            self.send_size();
        }
        self.finish()
    }

    /// A message from the daemon.
    pub fn receive(&mut self, msg: ServerMsg) -> Vec<ClientMsg> {
        self.apply(msg);
        self.finish()
    }

    fn apply(&mut self, msg: ServerMsg) {
        if let Some(screen) = &mut self.screen {
            if screen.apply(&msg) {
                return;
            }
        }
        match msg {
            ServerMsg::Tree(tree) => self.take_tree(tree),
            ServerMsg::PaneTelemetry { pane, telemetry } => {
                // The rail says nothing of telemetry; the tree stays true.
                self.tree.as_mut_slice().apply_telemetry(pane, telemetry);
            }
            ServerMsg::Created { request_id, created } => {
                let Some(at) = self.asked.iter().position(|id| *id == request_id) else {
                    return;
                };
                self.asked.remove(at);
                if let Some(Created::Pane(pane)) = created {
                    self.made = Some(pane);
                    self.show_made();
                }
            }
            ServerMsg::Clipboard { pane, text } if Some(pane) == self.shown => {
                self.passing.push(Event::Clipboard { text });
            }
            ServerMsg::Error { message } => self.passing.push(Event::Notice { text: message }),
            ServerMsg::Hello(hello) => {
                // A greeting that arrived after the window stopped waiting.
                self.alarm = self.alarm.take().or_else(|| another_build(&hello));
                self.daemon = Some(hello);
                self.status_changed = true;
            }
            ServerMsg::Restarting => self.passing.push(Event::Notice {
                text: "argusd is restarting".into(),
            }),
            ServerMsg::Decisions(board) => {
                let scope = self.scope();
                self.features.receive_board(*board, scope.as_ref(), &self.tree, &mut self.out);
            }
            ServerMsg::Tasks(list) => {
                let scope = self.scope();
                self.features.receive_tasks(*list, scope.as_ref());
            }
            // The tree that follows says what closed and what stopping
            // ends; the rest answers requests this window does not make yet.
            _ => {}
        }
    }

    fn take_tree(&mut self, tree: Vec<ProjectInfo>) {
        self.tree = tree;
        self.loaded = true;
        self.rail_changed = true;
        self.features.tree_changed();
        if !self.tree.iter().any(|p| Some(p.id) == self.project) {
            self.project = self.tree.first().map(|p| p.id);
        }
        // A pane that is gone is gone from the daemon's subscriptions too.
        if let Some(pane) = self.shown {
            if self.project_of(pane).is_none() {
                self.shown = None;
                self.screen = None;
                self.sized = None;
            }
        }
        self.show_made();
    }

    fn show_made(&mut self) {
        let Some(pane) = self.made else {
            return;
        };
        if self.project_of(pane).is_some() {
            self.made = None;
            self.show(pane);
        }
    }

    // ---- Gestures ----------------------------------------------------------

    /// Puts a pane on screen, leaving the Feature stage for it: a pane
    /// chosen on the rail is one the person means to look at and type into.
    pub fn select(&mut self, pane: PaneId) -> Vec<ClientMsg> {
        if self.project_of(pane).is_some() && self.features.is_open() {
            self.features.set_open(false);
            self.rail_changed = true;
        }
        self.show(pane);
        self.finish()
    }

    fn show(&mut self, pane: PaneId) {
        if self.shown == Some(pane) {
            return;
        }
        let Some(project) = self.project_of(pane) else {
            return;
        };
        self.hide();
        self.project = Some(project);
        self.repository = None;
        self.shown = Some(pane);
        self.screen = Some(Screen::new(pane));
        self.rail_changed = true;
        self.out.push(ClientMsg::Subscribe { pane });
        self.send_size();
    }

    fn hide(&mut self) {
        if let Some(pane) = self.shown.take() {
            self.out.push(ClientMsg::Unsubscribe { pane });
        }
        self.screen = None;
        self.sized = None;
        self.rail_changed = true;
    }

    /// The room the page has for a screen. The daemon sizes a pane to the
    /// client the person was last at, so while another is that client the
    /// grid may come back another size, and the page pads or crops it.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Vec<ClientMsg> {
        self.capacity = Some((rows.max(1), cols.max(1)));
        self.send_size();
        self.finish()
    }

    fn send_size(&mut self) {
        let (Some(pane), Some(size)) = (self.shown, self.capacity) else {
            return;
        };
        if self.sized == Some(size) {
            return;
        }
        self.sized = Some(size);
        self.out.push(ClientMsg::Resize { pane, rows: size.0, cols: size.1 });
    }

    pub fn key(&mut self, key: &PageKey) -> Vec<ClientMsg> {
        let bytes = keys::to_key(key).map(|k| argus_protocol::keys::encode(&k)).unwrap_or_default();
        self.input(bytes);
        self.finish()
    }

    /// Text an input method composed, typed as it stands.
    pub fn text(&mut self, text: &str) -> Vec<ClientMsg> {
        self.input(text.as_bytes().to_vec());
        self.finish()
    }

    fn input(&mut self, bytes: Vec<u8>) {
        if let (Some(pane), false) = (self.shown, bytes.is_empty()) {
            self.out.push(ClientMsg::Input { pane, bytes });
        }
    }

    /// A paste, as one event: the daemon brackets it only if the child asked.
    pub fn paste(&mut self, text: String) -> Vec<ClientMsg> {
        if let (Some(pane), false) = (self.shown, text.is_empty()) {
            self.out.push(ClientMsg::Paste { pane, text });
        }
        self.finish()
    }

    /// The shown pane's text between two cells; empty with no pane.
    pub fn selected_text(&self, from: (u16, u16), to: (u16, u16)) -> String {
        self.screen.as_ref().map(|screen| screen.text(from, to)).unwrap_or_default()
    }

    /// Something this window says on the bar a while, as it does the
    /// daemon's refusals.
    pub fn note(&mut self, text: impl Into<String>) -> Vec<ClientMsg> {
        self.passing.push(Event::Notice { text: text.into() });
        self.finish()
    }

    /// Asks for a shell on a checkout, shown once it exists.
    pub fn spawn_shell(&mut self, checkout: CheckoutId) -> Vec<ClientMsg> {
        let exists = self
            .tree
            .iter()
            .flat_map(|p| &p.repositories)
            .flat_map(|r| &r.checkouts)
            .any(|c| c.id == checkout);
        if exists {
            self.last_request = self.last_request.wrapping_add(1).max(1);
            if self.asked.len() == MAX_ASKED {
                self.asked.remove(0);
            }
            self.asked.push(self.last_request);
            self.out.push(ClientMsg::SpawnShell { checkout, request_id: self.last_request });
        }
        self.finish()
    }

    /// The window gained or lost focus. Only a focused window's view takes
    /// the person's focus, so one in the background never pulls it away
    /// from a terminal client they are typing in.
    pub fn focus(&mut self, focused: bool) -> Vec<ClientMsg> {
        self.focused = focused;
        self.finish()
    }

    pub fn select_project(&mut self, project: ProjectId) -> Vec<ClientMsg> {
        if self.project != Some(project) && self.tree.iter().any(|p| p.id == project) {
            self.project = Some(project);
            self.hide();
        }
        self.finish()
    }

    pub fn toggle_repository(&mut self, repository: RepositoryId) -> Vec<ClientMsg> {
        if !self.toggled.remove(&repository) {
            self.toggled.insert(repository);
        }
        self.rail_changed = true;
        self.finish()
    }

    /// A click on a repository's row: the first chooses it, for the
    /// Feature stage to read, and opens it; a second on the chosen one
    /// opens or closes it.
    pub fn choose_repository(&mut self, repository: RepositoryId) -> Vec<ClientMsg> {
        let Some(repo) = self.current_project().and_then(|p| p.repositories.iter().find(|r| r.id == repository)) else {
            return self.finish();
        };
        if self.chosen_repository().map(|r| r.id) == Some(repository) {
            return self.toggle_repository(repository);
        }
        let quiet = !repo.checkouts.iter().any(|c| c.listed_panes().next().is_some());
        let open = quiet == self.toggled.contains(&repository);
        if !open {
            self.toggle_repository(repository);
        }
        self.repository = Some(repository);
        self.rail_changed = true;
        self.finish()
    }

    // ---- The Feature stage ---------------------------------------------------

    /// Shows the Feature stage, or goes back to the Workspace.
    pub fn show_features(&mut self, open: bool) -> Vec<ClientMsg> {
        if self.features.is_open() != open {
            self.features.set_open(open);
            self.rail_changed = true;
        }
        self.finish()
    }

    pub fn peek(&mut self, slug: &str) -> Vec<ClientMsg> {
        let scope = self.scope();
        self.features.peek(slug, scope.as_ref(), &mut self.out);
        self.finish()
    }

    pub fn set_layout(&mut self, layout: Layout) -> Vec<ClientMsg> {
        let scope = self.scope();
        self.features.set_layout(layout, scope.as_ref());
        self.finish()
    }

    pub fn toggle_folded(&mut self) -> Vec<ClientMsg> {
        let scope = self.scope();
        self.features.toggle_folded(scope.as_ref(), &self.tree, &mut self.out);
        self.finish()
    }

    pub fn hold(&mut self, slug: &str, reason: Option<String>) -> Vec<ClientMsg> {
        let scope = self.scope();
        self.features.hold(slug, reason, scope.as_ref(), &mut self.out);
        self.finish()
    }

    pub fn archive(&mut self, slug: &str, archived: bool) -> Vec<ClientMsg> {
        let scope = self.scope();
        self.features.archive(slug, archived, scope.as_ref(), &mut self.out);
        self.finish()
    }

    /// Leaves the Feature stage for a pane working a feature.
    pub fn go_to_pane(&mut self, pane: PaneId) -> Vec<ClientMsg> {
        if self.project_of(pane).is_some() {
            self.features.set_open(false);
            self.show(pane);
        }
        self.finish()
    }

    /// The settings, when a change asks for them to be saved.
    pub fn take_settings(&mut self) -> Option<Settings> {
        self.features.take_settings()
    }

    fn current_project(&self) -> Option<&ProjectInfo> {
        self.tree.iter().find(|p| Some(p.id) == self.project)
    }

    /// The repository the Feature stage reads: the one chosen on the rail,
    /// else the one holding the pane on screen, else the first with
    /// anything running, else the first.
    fn chosen_repository(&self) -> Option<&argus_protocol::RepositoryInfo> {
        let project = self.current_project()?;
        let by_id = |id: RepositoryId| project.repositories.iter().find(|r| r.id == id);
        let holding_shown = self.shown.and_then(|pane| {
            project
                .repositories
                .iter()
                .find(|r| r.checkouts.iter().any(|c| c.panes.iter().any(|p| p.id == pane)))
        });
        let (active, _) = project.repositories_by_activity();
        self.repository
            .and_then(by_id)
            .or(holding_shown)
            .or_else(|| active.first().map(|i| &project.repositories[*i]))
            .or_else(|| project.repositories.first())
    }

    fn scope(&self) -> Option<Scope> {
        let project = self.current_project()?;
        let repo = self.chosen_repository()?;
        let checkout = repo.checkouts.iter().find(|c| c.primary).or_else(|| repo.checkouts.first())?;
        Some(Scope {
            project: project.id,
            project_name: project.name.clone(),
            repository: repo.id,
            repository_name: repo.name.clone(),
            checkout: checkout.id,
            key: checkout.path.clone(),
        })
    }

    // ---- What goes out -----------------------------------------------------

    /// The messages this change sends, with the view when it changed.
    fn finish(&mut self) -> Vec<ClientMsg> {
        // Whatever changed may have changed which board the stage reads.
        let scope = self.scope();
        if self.connection == Connection::Connected {
            self.features.refresh(scope.as_ref(), &mut self.out);
        }
        if self.daemon.as_ref().is_some_and(|d| d.can(FOCUS)) {
            let view = self.view();
            if self.reported.as_ref() != Some(&view) {
                self.reported = Some(view.clone());
                self.out.push(ClientMsg::View(view));
            }
        }
        std::mem::take(&mut self.out)
    }

    fn view(&self) -> View {
        let checkout = self.shown.and_then(|pane| {
            self.tree
                .iter()
                .flat_map(|p| &p.repositories)
                .flat_map(|r| &r.checkouts)
                .find(|c| c.panes.iter().any(|p| p.id == pane))
                .map(|c| c.id)
        });
        View {
            project: self.project,
            checkout,
            pane: self.shown,
            active: self.focused,
        }
    }

    /// What the page has not heard yet: the status and the rail when they
    /// changed, and whatever passed through. `now` is seconds since the
    /// Unix epoch.
    pub fn events(&mut self, now: u64) -> Vec<Event> {
        let mut events = Vec::new();
        if std::mem::take(&mut self.status_changed) {
            events.push(Event::Status(self.status()));
        }
        if std::mem::take(&mut self.rail_changed) {
            events.push(Event::Rail(Box::new(self.rail(now))));
        }
        let scope = self.scope();
        if let Some(features) = self.features.take(scope.as_ref(), &self.tree) {
            events.push(Event::Features(Box::new(features)));
        }
        events.append(&mut self.passing);
        events
    }

    /// Everything, for a page that has just loaded and holds nothing; the
    /// shown pane's whole screen follows with the next frame.
    pub fn everything(&mut self, now: u64) -> Vec<Event> {
        self.status_changed = true;
        self.rail_changed = true;
        if let Some(screen) = &mut self.screen {
            screen.refresh();
        }
        let mut events = self.events(now);
        if !events.iter().any(|e| matches!(e, Event::Features(_))) {
            let scope = self.scope();
            events.push(Event::Features(Box::new(self.features.everything(scope.as_ref(), &self.tree))));
        }
        events
    }

    pub fn take_frame(&mut self) -> Option<Event> {
        self.screen.as_mut()?.take_frame().map(Event::Frame)
    }

    fn status(&self) -> Status {
        Status {
            connection: self.connection,
            detail: self.detail.clone(),
            alarm: self.alarm.clone(),
        }
    }

    fn rail(&self, now: u64) -> crate::page::Rail {
        let choices = Choices {
            project: self.project,
            shown: self.shown,
            toggled: &self.toggled,
            now,
        };
        let mut rail = rail::build(&self.tree, &choices, self.loaded);
        rail.repository = self.chosen_repository().map(|r| r.id.0);
        rail.stage = if self.features.is_open() { "feature" } else { "workspace" };
        rail
    }

    fn project_of(&self, pane: PaneId) -> Option<ProjectId> {
        self.tree
            .iter()
            .find(|p| {
                p.repositories
                    .iter()
                    .flat_map(|r| &r.checkouts)
                    .any(|c| c.panes.iter().any(|p| p.id == pane))
            })
            .map(|p| p.id)
    }
}

/// What to say of a daemon from another build than this window, as the
/// terminal client says it.
fn another_build(daemon: &Hello) -> Option<String> {
    let ours = env!("CARGO_PKG_VERSION");
    (daemon.version != ours).then(|| {
        format!(
            "argusd is {}, this window {ours}: `argus server restart` brings them together",
            daemon.version
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane, project, repository};
    use argus_protocol::{PaneKind, PaneStatus};

    fn tree() -> Vec<ProjectInfo> {
        vec![
            project(1, "src", vec![repository(10, "argus", None, vec![checkout(
                100,
                "main",
                true,
                vec![
                    pane(1, PaneKind::Agent, "claude", PaneStatus::Working),
                    pane(2, PaneKind::Shell, "zsh", PaneStatus::Idle),
                ],
            )])]),
            project(2, "work", vec![repository(20, "api", None, vec![checkout(
                200,
                "main",
                true,
                vec![pane(3, PaneKind::Shell, "zsh", PaneStatus::Idle)],
            )])]),
        ]
    }

    fn with_tree(tree: Vec<ProjectInfo>) -> ServerMsg {
        ServerMsg::Tree(tree)
    }

    /// A session connected to a daemon of this build, holding `tree()`.
    fn session(daemon: Hello) -> Session {
        let mut session = Session::default();
        session.connected(Some(daemon), None, vec![with_tree(tree())]);
        session
    }

    fn without_focus() -> Hello {
        Hello { capabilities: Vec::new(), ..Hello::this_build() }
    }

    fn names(msgs: &[ClientMsg]) -> Vec<String> {
        msgs.iter()
            .map(|m| match m {
                ClientMsg::Subscribe { pane } => format!("subscribe {pane}"),
                ClientMsg::Unsubscribe { pane } => format!("unsubscribe {pane}"),
                ClientMsg::Resize { pane, rows, cols } => format!("resize {pane} {rows}x{cols}"),
                ClientMsg::Input { pane, bytes } => format!("input {pane} {bytes:?}"),
                ClientMsg::Paste { pane, text } => format!("paste {pane} {text}"),
                ClientMsg::SpawnShell { checkout, request_id } => format!("shell {checkout} #{request_id}"),
                ClientMsg::View(view) => format!("view {:?} {}", view.pane.map(|p| p.0), view.active),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn selecting_subscribes_then_sizes_and_another_unsubscribes_the_first() {
        let mut s = session(without_focus());
        s.resize(30, 100);
        assert_eq!(names(&s.select(PaneId(1))), ["subscribe 1", "resize 1 30x100"]);
        assert_eq!(names(&s.select(PaneId(1))), Vec::<String>::new(), "already shown");
        assert_eq!(
            names(&s.select(PaneId(2))),
            ["unsubscribe 1", "subscribe 2", "resize 2 30x100"]
        );
    }

    #[test]
    fn a_pane_not_in_the_tree_cannot_be_selected() {
        let mut s = session(without_focus());
        assert!(s.select(PaneId(99)).is_empty());
    }

    #[test]
    fn a_resize_is_sent_only_when_the_size_changes() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        assert_eq!(names(&s.resize(30, 100)), ["resize 1 30x100"]);
        assert!(s.resize(30, 100).is_empty());
        assert_eq!(names(&s.resize(31, 100)), ["resize 1 31x100"]);
    }

    #[test]
    fn a_reconnect_subscribes_the_shown_pane_again_at_its_size() {
        let mut s = session(without_focus());
        s.resize(30, 100);
        s.select(PaneId(1));
        s.connecting(None);
        let sent = s.connected(Some(without_focus()), None, vec![with_tree(tree())]);
        assert_eq!(names(&sent), ["subscribe 1", "resize 1 30x100"]);
    }

    #[test]
    fn a_reconnect_to_a_tree_without_the_shown_pane_lets_it_go() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        let mut gone = tree();
        gone[0].repositories[0].checkouts[0].panes.remove(0);
        let sent = s.connected(Some(without_focus()), None, vec![with_tree(gone)]);
        assert!(sent.is_empty(), "{:?}", names(&sent));
    }

    #[test]
    fn a_spawned_shell_is_shown_only_once_answered_and_in_the_tree() {
        let mut s = session(without_focus());
        assert_eq!(names(&s.spawn_shell(CheckoutId(100))), ["shell 100 #1"]);

        let created = ServerMsg::Created { request_id: 1, created: Some(Created::Pane(PaneId(7))) };
        assert!(s.receive(created).is_empty(), "no tree holds it yet");

        let mut grown = tree();
        grown[0].repositories[0].checkouts[0].panes.push(pane(7, PaneKind::Shell, "zsh", PaneStatus::Idle));
        assert_eq!(names(&s.receive(with_tree(grown))), ["subscribe 7"]);
    }

    #[test]
    fn a_tree_arriving_before_the_answer_waits_for_it() {
        let mut s = session(without_focus());
        s.spawn_shell(CheckoutId(100));
        let mut grown = tree();
        grown[0].repositories[0].checkouts[0].panes.push(pane(7, PaneKind::Shell, "zsh", PaneStatus::Idle));
        assert!(s.receive(with_tree(grown)).is_empty(), "a new row is not ours until answered");
        let created = ServerMsg::Created { request_id: 1, created: Some(Created::Pane(PaneId(7))) };
        assert_eq!(names(&s.receive(created)), ["subscribe 7"]);
    }

    #[test]
    fn an_answer_to_someone_elses_request_is_ignored() {
        let mut s = session(without_focus());
        let created = ServerMsg::Created { request_id: 5, created: Some(Created::Pane(PaneId(2))) };
        assert!(s.receive(created).is_empty());
    }

    #[test]
    fn a_view_goes_only_to_a_daemon_that_takes_one_and_follows_window_focus() {
        let mut s = session(Hello::this_build());
        assert_eq!(names(&s.select(PaneId(1))), ["subscribe 1", "view Some(1) true"]);
        assert_eq!(names(&s.focus(false)), ["view Some(1) false"]);
        assert!(s.focus(false).is_empty(), "an unchanged view is not sent again");

        let mut old = session(without_focus());
        assert!(!names(&old.select(PaneId(1))).iter().any(|m| m.starts_with("view")));
    }

    #[test]
    fn a_tree_without_the_shown_pane_drops_it_without_unsubscribing() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        let mut gone = tree();
        gone[0].repositories[0].checkouts[0].panes.remove(0);
        assert!(s.receive(with_tree(gone)).is_empty());
        assert!(s.take_frame().is_none());
        assert!(s.key(&PageKey { key: "a".into(), ..PageKey::default() }).is_empty());
    }

    #[test]
    fn keys_text_and_pastes_go_to_the_shown_pane_and_nowhere_without_one() {
        let mut s = session(without_focus());
        let a = PageKey { key: "a".into(), ..PageKey::default() };
        assert!(s.key(&a).is_empty(), "no pane, nothing typed");

        s.select(PaneId(2));
        assert_eq!(names(&s.key(&a)), ["input 2 [97]"]);
        assert!(s.key(&PageKey { key: "Shift".into(), ..PageKey::default() }).is_empty());
        assert_eq!(names(&s.text("é")), ["input 2 [195, 169]"]);
        assert_eq!(names(&s.paste("ls -la".into())), ["paste 2 ls -la"]);
        assert!(s.paste(String::new()).is_empty());
    }

    #[test]
    fn switching_project_lets_the_shown_pane_go() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        assert_eq!(names(&s.select_project(ProjectId(2))), ["unsubscribe 1"]);
        assert!(s.select_project(ProjectId(9)).is_empty(), "no such project");
    }

    #[test]
    fn selecting_a_pane_in_another_project_switches_to_it() {
        let mut s = session(without_focus());
        assert_eq!(s.view().project, Some(ProjectId(1)));
        assert_eq!(names(&s.select(PaneId(3))), ["subscribe 3"]);
        assert_eq!(s.view().project, Some(ProjectId(2)));
    }

    #[test]
    fn the_page_hears_each_change_once() {
        let mut s = session(without_focus());
        let first = s.events(0);
        assert!(
            matches!(first.as_slice(), [Event::Status(_), Event::Rail(_), Event::Features(_)]),
            "{first:?}"
        );
        assert!(s.events(0).is_empty());
        s.toggle_repository(RepositoryId(10));
        assert!(matches!(s.events(0).as_slice(), [Event::Rail(_)]));
        assert_eq!(s.everything(0).len(), 3, "a fresh page hears it all");
    }

    #[test]
    fn a_daemon_from_another_build_is_an_alarm_until_the_next_connection() {
        let mut s = Session::default();
        let other = Hello { version: "0.0.1".into(), ..Hello::this_build() };
        s.connected(Some(other), None, Vec::new());
        let Event::Status(status) = &s.events(0)[0] else { panic!() };
        assert!(status.alarm.as_deref().unwrap().contains("argus server restart"));

        s.connected(Some(Hello::this_build()), None, Vec::new());
        let Event::Status(status) = &s.events(0)[0] else { panic!() };
        assert_eq!(status.alarm, None);
    }

    #[test]
    fn errors_and_clipboards_pass_through_once() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.events(0);
        s.receive(ServerMsg::Error { message: "no".into() });
        s.receive(ServerMsg::Clipboard { pane: PaneId(1), text: "copied".into() });
        s.receive(ServerMsg::Clipboard { pane: PaneId(2), text: "not shown".into() });
        let events = s.events(0);
        assert!(matches!(
            events.as_slice(),
            [Event::Notice { text }, Event::Clipboard { text: copied }] if text == "no" && copied == "copied"
        ), "{events:?}");
    }

    #[test]
    fn opening_the_feature_stage_asks_for_the_chosen_repositorys_board() {
        let mut s = session(without_focus());
        assert!(names(&s.show_features(true)).iter().any(|m| m.starts_with("GetDecisions")));
        assert!(s.show_features(true).is_empty(), "asked once");
        let Some(Event::Rail(rail)) = s.events(0).into_iter().find(|e| matches!(e, Event::Rail(_))) else {
            panic!("the rail says which stage is open");
        };
        assert_eq!((rail.stage, rail.repository), ("feature", Some(10)));
    }

    #[test]
    fn a_pane_on_screen_chooses_its_repository_until_another_is_chosen() {
        let mut tree = tree();
        tree[0].repositories.push(crate::fixtures::repository(11, "hearth", None, vec![checkout(110, "main", true, vec![
            pane(5, PaneKind::Shell, "zsh", PaneStatus::Idle),
        ])]));
        let mut s = Session::default();
        s.connected(Some(without_focus()), None, vec![with_tree(tree)]);
        s.select(PaneId(5));
        assert_eq!(s.chosen_repository().map(|r| r.id), Some(RepositoryId(11)));
        s.choose_repository(RepositoryId(10));
        assert_eq!(s.chosen_repository().map(|r| r.id), Some(RepositoryId(10)));
        let sent = s.show_features(true);
        assert!(
            sent.iter().any(|m| matches!(m, ClientMsg::GetDecisions { checkout: CheckoutId(100), .. })),
            "{:?}",
            names(&sent)
        );
    }

    #[test]
    fn choosing_a_pane_on_the_rail_leaves_the_feature_stage() {
        let mut s = session(without_focus());
        s.show_features(true);
        s.select(PaneId(2));
        assert!(!s.features.is_open());
        s.show_features(true);
        s.select(PaneId(99));
        assert!(s.features.is_open(), "a pane that is not there changes nothing");
    }

    #[test]
    fn going_to_a_features_pane_leaves_the_stage_for_it() {
        let mut s = session(without_focus());
        s.show_features(true);
        assert_eq!(names(&s.go_to_pane(PaneId(1))), ["subscribe 1"]);
        assert!(!s.features.is_open());
    }

    #[test]
    fn stopping_is_remembered_until_a_connection_succeeds() {
        let mut s = session(without_focus());
        s.stopped();
        assert!(s.is_stopped());
        s.connected(None, None, Vec::new());
        assert!(!s.is_stopped());
    }
}
