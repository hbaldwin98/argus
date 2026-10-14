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

use argus_protocol::keys::{Key, KeyCode, Mods};
use argus_protocol::mouse::{self, MouseButton, MouseEvent, MouseKind};
use argus_protocol::paste;
use argus_protocol::{
    Cell, CheckoutId, ClientMsg, Created, Hello, HeldTree, PaneId, ProjectId, ProjectInfo,
    RepositoryId, ServerMsg, View, FOCUS, PASTE_FILE,
};

use crate::feature_stage::{FeatureStage, Scope};
use crate::features::Layout;
use crate::keymap::{self, Action};
use crate::keys::{self, PageKey};
use crate::page::{Connection, Event, PageMouse, Status};
use crate::settings::Settings;
use crate::rail::{self, Choices};
use crate::checkouts_stage::{CheckoutsStage, Deleting, Opened};
use crate::history::{HistoryDialog, Then};
use crate::review::ReviewDialog;
use crate::screen::Screen;

/// Lines one wheel notch moves a parked view, as terminals scroll for one
/// detent of a physical wheel.
const WHEEL_LINES: i32 = 3;

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
    /// The agent templates the daemon offers, the first its default.
    templates: Vec<String>,
    checkouts: CheckoutsStage,
    review: Option<ReviewDialog>,
    /// What the page was last sent of the review, so an unchanged one is
    /// not sent again.
    review_sent: Option<crate::page::ReviewPage>,
    history: Option<HistoryDialog>,
    history_sent: Option<crate::page::HistoryPage>,
    /// A pane the daemon made for us that no tree has carried yet.
    made: Option<PaneId>,
    focused: bool,
    /// The leader was pressed in the pane: the next key is the window's.
    leader: bool,
    /// The person asked for the pane to fill the window.
    filled: bool,
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
            templates: Vec::new(),
            checkouts: CheckoutsStage::default(),
            review: None,
            review_sent: None,
            history: None,
            history_sent: None,
            made: None,
            focused: true,
            leader: false,
            filled: false,
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
                match created {
                    Some(Created::Pane(pane)) => {
                        self.made = Some(pane);
                        self.show_made();
                    }
                    // A worktree made from the Checkouts stage is the row it
                    // lands on.
                    Some(Created::Checkout(checkout)) => self.checkouts.select(format!("c{}", checkout.0)),
                    _ => {}
                }
            }
            ServerMsg::Review(review) => {
                let taken = self.history.as_mut().is_some_and(|h| h.receive_diff(review.clone()));
                if let Some(dialog) = self.review.as_mut().filter(|_| !taken) {
                    dialog.receive(review);
                }
            }
            ServerMsg::Commits { request_id, commits, .. } => {
                if let Some(dialog) = &mut self.history {
                    dialog.receive_commits(request_id, commits);
                }
            }
            ServerMsg::CommitsFailed { request_id, message, .. } => {
                if let Some(dialog) = &mut self.history {
                    dialog.commits_failed(request_id, message);
                }
            }
            ServerMsg::CommitFiles { checkout, commit, files } => {
                if let Some(dialog) = &mut self.history {
                    dialog.receive_files(checkout, &commit, files);
                }
            }
            ServerMsg::CommitFilesFailed { checkout, commit, message } => {
                if let Some(dialog) = &mut self.history {
                    dialog.files_failed(checkout, &commit);
                }
                self.passing.push(Event::Notice { text: message });
            }
            ServerMsg::ReviewFailed { request_id, message, .. } => {
                if let Some(dialog) = &mut self.review {
                    dialog.failed(request_id, message);
                }
            }
            ServerMsg::ReviewCommentSaved { id, delivered } => {
                if let Some(dialog) = &mut self.review {
                    dialog.commented();
                }
                let text = if delivered { format!("comment #{id} sent") } else { format!("comment #{id} saved; the agent was not there to take it") };
                self.passing.push(Event::Notice { text });
            }
            ServerMsg::BranchNotMerged { checkout, branch, commits } => {
                self.checkouts.refused(checkout, branch, &commits, argus_protocol::ago::now());
            }
            ServerMsg::Clipboard { pane, text } if Some(pane) == self.shown => {
                self.passing.push(Event::Clipboard { text });
            }
            ServerMsg::ScrollbackRows { pane, offset, depth, cells } => {
                self.scrolled(pane, offset, depth, None, cells);
            }
            ServerMsg::ScrollbackRuns { pane, offset, depth, rows, cols, runs, top } => {
                self.scrolled(pane, offset, depth, top, argus_protocol::grid_from_runs(rows, cols, &runs));
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
            ServerMsg::Templates(names) => {
                self.templates = names;
                self.rail_changed = true;
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
        if self.project_of(pane).is_some() {
            self.leave_stages();
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
        // As in the terminal client, filling is for the pane it was asked
        // on: another pane comes back with the rail beside it.
        self.filled = false;
        self.leader = false;
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

    /// A key pressed in the pane, dispatched from the keymap: the leader
    /// waits for the next key, a binding runs, and anything else is the
    /// program's. A key after the leader that nothing binds is dropped, as
    /// the terminal client drops it, rather than typed by surprise.
    pub fn key(&mut self, key: &PageKey) -> Vec<ClientMsg> {
        let after_leader = std::mem::take(&mut self.leader);
        match keymap::in_pane(key, after_leader) {
            Some(action) => self.run(action),
            None if after_leader => {}
            None if keymap::is_leader(key) && self.shown.is_some() => self.leader = true,
            None => {
                let bytes = keys::to_key(key).map(|k| argus_protocol::keys::encode(&k)).unwrap_or_default();
                self.input(bytes);
            }
        }
        self.finish()
    }

    /// Whether `key` is the paste chord, which `commands` answers by
    /// reading the clipboard rather than typing it.
    pub fn pastes(&self, key: &PageKey) -> bool {
        !self.leader && keymap::in_pane(key, false) == Some(Action::Paste)
    }

    fn run(&mut self, action: Action) {
        let page = |s: &Session| s.screen.as_ref().map_or(2, |screen| screen.rows()).max(2) as i32 - 1;
        match action {
            Action::Leader => self.input(argus_protocol::keys::encode(&Key::new(KeyCode::Char(' '), Mods::CTRL))),
            Action::Fill => {
                self.filled = !self.filled;
                self.rail_changed = true;
            }
            Action::Close => {
                if let Some(pane) = self.shown {
                    self.kill(pane);
                }
            }
            Action::Workspace => self.leave_stages(),
            Action::Feature => self.open_features(true),
            Action::Checkouts => self.open_checkouts(true),
            // A screen less a line, so the line at the edge stays in view.
            Action::PageUp => self.scroll_by(-page(self)),
            Action::PageDown => self.scroll_by(page(self)),
            // `commands` reads the clipboard before the key gets here; the
            // rest are the page's, for keys a pane never hears.
            _ => {}
        }
    }

    /// The wheel over the pane, `notches` up when negative, at the cell
    /// `at` under the pointer, decided as the terminal client decides it: a
    /// child that asked for the mouse gets its wheel reports; otherwise the
    /// normal screen, which has history behind it, moves the view; and a
    /// full-screen child that asked for nothing gets a cursor key a notch,
    /// xterm's alternate scroll, which is what Claude Code, Codex and
    /// Cursor Agent listen for.
    pub fn wheel(&mut self, notches: i32, at: (u16, u16)) -> Vec<ClientMsg> {
        let Some(screen) = &self.screen else {
            return self.finish();
        };
        let (mouse, alternate, parked) = (screen.mouse(), screen.alternate_screen(), screen.parked().is_some());
        let count = notches.unsigned_abs() as usize;
        let up = notches < 0;
        let event = MouseEvent {
            kind: if up { MouseKind::ScrollUp } else { MouseKind::ScrollDown },
            row: at.0,
            col: at.1,
        };
        match mouse::encode(&event, mouse) {
            Some(report) if !parked => self.input(report.repeat(count)),
            _ if alternate && !parked => {
                let code = if up { KeyCode::Up } else { KeyCode::Down };
                self.input(argus_protocol::keys::encode(&Key::new(code, Mods::NONE)).repeat(count));
            }
            _ => self.scroll_by(notches.saturating_mul(WHEEL_LINES)),
        }
        self.finish()
    }

    /// A press, release or motion over the pane, at `[row, col]`, with
    /// `button` as `MouseEvent.button` counts (0 left, 1 middle, 2 right).
    /// The child gets it only as its mode reports it; a press, like typing,
    /// returns a parked view to live.
    pub fn mouse(&mut self, gesture: PageMouse, button: u8, at: (u16, u16)) -> Vec<ClientMsg> {
        let button = match button {
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            _ => MouseButton::Left,
        };
        let kind = match gesture {
            PageMouse::Down => MouseKind::Down(button),
            PageMouse::Up => MouseKind::Up(button),
            PageMouse::Drag => MouseKind::Drag(button),
            PageMouse::Move => MouseKind::Moved,
        };
        let tracking = self.screen.as_ref().map(|screen| screen.mouse()).unwrap_or_default();
        if let Some(bytes) = mouse::encode(&MouseEvent { kind, row: at.0, col: at.1 }, tracking) {
            self.input(bytes);
        }
        self.finish()
    }

    fn scroll_by(&mut self, lines: i32) {
        let (Some(pane), Some(screen)) = (self.shown, &mut self.screen) else {
            return;
        };
        let (offset, depth, top) = screen.parked().unwrap_or((0, 0, None));
        // A view never parked has not heard how deep the history goes, so
        // its first request up is unbounded and the daemon's clamp decides.
        let ceiling = if screen.parked().is_some() { i64::from(depth) } else { i64::MAX };
        let want = (i64::from(offset) - i64::from(lines)).clamp(0, ceiling);
        // Clamped at either end, a run of notches asks for the same place
        // over and over; only a move is worth a round trip.
        if want == i64::from(offset) {
            return;
        }
        if want == 0 {
            screen.unpark();
            return;
        }
        // Asked by line, so the view moves from the text on screen however
        // far output has pushed it back since.
        let top = top.map(|top| top.saturating_add_signed(i64::from(offset) - want));
        screen.park(want as u32, top);
        self.out.push(ClientMsg::Scrollback { pane, offset: want as u32, top });
    }

    /// Text an input method composed, typed as it stands.
    pub fn text(&mut self, text: &str) -> Vec<ClientMsg> {
        self.input(text.as_bytes().to_vec());
        self.finish()
    }

    /// Typing returns the view to the live screen: the child's echo lands
    /// there, and a parked view is not where input can be seen.
    fn input(&mut self, bytes: Vec<u8>) {
        if let (Some(pane), false) = (self.shown, bytes.is_empty()) {
            self.live();
            self.out.push(ClientMsg::Input { pane, bytes });
        }
    }

    /// A paste, as one event: the daemon brackets it only if the child asked.
    pub fn paste(&mut self, text: String) -> Vec<ClientMsg> {
        if let (Some(pane), false) = (self.shown, text.is_empty()) {
            self.live();
            self.out.push(ClientMsg::Paste { pane, text: paste::normalize(&text) });
        }
        self.finish()
    }

    /// A file off the clipboard — a picture, or one a file manager copied —
    /// sent whole: the pane's process reads the daemon's disk, not this
    /// machine's. The daemon keeps it and pastes its path, the one form
    /// every harness takes an image in.
    pub fn paste_file(&mut self, name: String, bytes: Vec<u8>) -> Vec<ClientMsg> {
        let Some(pane) = self.shown else {
            return self.finish();
        };
        let note = if !self.daemon.as_ref().is_some_and(|d| d.can(PASTE_FILE)) {
            "argusd is too old to paste files: run `argus server restart`".to_string()
        } else if bytes.len() > paste::PASTE_FILE_MAX {
            format!("{name} is too large to paste ({} MB)", bytes.len() >> 20)
        } else {
            self.live();
            let said = format!("pasted {name}");
            self.out.push(ClientMsg::PasteFile { pane, name, bytes });
            said
        };
        self.note(note)
    }

    fn live(&mut self) {
        if let Some(screen) = &mut self.screen {
            screen.unpark();
        }
    }

    fn scrolled(&mut self, pane: PaneId, offset: u32, depth: u32, top: Option<u64>, grid: Vec<Vec<Cell>>) {
        let Some(screen) = self.screen.as_mut().filter(|_| Some(pane) == self.shown) else {
            return;
        };
        if !screen.scrolled(offset, depth, top, grid) {
            self.passing.push(Event::Notice { text: "nothing further back".into() });
        }
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
        self.spawn(checkout, |request_id| ClientMsg::SpawnShell { checkout, request_id })
    }

    /// Asks for an agent from one of the daemon's templates on a checkout,
    /// shown once it exists as a shell is.
    pub fn spawn_agent(&mut self, checkout: CheckoutId, template: String) -> Vec<ClientMsg> {
        if !self.templates.contains(&template) {
            return self.note(format!("argusd has no agent template called {template}"));
        }
        self.spawn(checkout, |request_id| ClientMsg::SpawnAgent { checkout, template, request_id })
    }

    fn spawn(&mut self, checkout: CheckoutId, ask: impl FnOnce(u64) -> ClientMsg) -> Vec<ClientMsg> {
        let exists = self
            .tree
            .iter()
            .flat_map(|p| &p.repositories)
            .flat_map(|r| &r.checkouts)
            .any(|c| c.id == checkout);
        if exists {
            let request_id = self.next_request();
            self.out.push(ask(request_id));
        }
        self.finish()
    }

    /// Names a request answered by its own message rather than `Created`.
    fn fresh_id(&mut self) -> u64 {
        self.last_request = self.last_request.wrapping_add(1).max(1);
        self.last_request
    }

    /// Names a request whose answer this window waits for.
    fn next_request(&mut self) -> u64 {
        self.last_request = self.last_request.wrapping_add(1).max(1);
        if self.asked.len() == MAX_ASKED {
            self.asked.remove(0);
        }
        self.asked.push(self.last_request);
        self.last_request
    }

    /// Ends a pane's program and removes it. The pane stays on screen until
    /// a tree without it arrives, so a refused close leaves it showing.
    pub fn close(&mut self, pane: PaneId) -> Vec<ClientMsg> {
        self.kill(pane);
        self.finish()
    }

    fn kill(&mut self, pane: PaneId) {
        if self.project_of(pane).is_some() {
            self.out.push(ClientMsg::Kill { pane });
        }
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
        if open {
            self.open_features(true);
        } else {
            self.leave_stages();
        }
        self.finish()
    }

    fn open_features(&mut self, open: bool) {
        if open && self.checkouts.is_open() {
            self.checkouts.set_open(false);
            self.rail_changed = true;
        }
        if self.features.is_open() != open {
            self.features.set_open(open);
            self.rail_changed = true;
        }
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
            self.leave_stages();
            self.show(pane);
        }
        self.finish()
    }

    /// Leaves whichever stage is open for the Workspace.
    fn leave_stages(&mut self) {
        if self.features.is_open() || self.checkouts.is_open() {
            self.features.set_open(false);
            self.checkouts.set_open(false);
            self.rail_changed = true;
        }
    }

    /// Shows the Checkouts stage, or goes back to the Workspace.
    pub fn show_checkouts(&mut self, open: bool) -> Vec<ClientMsg> {
        self.open_checkouts(open);
        self.finish()
    }

    fn open_checkouts(&mut self, open: bool) {
        if open {
            self.open_features(false);
        }
        if self.checkouts.is_open() != open {
            self.checkouts.set_open(open);
            self.rail_changed = true;
        }
    }

    fn checkout_info(&self, checkout: CheckoutId) -> Option<&argus_protocol::CheckoutInfo> {
        self.tree.iter().flat_map(|p| &p.repositories).flat_map(|r| &r.checkouts).find(|c| c.id == checkout)
    }

    /// Opens the review of a checkout's working changes.
    pub fn open_review(&mut self, checkout: CheckoutId) -> Vec<ClientMsg> {
        let Some(info) = self.checkout_info(checkout).cloned() else {
            return self.finish();
        };
        let mut ids = Vec::new();
        let (dialog, msgs) = ReviewDialog::working(&info, || {
            let id = self.fresh_id();
            ids.push(id);
            id
        });
        self.review = Some(dialog);
        self.out.extend(msgs);
        self.finish()
    }

    pub fn open_history(&mut self, checkout: CheckoutId) -> Vec<ClientMsg> {
        let Some(info) = self.checkout_info(checkout).cloned() else {
            return self.finish();
        };
        let (dialog, msg) = HistoryDialog::open(&info, self.fresh_id());
        self.history = Some(dialog);
        self.out.push(msg);
        self.finish()
    }

    /// A key or click of the history dialog; `row` is a clicked row.
    pub fn history_action(&mut self, action: Action, row: Option<usize>) -> Vec<ClientMsg> {
        let Some(mut dialog) = self.history.take() else {
            return self.finish();
        };
        let then = match (action, row) {
            (_, Some(row)) => dialog.select_row(row, || self.fresh_id()),
            (Action::Next, _) => dialog.move_by(1, || self.fresh_id()),
            (Action::Previous, _) => dialog.move_by(-1, || self.fresh_id()),
            (Action::First, _) => dialog.ends(false, || self.fresh_id()),
            (Action::Last, _) => dialog.ends(true, || self.fresh_id()),
            (Action::Unfold, _) => dialog.unfold(),
            (Action::Fold, _) => {
                dialog.fold();
                Then::Nothing
            }
            (Action::Choose, _) => dialog.choose(),
            (Action::AsReview, _) => dialog.as_review(),
            (Action::CloseLayer, _) => return self.finish(),
            _ => Then::Nothing,
        };
        match then {
            Then::Nothing => self.history = Some(dialog),
            Then::Ask(msg) => {
                self.history = Some(dialog);
                self.out.push(*msg);
            }
            // The history gives way to the review of the commit.
            Then::Review(commit) => {
                if let Some(info) = self.checkout_info(dialog.checkout()).cloned() {
                    let (review, msg) = ReviewDialog::of_commit(&info, commit, self.fresh_id());
                    self.review = Some(review);
                    self.out.push(msg);
                }
            }
        }
        self.finish()
    }

    /// A key or button of the review dialog.
    pub fn review_action(&mut self, action: Action) -> Vec<ClientMsg> {
        let Some(dialog) = &mut self.review else {
            return self.finish();
        };
        match action {
            Action::Next => dialog.move_by(1),
            Action::Previous => dialog.move_by(-1),
            Action::First => dialog.ends(false),
            Action::Last => dialog.ends(true),
            Action::NextFile => dialog.jump_file(true),
            Action::PreviousFile => dialog.jump_file(false),
            Action::Pick => dialog.toggle_pick(),
            Action::Side => dialog.switch_side(),
            Action::Split => dialog.toggle_split(),
            Action::Comment => {
                if let Err(text) = dialog.compose() {
                    self.passing.push(Event::Notice { text: text.into() });
                }
            }
            Action::CloseLayer => self.review = None,
            _ => {}
        }
        self.finish()
    }

    /// A click in the review: a file in the list, a line in the diff (with
    /// shift, extending the range), a side, or a recipient.
    pub fn review_click(&mut self, target: ReviewClick) -> Vec<ClientMsg> {
        if let Some(dialog) = &mut self.review {
            match target {
                ReviewClick::File { file } => dialog.open_file(file),
                ReviewClick::Row { row, extend } => dialog.select_row(row, extend),
                ReviewClick::Side { staged } => dialog.show_side(staged),
                ReviewClick::Recipient { pane } => dialog.choose_recipient(PaneId(pane)),
            }
        }
        self.finish()
    }

    pub fn review_refresh(&mut self) -> Vec<ClientMsg> {
        let Some(mut dialog) = self.review.take() else {
            return self.finish();
        };
        let msgs = dialog.refresh(|| self.fresh_id());
        self.review = Some(dialog);
        self.out.extend(msgs);
        self.finish()
    }

    pub fn review_comment(&mut self, body: Option<String>) -> Vec<ClientMsg> {
        let Some(dialog) = &mut self.review else {
            return self.finish();
        };
        match body {
            None => dialog.cancel_comment(),
            Some(body) => match dialog.comment(body) {
                Ok(msg) => self.out.push(msg),
                Err(text) => self.passing.push(Event::Notice { text: text.into() }),
            },
        }
        self.finish()
    }

    /// `x` on a checkout in the rail: the Checkouts stage opens on its row
    /// and asks, since its question is the stage's and is drawn there.
    pub fn remove_checkout(&mut self, checkout: CheckoutId) -> Vec<ClientMsg> {
        let Some(repo) = self
            .current_project()
            .and_then(|p| p.repositories.iter().find(|r| r.checkouts.iter().any(|c| c.id == checkout)))
            .cloned()
        else {
            return self.finish();
        };
        let Some(info) = repo.checkouts.iter().find(|c| c.id == checkout) else {
            return self.finish();
        };
        if let Err(text) = self.checkouts.ask_remove(info) {
            return self.note(text);
        }
        if self.repository != Some(repo.id) {
            self.repository = Some(repo.id);
            self.rail_changed = true;
        }
        self.open_checkouts(true);
        self.checkouts.select(format!("c{}", checkout.0));
        self.finish()
    }

    /// The answer to the stage's question: a yes does what it asked about.
    pub fn answer_checkouts(&mut self, yes: bool) -> Vec<ClientMsg> {
        if let Some(deleting) = self.checkouts.answer(yes) {
            self.out.push(deletion(deleting));
        }
        self.finish()
    }

    /// A worktree for the selected row: `name` is the new branch for a
    /// checkout row, and unused for a branch row, which brings its own.
    pub fn new_worktree(&mut self, name: Option<String>) -> Vec<ClientMsg> {
        let Some(repo) = self.chosen_repository().cloned() else {
            return self.finish();
        };
        match self.checkouts.new_worktree(&repo, name) {
            Ok((checkout, branch)) => {
                let request_id = self.next_request();
                self.out.push(ClientMsg::CreateWorktree { checkout, branch, request_id });
            }
            Err(text) => self.passing.push(Event::Notice { text: text.to_owned() }),
        }
        self.finish()
    }

    pub fn select_checkout_row(&mut self, key: String) -> Vec<ClientMsg> {
        self.checkouts.select(key);
        self.finish()
    }

    pub fn filter_checkouts(&mut self, filter: String) -> Vec<ClientMsg> {
        self.checkouts.set_filter(filter);
        self.finish()
    }

    /// A key or button of the Checkouts stage, acting on its selected row.
    pub fn checkouts_action(&mut self, action: Action) -> Vec<ClientMsg> {
        let Some(repo) = self.chosen_repository().cloned() else {
            return self.finish();
        };
        match action {
            Action::Next => self.checkouts.step(&repo, 1),
            Action::Previous => self.checkouts.step(&repo, -1),
            Action::First => self.checkouts.step(&repo, isize::MIN),
            Action::Last => self.checkouts.step(&repo, isize::MAX),
            Action::Branches => self.checkouts.toggle_branches(),
            Action::Choose => match self.checkouts.open_row(&repo) {
                Some(Opened::Pane(pane)) => {
                    self.leave_stages();
                    self.show(pane);
                }
                Some(Opened::Switch { checkout, branch }) => self.out.push(ClientMsg::SwitchBranch { checkout, branch }),
                Some(Opened::Say(text)) => self.passing.push(Event::Notice { text }),
                None => {}
            },
            Action::Fetch | Action::Pull => {
                if let Some(checkout) = self.checkouts.where_git_runs(&repo) {
                    self.out.push(if action == Action::Fetch {
                        ClientMsg::Fetch { checkout }
                    } else {
                        ClientMsg::Pull { checkout }
                    });
                }
            }
            Action::OpenReview => match self.checkouts.checkout(&repo) {
                Some(checkout) => return self.open_review(checkout),
                None => self.passing.push(Event::Notice { text: "a branch has no changes of its own to review".into() }),
            },
            Action::OpenHistory => match self.checkouts.checkout(&repo) {
                Some(checkout) => return self.open_history(checkout),
                None => self.passing.push(Event::Notice { text: "a branch's history opens from a checkout of it".into() }),
            },
            Action::Delete => match self.checkouts.delete(&repo) {
                Ok(Some(deleting)) => self.out.push(deletion(deleting)),
                Ok(None) => {}
                Err(text) => self.passing.push(Event::Notice { text }),
            },
            Action::StartShell | Action::StartAgent => {
                let Some(checkout) = self.checkouts.checkout(&repo) else {
                    return self.note("give the branch a worktree first (N)");
                };
                return match (action, self.templates.first().cloned()) {
                    (Action::StartShell, _) => self.spawn_shell(checkout),
                    (_, Some(agent)) => self.spawn_agent(checkout, agent),
                    (_, None) => self.note("argusd offers no agent to start"),
                };
            }
            _ => {}
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
        self.features.set_board_wanted(self.checkouts.is_open());
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
        if self.review.is_some() || self.review_sent.as_ref().is_some_and(|r| r.open) {
            let page = match &self.review {
                Some(dialog) => dialog.page(self.checkout_info(dialog.checkout())),
                None => crate::page::ReviewPage::default(),
            };
            if self.review_sent.as_ref() != Some(&page) {
                self.review_sent = Some(page.clone());
                events.push(Event::Review(Box::new(page)));
            }
        }
        if self.history.is_some() || self.history_sent.as_ref().is_some_and(|h| h.open) {
            let page = match &self.history {
                Some(dialog) => dialog.page(now),
                None => crate::page::HistoryPage::default(),
            };
            if self.history_sent.as_ref() != Some(&page) {
                self.history_sent = Some(page.clone());
                events.push(Event::History(Box::new(page)));
            }
        }
        if self.checkouts.is_open() {
            let repo = self.chosen_repository().cloned();
            if let Some(table) = self.checkouts.take(repo.as_ref(), self.features.board(), self.features.peeked(), now) {
                events.push(Event::Checkouts(Box::new(table)));
            }
        }
        events.append(&mut self.passing);
        events
    }

    /// Everything, for a page that has just loaded and holds nothing; the
    /// shown pane's whole screen follows with the next frame.
    pub fn everything(&mut self, now: u64) -> Vec<Event> {
        self.status_changed = true;
        self.review_sent = None;
        self.history_sent = None;
        self.rail_changed = true;
        if let Some(screen) = &mut self.screen {
            screen.refresh();
        }
        let mut events = vec![Event::Keymap { groups: keymap::groups() }];
        events.append(&mut self.events(now));
        if !events.iter().any(|e| matches!(e, Event::Features(_))) {
            let scope = self.scope();
            events.push(Event::Features(Box::new(self.features.everything(scope.as_ref(), &self.tree))));
        }
        if !events.iter().any(|e| matches!(e, Event::Checkouts(_))) {
            let repo = self.chosen_repository().cloned();
            let table = self.checkouts.everything(repo.as_ref(), self.features.board(), self.features.peeked(), now);
            events.push(Event::Checkouts(Box::new(table)));
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
        rail.stage = if self.features.is_open() {
            "feature"
        } else if self.checkouts.is_open() {
            "checkouts"
        } else {
            "workspace"
        };
        rail.filled = self.filled && !self.features.is_open() && self.shown.is_some();
        rail.templates = self.templates.clone();
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
/// What a click in the review dialog landed on.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum ReviewClick {
    File { file: usize },
    Row { row: usize, extend: bool },
    Side { staged: bool },
    Recipient { pane: u64 },
}

fn deletion(deleting: Deleting) -> ClientMsg {
    match deleting {
        Deleting::Branch { checkout, branch, force } => ClientMsg::DeleteBranch { checkout, branch, force },
        Deleting::Checkout(checkout) => ClientMsg::RemoveCheckout { checkout },
    }
}

fn another_build(daemon: &Hello) -> Option<String> {
    let ours = env!("CARGO_PKG_VERSION");
    (daemon.version != ours).then(|| {
        format!(
            "argusd {}, window {ours}: run `argus server restart`",
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
    fn closing_asks_the_daemon_and_the_pane_stays_until_the_tree_drops_it() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        assert_eq!(names(&s.close(PaneId(1))), ["Kill { pane: PaneId(1) }"]);
        assert_eq!(s.shown, Some(PaneId(1)), "nothing is predicted");
        let mut gone = tree();
        gone[0].repositories[0].checkouts[0].panes.remove(0);
        s.receive(with_tree(gone));
        assert_eq!(s.shown, None);
    }

    #[test]
    fn a_pane_not_in_the_tree_cannot_be_closed() {
        let mut s = session(without_focus());
        assert!(s.close(PaneId(99)).is_empty());
    }

    #[test]
    fn the_leader_and_x_close_the_shown_pane() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        let leader = PageKey { key: " ".into(), ctrl: true, ..PageKey::default() };
        s.key(&leader);
        let x = PageKey { key: "x".into(), ..PageKey::default() };
        assert_eq!(names(&s.key(&x)), ["Kill { pane: PaneId(1) }"]);
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
        assert_eq!(s.everything(0).len(), 5, "a fresh page hears it all, the keymap first");
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

    /// A session showing pane 2, whose screen of `rows` has arrived.
    fn showing(rows: u16, alternate_screen: bool) -> Session {
        let mut s = session(without_focus());
        s.select(PaneId(2));
        s.receive(ServerMsg::PaneRows {
            pane: PaneId(2),
            rows,
            cols: 10,
            runs: Vec::new(),
            cursor: argus_protocol::Cursor::default(),
            mouse: argus_protocol::MouseTracking::default(),
            alternate_screen,
        });
        s
    }

    fn history(offset: u32, depth: u32, top: Option<u64>) -> ServerMsg {
        ServerMsg::ScrollbackRuns { pane: PaneId(2), offset, depth, rows: 5, cols: 10, runs: Vec::new(), top }
    }

    fn scrollback(msgs: &[ClientMsg]) -> Vec<(u32, Option<u64>)> {
        msgs.iter()
            .filter_map(|m| match m {
                ClientMsg::Scrollback { offset, top, .. } => Some((*offset, *top)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_wheel_parks_the_view_and_asks_further_up_by_line() {
        let mut s = showing(5, false);
        assert_eq!(scrollback(&s.wheel(-1, (0, 0))), [(3, None)]);
        s.receive(history(3, 50, Some(100)));
        assert_eq!(scrollback(&s.wheel(-1, (0, 0))), [(6, Some(97))]);
        assert_eq!(scrollback(&s.wheel(1, (0, 0))), [(3, Some(100))]);
    }

    #[test]
    fn scrolling_stops_at_the_depth_the_daemon_gave() {
        let mut s = showing(5, false);
        s.wheel(-1, (0, 0));
        s.receive(history(3, 4, Some(10)));
        assert_eq!(scrollback(&s.wheel(-1, (0, 0))), [(4, Some(9))]);
        s.receive(history(4, 4, Some(9)));
        assert!(scrollback(&s.wheel(-1, (0, 0))).is_empty(), "already at the top");
    }

    #[test]
    fn scrolling_down_to_the_bottom_is_live_without_asking() {
        let mut s = showing(5, false);
        s.wheel(-1, (0, 0));
        s.receive(history(3, 50, Some(100)));
        assert!(scrollback(&s.wheel(4, (0, 0))).is_empty());
        assert!(s.screen.as_ref().unwrap().parked().is_none());
    }

    #[test]
    fn typing_or_pasting_returns_the_view_to_the_live_screen() {
        let mut s = showing(5, false);
        s.wheel(-1, (0, 0));
        s.key(&PageKey { key: "a".into(), ..PageKey::default() });
        assert!(s.screen.as_ref().unwrap().parked().is_none());

        s.wheel(-1, (0, 0));
        s.paste("x".into());
        assert!(s.screen.as_ref().unwrap().parked().is_none());
    }

    #[test]
    fn shift_page_up_pages_by_a_screen_less_a_line() {
        let mut s = showing(5, false);
        let page_up = PageKey { key: "PageUp".into(), shift: true, ..PageKey::default() };
        assert_eq!(scrollback(&s.key(&page_up)), [(4, None)]);
        let plain = PageKey { key: "PageUp".into(), ..PageKey::default() };
        assert_eq!(names(&s.key(&plain)), ["input 2 [27, 91, 53, 126]"], "the child's own key");
    }

    fn ctrl_space() -> PageKey {
        PageKey { key: " ".into(), code: "Space".into(), ctrl: true, ..PageKey::default() }
    }

    fn press(key: &str) -> PageKey {
        PageKey { key: key.into(), ..PageKey::default() }
    }

    #[test]
    fn leader_twice_sends_leader_to_pane() {
        let mut s = showing(5, false);
        assert!(s.key(&ctrl_space()).is_empty(), "the leader waits for the next key");
        assert_eq!(names(&s.key(&ctrl_space())), ["input 2 [0]"]);
        assert_eq!(names(&s.key(&press("a"))), ["input 2 [97]"], "and the pane has its keys again");
    }

    #[test]
    fn a_key_after_the_leader_is_the_windows_not_the_programs() {
        let mut s = showing(5, false);
        s.events(0);
        s.key(&ctrl_space());
        let typed = |msgs: Vec<ClientMsg>| msgs.iter().any(|m| matches!(m, ClientMsg::Input { .. }));
        assert!(!typed(s.key(&press("2"))), "nothing typed");
        assert!(s.features.is_open());
        s.key(&ctrl_space());
        s.key(&press("1"));
        assert!(!s.features.is_open());
        s.key(&ctrl_space());
        assert!(s.key(&press("q")).is_empty(), "an unbound key after the leader is dropped");
        assert_eq!(names(&s.key(&press("2"))), ["input 2 [50]"], "a digit alone is typed");
    }

    #[test]
    fn leader_f_fills_the_window_with_the_pane_and_again_gives_it_back() {
        let mut s = showing(5, false);
        let filled = |s: &mut Session| match s.events(0).as_slice() {
            [.., Event::Rail(rail)] => rail.filled,
            other => panic!("no rail in {other:?}"),
        };
        s.events(0);
        s.key(&ctrl_space());
        assert!(s.key(&press("f")).is_empty());
        assert!(filled(&mut s));
        s.key(&ctrl_space());
        s.key(&press("f"));
        assert!(!filled(&mut s));
    }

    #[test]
    fn a_filled_window_gives_the_rail_back_for_another_pane_or_stage() {
        let mut s = showing(5, false);
        s.key(&ctrl_space());
        s.key(&press("f"));
        s.show_features(true);
        assert!(!s.rail(0).filled, "the Feature stage has no pane to fill");
        s.show_features(false);
        assert!(s.rail(0).filled, "and the pane fills again on the way back");
        s.select(PaneId(1));
        assert!(!s.rail(0).filled);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_paste_chord_reads_the_clipboard_unless_the_leader_is_waiting() {
        let mut s = showing(5, false);
        let ctrl_v = PageKey { ctrl: true, ..press("v") };
        assert!(s.pastes(&ctrl_v));
        assert!(!s.pastes(&press("v")));
        s.key(&ctrl_space());
        assert!(!s.pastes(&ctrl_v));
    }

    #[test]
    fn the_wheel_over_a_full_screen_child_is_cursor_keys() {
        let mut s = showing(5, true);
        assert_eq!(names(&s.wheel(-2, (0, 0))), ["input 2 [27, 91, 65, 27, 91, 65]"]);
        assert_eq!(names(&s.wheel(1, (0, 0))), ["input 2 [27, 91, 66]"]);
    }

    #[test]
    fn the_wheel_over_a_child_that_asked_for_the_mouse_is_its_wheel_report() {
        // Claude Code asks for the mouse; a cursor key there walks its
        // prompt history instead of scrolling.
        let mut s = showing(5, true);
        s.receive(ServerMsg::RowDamage {
            pane: PaneId(2),
            scroll: None,
            runs: Vec::new(),
            cursor: argus_protocol::Cursor::default(),
            mouse: argus_protocol::MouseTracking {
                mode: argus_protocol::MouseMode::PressRelease,
                encoding: argus_protocol::MouseEncoding::Sgr,
            },
            alternate_screen: true,
        });
        let up = "\x1b[<64;4;2M".repeat(2).into_bytes();
        assert_eq!(names(&s.wheel(-2, (1, 3))), [format!("input 2 {up:?}")]);
    }

    #[test]
    fn a_history_with_nothing_behind_the_screen_leaves_it_live_and_says_so() {
        let mut s = showing(5, false);
        s.events(0);
        s.wheel(-1, (0, 0));
        s.receive(history(0, 0, None));
        assert!(s.screen.as_ref().unwrap().parked().is_none());
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text == "nothing further back")));
    }

    #[test]
    fn a_late_answer_does_not_pull_a_live_view_up() {
        let mut s = showing(5, false);
        s.wheel(-1, (0, 0));
        s.key(&PageKey { key: "a".into(), ..PageKey::default() });
        s.receive(history(3, 50, Some(100)));
        assert!(s.screen.as_ref().unwrap().parked().is_none());
    }

    #[test]
    fn a_pasted_file_crosses_whole_to_a_daemon_that_takes_one() {
        let mut s = session(Hello::this_build());
        s.select(PaneId(2));
        let sent = s.paste_file("clipboard.png".into(), vec![1, 2, 3]);
        assert!(sent.iter().any(|m| matches!(m,
            ClientMsg::PasteFile { pane: PaneId(2), name, bytes } if name == "clipboard.png" && bytes == &[1, 2, 3]
        )));

        let mut old = session(without_focus());
        old.select(PaneId(2));
        old.events(0);
        assert!(old.paste_file("shot.png".into(), vec![1]).is_empty());
        assert!(old.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text.contains("too old"))));
    }

    #[test]
    fn a_pasted_windows_line_ending_is_not_enter() {
        let mut s = session(without_focus());
        s.select(PaneId(2));
        assert_eq!(names(&s.paste("one\r\ntwo".into())), ["paste 2 one\ntwo"]);
    }

    #[test]
    fn a_press_reaches_a_child_only_in_the_mode_it_asked_for() {
        let mut s = showing(5, false);
        assert!(s.mouse(PageMouse::Down, 0, (1, 3)).is_empty(), "it asked for nothing");

        s.receive(ServerMsg::RowDamage {
            pane: PaneId(2),
            scroll: None,
            runs: Vec::new(),
            cursor: argus_protocol::Cursor::default(),
            mouse: argus_protocol::MouseTracking {
                mode: argus_protocol::MouseMode::PressRelease,
                encoding: argus_protocol::MouseEncoding::Sgr,
            },
            alternate_screen: false,
        });
        let down = b"\x1b[<2;4;2M".to_vec();
        assert_eq!(names(&s.mouse(PageMouse::Down, 2, (1, 3))), [format!("input 2 {down:?}")]);
        assert!(s.mouse(PageMouse::Drag, 0, (1, 4)).is_empty(), "no motion asked for");
    }

    #[test]
    fn the_rail_offers_the_daemons_templates_as_agents() {
        let mut s = session(without_focus());
        s.receive(ServerMsg::Templates(vec!["claude".into(), "codex".into()]));
        let Some(Event::Rail(rail)) = s.events(0).into_iter().find(|e| matches!(e, Event::Rail(_))) else {
            panic!("the rail is drawn again");
        };
        assert_eq!(rail.templates, ["claude", "codex"]);
    }

    #[test]
    fn an_agent_is_asked_of_a_template_the_daemon_has_and_shown_once_made() {
        let mut s = session(without_focus());
        s.receive(ServerMsg::Templates(vec!["claude".into()]));
        let sent = s.spawn_agent(CheckoutId(100), "claude".into());
        let Some(ClientMsg::SpawnAgent { checkout, template, request_id }) = sent.first() else {
            panic!("{sent:?}");
        };
        assert_eq!((*checkout, template.as_str()), (CheckoutId(100), "claude"));
        s.receive(ServerMsg::Created { request_id: *request_id, created: Some(Created::Pane(PaneId(2))) });
        assert_eq!(s.shown, Some(PaneId(2)));

        s.events(0);
        assert!(s.spawn_agent(CheckoutId(100), "nobody".into()).is_empty());
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text.contains("nobody"))));
    }

    #[test]
    fn the_checkouts_stage_opens_alone_and_asks_for_the_board_it_names_features_from() {
        let mut s = session(without_focus());
        s.open_features(true);
        let sent = s.show_checkouts(true);
        assert!(!s.features.is_open() && s.checkouts.is_open(), "one stage at a time");
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::GetDecisions { .. })), "{sent:?}");
        let events = s.events(0);
        assert!(events.iter().any(|e| matches!(e, Event::Checkouts(table) if table.repository.as_deref() == Some("argus"))));
        let Some(Event::Rail(rail)) = events.iter().find(|e| matches!(e, Event::Rail(_))) else { panic!() };
        assert_eq!(rail.stage, "checkouts");
    }

    #[test]
    fn enter_on_a_checkout_row_shows_its_pane_in_the_workspace() {
        let mut s = session(without_focus());
        s.show_checkouts(true);
        s.select_checkout_row("c100".into());
        s.checkouts_action(Action::Choose);
        assert_eq!(s.shown, Some(PaneId(1)));
        assert!(!s.checkouts.is_open());
    }

    #[test]
    fn fetch_and_pull_run_in_the_selected_checkout() {
        let mut s = session(without_focus());
        s.show_checkouts(true);
        s.select_checkout_row("c100".into());
        assert!(matches!(s.checkouts_action(Action::Pull).as_slice(), [ClientMsg::Pull { checkout: CheckoutId(100) }]));
        assert!(matches!(s.checkouts_action(Action::Fetch).as_slice(), [ClientMsg::Fetch { checkout: CheckoutId(100) }]));
    }

    #[test]
    fn a_refused_branch_delete_is_asked_about_and_a_yes_forces_it() {
        let mut s = session(without_focus());
        s.show_checkouts(true);
        s.events(0);
        s.receive(ServerMsg::BranchNotMerged { checkout: CheckoutId(100), branch: "spike".into(), commits: Vec::new() });
        let asked = s.events(0).into_iter().find_map(|e| match e {
            Event::Checkouts(table) => table.confirm,
            _ => None,
        });
        assert_eq!(asked, Some(crate::page::Confirm::Force { branch: "spike".into(), commits: Vec::new() }));
        let sent = s.answer_checkouts(true);
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::DeleteBranch { checkout: CheckoutId(100), branch, force: true }] if branch == "spike"
        ));
    }

    #[test]
    fn x_on_a_rail_worktree_opens_its_row_and_asks_and_the_primary_stays() {
        let mut s = session(without_focus());
        let mut more = tree();
        more[0].repositories[0].checkouts.push(checkout(101, "spike", false, Vec::new()));
        s.receive(with_tree(more));
        s.events(0);
        let sent = s.remove_checkout(CheckoutId(101));
        assert!(!sent.iter().any(|m| matches!(m, ClientMsg::RemoveCheckout { .. })), "nothing is removed before the answer");
        assert!(s.checkouts.is_open());
        assert_eq!(s.checkouts.selected_key(), Some("c101"));
        let asked = s.events(0).into_iter().find_map(|e| match e {
            Event::Checkouts(table) => table.confirm,
            _ => None,
        });
        assert!(matches!(asked, Some(crate::page::Confirm::Remove { name, .. }) if name == "spike"));
        assert!(matches!(s.answer_checkouts(true).as_slice(), [ClientMsg::RemoveCheckout { checkout: CheckoutId(101) }]));

        s.remove_checkout(CheckoutId(100));
        let said = s.events(0).into_iter().find_map(|e| match e {
            Event::Notice { text } => Some(text),
            _ => None,
        });
        assert_eq!(said.as_deref(), Some("main is the primary checkout and stays"));
        assert!(s.answer_checkouts(true).is_empty(), "nothing was asked");
    }

    #[test]
    fn a_new_worktree_is_the_row_selected_once_made() {
        let mut s = session(without_focus());
        s.show_checkouts(true);
        s.select_checkout_row("c100".into());
        let sent = s.new_worktree(Some("spike".into()));
        let Some(ClientMsg::CreateWorktree { checkout, branch, request_id }) = sent.first() else {
            panic!("{sent:?}");
        };
        assert_eq!((*checkout, branch.as_str()), (CheckoutId(100), "spike"));
        s.receive(ServerMsg::Created { request_id: *request_id, created: Some(Created::Checkout(CheckoutId(200))) });
        assert_eq!(s.checkouts.selected_key(), Some("c200"));
    }
}
