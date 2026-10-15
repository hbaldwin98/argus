//! What this window holds of the daemon and routes between its parts: the
//! tree, which project and repository the rail has chosen, which stage is
//! up, and what each daemon message and page call changes.
//!
//! Plain Rust with no Tauri in it: each daemon message and each call the
//! page makes goes in, and out come the messages to send the daemon. The
//! page's events are taken afterwards (`events`, `take_frame`), so however
//! many things changed in between, the page hears each once.
//!
//! Each subject keeps its own state and what the page last heard of it —
//! the status bar (`status`), the pane on screen (`shown`), the agents
//! offered (`agents`), the review and history (`dialogs`), the two stages
//! (`feature_stage`, `checkouts_stage`), a repository's past agent
//! sessions (`sessions`) and the dialog listing them (`sessions_dialog`),
//! and what the daemon is making
//! (`argus_protocol::awaited`). The session says which of them a message
//! or gesture is for, and what one of them changing means for the rest.
//!
//! The client never predicts: the rail changes when the daemon's tree does,
//! and a shell asked for is shown only once the daemon has said it made it
//! and a tree holds it.

use std::collections::HashSet;

use argus_protocol::awaited::Awaited;
use argus_protocol::column_verbs::Verb;
use argus_protocol::keys::{Key, KeyCode, Mods};
use argus_protocol::mouse::{self, MouseButton, MouseEvent, MouseKind};
use argus_protocol::pane_view::{Effect, PaneView};
use argus_protocol::paste;
use argus_protocol::{
    transitions, CheckoutId, ClientMsg, Created, Hello, HeldTree, PaneId, ProjectId, ProjectInfo,
    RepositoryId, ServerMsg, View, FOCUS, PASTE_FILE,
};

use crate::agents::Agents;
use crate::checkouts_stage::{CheckoutsStage, Opened};
use crate::dialogs::Dialogs;
use crate::feature_stage::{FeatureStage, Scope};
use crate::features::Layout;
use crate::keymap::{self, Action};
use crate::keys::{self, PageKey};
use crate::page::{self, Call, Event, PageMouse, ReviewClick, ReviewJumpGesture, Stage};
use crate::palette;
use crate::rail::{self, Choices};
use crate::sent::Sent;
use crate::sessions::Sessions;
use crate::sessions_dialog::{SessionsDialog, Then as SessionsThen};
use crate::settings::Settings;
use crate::shown::ShownPane;
use crate::status::StatusBar;

/// What to do with a thing the daemon made for this window, once the tree
/// holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Made {
    /// Show the new pane.
    Show,
    /// Show the new pane, leaving whichever stage is up: the editor E asked
    /// for from the review, or the composer's start, which stays up until
    /// the pane is made so a refused one keeps what was typed.
    ShowOverStages,
    /// Select the new worktree's row in the Checkouts stage.
    SelectRow,
    /// Show the pane a session was resumed in, putting the sessions
    /// dialog away with whichever stage is up.
    Resumed,
}

pub struct Session {
    tree: Vec<ProjectInfo>,
    /// Whether any daemon has sent a tree yet.
    loaded: bool,
    status: StatusBar,
    project: Option<ProjectId>,
    /// The repository the rail last had chosen, whose features the Feature
    /// stage reads.
    repository: Option<RepositoryId>,
    toggled: HashSet<RepositoryId>,
    stage: Stage,
    features: FeatureStage,
    checkouts: CheckoutsStage,
    shown: ShownPane,
    awaited: Awaited<Made>,
    agents: Agents,
    dialogs: Dialogs,
    sessions: Sessions,
    /// The recent-sessions dialog, while open.
    recent: Option<SessionsDialog>,
    recent_sent: Sent<page::SessionsPage>,
    /// The command palette is open, so what it can go to is sent.
    palette: bool,
    palette_sent: Sent<page::Palette>,
    focused: bool,
    /// The view last reported, so an unchanged one is not sent again.
    reported: Option<View>,
    /// The rail draws from nearly everything here, so whatever changes it
    /// says so, and it is built once when the page is next told.
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
            status: StatusBar::new(settings.chime),
            project: None,
            repository: None,
            toggled: HashSet::new(),
            stage: Stage::Workspace,
            features: FeatureStage::new(settings.layouts),
            checkouts: CheckoutsStage::default(),
            shown: ShownPane::default(),
            awaited: Awaited::default(),
            agents: Agents::new(settings.last_agent),
            dialogs: Dialogs::new(settings.review),
            sessions: Sessions::default(),
            recent: None,
            recent_sent: Sent::default(),
            palette: false,
            palette_sent: Sent::default(),
            focused: true,
            reported: None,
            rail_changed: true,
            passing: Vec::new(),
            out: Vec::new(),
        }
    }

    // ---- The connection ----------------------------------------------------

    /// Trying to reach the daemon. The last tree stays on screen meanwhile;
    /// `detail` is what the last failed try said.
    pub fn connecting(&mut self, detail: Option<String>) {
        self.status.connecting(detail);
    }

    pub fn stopped(&mut self) {
        self.status.stopped();
    }

    pub fn is_stopped(&self) -> bool {
        self.status.is_stopped()
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
        self.status.connected(daemon, alarm);
        // A new connection knows nothing this window asked of the last.
        self.reported = None;
        self.awaited.forget_asked();
        self.features.forget_asked();
        self.sessions = Sessions::default();
        for msg in opening {
            self.apply(msg);
        }
        // An open sessions dialog asks this daemon again, or says it cannot.
        if let Some(repository) = self.recent.as_ref().map(SessionsDialog::repository) {
            let asked = self.ask_sessions(repository);
            if let Some(dialog) = &mut self.recent {
                dialog.reasked(asked);
            }
        }
        self.shown.reconnected(&mut self.out);
        self.finish()
    }

    /// A message from the daemon.
    pub fn receive(&mut self, msg: ServerMsg) -> Vec<ClientMsg> {
        self.apply(msg);
        self.finish()
    }

    fn apply(&mut self, msg: ServerMsg) {
        if let Some(effect) = self.shown.receive(&msg) {
            self.take(effect);
            return;
        }
        let Some(msg) = self.dialogs.receive(msg, &mut self.passing) else {
            return;
        };
        let Some(msg) = self.sessions.receive(msg) else {
            return;
        };
        match msg {
            ServerMsg::Tree(tree) => self.take_tree(tree),
            ServerMsg::PaneTelemetry { pane, telemetry } => {
                // The rail says nothing of telemetry; the tree stays true.
                self.tree.as_mut_slice().apply_telemetry(pane, telemetry);
            }
            ServerMsg::Created { request_id, created } => {
                self.awaited.answer(request_id, created);
                self.settle_made();
            }
            ServerMsg::BranchNotMerged { checkout, branch, commits } => {
                self.checkouts.refused(checkout, branch, &commits, argus_protocol::ago::now());
            }
            ServerMsg::Clipboard { pane, text } if Some(pane) == self.shown.pane() => {
                self.passing.push(Event::Clipboard { text });
            }
            ServerMsg::Error { message } => self.passing.push(Event::Notice { text: message }),
            ServerMsg::Hello(hello) => self.status.greeted(hello),
            ServerMsg::Restarting => self.passing.push(Event::Notice {
                text: "argusd is restarting".into(),
            }),
            ServerMsg::Decisions(board) => self.features.receive_board(*board, &self.tree, &mut self.out),
            ServerMsg::Templates(names) => {
                self.agents.receive(names);
                self.rail_changed = true;
            }
            ServerMsg::Tasks(list) => self.features.receive_tasks(*list),
            // The tree that follows says what closed and what stopping
            // ends; the rest answers requests this window does not make yet.
            _ => {}
        }
    }

    /// The tree held across a reconnect is the one compared with, so an
    /// agent that finished while the daemon was away still chimes; the
    /// first tree, compared with none, is quiet.
    fn take_tree(&mut self, tree: Vec<ProjectInfo>) {
        let watched = self.watched();
        let rings = transitions(self.tree.as_slice(), tree.as_slice())
            .iter()
            .any(|t| t.after.status.needs_you() && Some(t.pane.id) != watched);
        if rings && self.status.chimes() {
            self.passing.push(Event::Chime);
        }
        self.tree = tree;
        self.loaded = true;
        self.rail_changed = true;
        self.features.tree_changed();
        if !self.tree.iter().any(|p| Some(p.id) == self.project) {
            self.project = self.tree.first().map(|p| p.id);
        }
        // A pane that is gone is gone from the daemon's subscriptions too.
        if self.shown.pane().is_some_and(|pane| self.project_of(pane).is_none()) {
            self.shown.gone();
        }
        self.settle_made();
    }

    /// Acts on whatever the daemon made that the tree now holds.
    fn settle_made(&mut self) {
        for (created, then) in self.awaited.settle(&self.tree) {
            match (created, then) {
                (Created::Pane(pane), Made::Show | Made::ShowOverStages) => {
                    self.show_made(pane, then == Made::ShowOverStages);
                }
                (Created::Pane(pane), Made::Resumed) => {
                    self.recent = None;
                    self.show_made(pane, true);
                }
                // A worktree made from the Checkouts stage is the row it
                // lands on.
                (Created::Checkout(checkout), Made::SelectRow) => self.checkouts.select(format!("c{}", checkout.0)),
                _ => {}
            }
        }
    }

    fn show_made(&mut self, pane: PaneId, over_stages: bool) {
        let Some(info) = self.pane_info(pane) else {
            return;
        };
        // Read off the tree, not the request: a refused or mistaken spawn
        // never becomes the one offered first.
        let template = info.template.clone().filter(|_| info.kind == argus_protocol::PaneKind::Agent);
        if template.is_some_and(|template| self.agents.started(&template)) {
            self.rail_changed = true;
        }
        if over_stages {
            self.leave_stages();
        }
        self.show(pane);
    }

    fn pane_info(&self, pane: PaneId) -> Option<&argus_protocol::PaneInfo> {
        self.tree.panes().find(|p| p.id == pane)
    }

    // ---- Gestures ----------------------------------------------------------

    /// A call the page made. The clipboard's, the window frame's and a
    /// reconnect are not the session's: `commands` answers them before a
    /// call gets here, and the paste key too, which `pastes` names.
    pub fn call(&mut self, call: Call) -> Vec<ClientMsg> {
        match call {
            Call::Select { pane } => self.select(PaneId(pane)),
            Call::Resize { rows, cols } => self.resize(rows, cols),
            Call::Key { key } => self.key(&key),
            Call::Text { text } => self.text(&text),
            Call::Paste { text } => self.paste(text),
            Call::Wheel { notches, at } => self.wheel(notches, at),
            Call::Mouse { gesture, button, at } => self.mouse(gesture, button, at),
            Call::SpawnShell { checkout } => self.spawn_shell(CheckoutId(checkout)),
            Call::SpawnAgent { checkout, template } => self.spawn_agent(CheckoutId(checkout), template),
            Call::Close { pane } => self.close(PaneId(pane)),
            Call::Focus { focused } => self.focus(focused),
            Call::SelectProject { project } => self.select_project(ProjectId(project)),
            Call::ToggleRepository { repository } => self.toggle_repository(RepositoryId(repository)),
            Call::ChooseRepository { repository } => self.choose_repository(RepositoryId(repository)),
            Call::ShowFeatures { open } => self.show_features(open),
            Call::ShowCheckouts { open } => self.show_checkouts(open),
            Call::ShowComposer { open } => self.show_composer(open),
            Call::Compose { checkout, template, prompt } => self.compose(CheckoutId(checkout), template, prompt),
            Call::SelectCheckoutRow { key } => self.select_checkout_row(key),
            Call::FilterCheckouts { filter } => self.filter_checkouts(filter),
            Call::CheckoutsAction { action } => self.checkouts_action(action),
            Call::RemoveCheckout { checkout } => self.remove_checkout(CheckoutId(checkout)),
            Call::AnswerCheckouts { yes } => self.answer_checkouts(yes),
            Call::NewWorktree { name } => self.new_worktree(name),
            Call::OpenReview { checkout } => self.open_review(CheckoutId(checkout)),
            Call::ReviewAction { action } => self.review_action(action),
            Call::ReviewClick { target } => self.review_click(target),
            Call::ReviewRefresh => self.review_refresh(),
            Call::ReviewComment { body } => self.review_comment(body),
            Call::ReviewJump { gesture } => self.review_jump(gesture),
            Call::OpenHistory { checkout } => self.open_history(CheckoutId(checkout)),
            Call::HistoryAction { action, row } => self.history_action(action, row),
            Call::FilterHistory { filter } => self.filter_history(filter),
            Call::OpenSessions { repository } => self.open_sessions(RepositoryId(repository)),
            Call::SessionsAction { action, row } => self.sessions_action(action, row),
            Call::SearchSessions { search } => self.search_sessions(search),
            Call::FilterSessions { harness, checkout } => self.filter_sessions(harness, checkout),
            Call::Peek { slug } => self.peek(&slug),
            Call::SetLayout { layout } => self.set_layout(layout),
            Call::ToggleFolded => self.toggle_folded(),
            Call::SetChime { on } => self.set_chime(on),
            Call::Hold { slug, reason } => self.hold(&slug, reason),
            Call::Archive { slug, archived } => self.archive(&slug, archived),
            Call::GoToPane { pane } => self.go_to_pane(PaneId(pane)),
            Call::Palette { open } => self.palette(open),
            Call::Command { action } => self.command(action),
            Call::GoToCheckout { checkout } => self.go_to_checkout(CheckoutId(checkout)),
            Call::GoToFeature { slug } => self.go_to_feature(&slug),
            Call::PasteClipboard
            | Call::Copy { .. }
            | Call::Reconnect
            | Call::Minimize
            | Call::ToggleMaximize
            | Call::CloseWindow
            | Call::DragWindow
            | Call::SnapLayouts => self.finish(),
        }
    }

    /// Puts a pane on screen, leaving the Feature stage for it: a pane
    /// chosen on the rail is one the person means to look at and type into.
    fn select(&mut self, pane: PaneId) -> Vec<ClientMsg> {
        if self.project_of(pane).is_some() {
            self.leave_stages();
        }
        self.show(pane);
        self.finish()
    }

    fn show(&mut self, pane: PaneId) {
        let Some(project) = self.project_of(pane) else {
            return;
        };
        if self.shown.show(pane, &mut self.out) {
            self.project = Some(project);
            self.repository = None;
            self.rail_changed = true;
        }
    }

    fn hide(&mut self) {
        self.shown.hide(&mut self.out);
        self.rail_changed = true;
    }

    fn resize(&mut self, rows: u16, cols: u16) -> Vec<ClientMsg> {
        self.shown.resize(rows, cols, &mut self.out);
        self.finish()
    }

    /// A key pressed in the pane, dispatched from the keymap: the leader
    /// waits for the next key, a binding runs, and anything else is the
    /// program's. A key after the leader that nothing binds is dropped, as
    /// the terminal client drops it, rather than typed by surprise.
    fn key(&mut self, key: &PageKey) -> Vec<ClientMsg> {
        let after_leader = self.shown.take_leader();
        match keymap::in_pane(key, after_leader) {
            Some(action) => self.run(action),
            None if after_leader => {}
            None if keymap::is_leader(key) => self.shown.await_leader(),
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
        !self.shown.leader_waiting() && keymap::in_pane(key, false) == Some(Action::Paste)
    }

    fn run(&mut self, action: Action) {
        match action {
            Action::Leader => self.input(argus_protocol::keys::encode(&Key::new(KeyCode::Char(' '), Mods::CTRL))),
            Action::Fill => {
                self.shown.toggle_fill();
                self.rail_changed = true;
            }
            Action::Close => {
                if let Some(pane) = self.shown.pane() {
                    self.kill(pane);
                }
            }
            Action::Workspace => self.leave_stages(),
            Action::Feature => self.go(Stage::Feature),
            Action::Checkouts => self.go(Stage::Checkouts),
            Action::Compose => self.go(Stage::Composer),
            Action::OpenReview => {
                if let Some(checkout) = self.view().checkout {
                    self.review_checkout(checkout);
                }
            }
            Action::PageUp => self.on_view(|view| view.page(-1)),
            Action::PageDown => self.on_view(|view| view.page(1)),
            Action::Palette | Action::Shortcuts => self.passing.push(Event::Run { action }),
            // `commands` reads the clipboard before the key gets here; the
            // rest are the page's, for keys a pane never hears.
            _ => {}
        }
    }

    /// The wheel over the pane, `notches` up when negative, at the cell
    /// `at` under the pointer, decided as the terminal client decides it
    /// (`PaneView::wheel`).
    fn wheel(&mut self, notches: i32, at: (u16, u16)) -> Vec<ClientMsg> {
        self.on_view(|view| view.wheel(notches, at));
        self.finish()
    }

    /// A press, release or motion over the pane, at `[row, col]`, with
    /// `button` as `MouseEvent.button` counts (0 left, 1 middle, 2 right).
    /// The child gets it only as its mode reports it; a press, like typing,
    /// returns a parked view to live.
    fn mouse(&mut self, gesture: PageMouse, button: u8, at: (u16, u16)) -> Vec<ClientMsg> {
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
        if let Some(bytes) = mouse::encode(&MouseEvent { kind, row: at.0, col: at.1 }, self.shown.mouse()) {
            self.input(bytes);
        }
        self.finish()
    }

    /// Text an input method composed, typed as it stands.
    fn text(&mut self, text: &str) -> Vec<ClientMsg> {
        self.input(text.as_bytes().to_vec());
        self.finish()
    }

    /// Typing returns the view to the live screen: the child's echo lands
    /// there, and a parked view is not where input can be seen.
    fn input(&mut self, bytes: Vec<u8>) {
        if let (Some(pane), false) = (self.shown.pane(), bytes.is_empty()) {
            self.live();
            self.out.push(ClientMsg::Input { pane, bytes });
        }
    }

    /// A paste, as one event: the daemon brackets it only if the child asked.
    pub fn paste(&mut self, text: String) -> Vec<ClientMsg> {
        if let (Some(pane), false) = (self.shown.pane(), text.is_empty()) {
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
        let Some(pane) = self.shown.pane() else {
            return self.finish();
        };
        let note = if !self.status.daemon_can(PASTE_FILE) {
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
        self.on_view(PaneView::live);
    }

    /// A gesture on the shown pane's view.
    fn on_view(&mut self, gesture: impl FnOnce(&mut PaneView) -> Effect) {
        if let Some(effect) = self.shown.gesture(gesture) {
            self.take(effect);
        }
    }

    /// What the view asked to send and say.
    fn take(&mut self, effect: Effect) {
        self.out.extend(effect.send);
        if let Some(note) = effect.note {
            self.passing.push(Event::Notice { text: note.into() });
        }
    }

    /// The shown pane's text between two cells; empty with no pane.
    pub fn selected_text(&self, from: (u16, u16), to: (u16, u16)) -> String {
        self.shown.text(from, to)
    }

    /// Something this window says on the bar a while, as it does the
    /// daemon's refusals.
    pub fn note(&mut self, text: impl Into<String>) -> Vec<ClientMsg> {
        self.passing.push(Event::Notice { text: text.into() });
        self.finish()
    }

    /// Asks for a shell on a checkout, shown once it exists.
    fn spawn_shell(&mut self, checkout: CheckoutId) -> Vec<ClientMsg> {
        self.spawn(checkout, Made::Show, |request_id| ClientMsg::SpawnShell { checkout, request_id })
    }

    /// Asks for an agent from one of the daemon's templates on a checkout,
    /// shown once it exists as a shell is.
    fn spawn_agent(&mut self, checkout: CheckoutId, template: String) -> Vec<ClientMsg> {
        self.spawn_agent_asking(checkout, template, None, Made::Show)
    }

    /// An agent whose conversation opens with `prompt`, from the composer;
    /// an empty message starts it idle, as `spawn_agent` does.
    fn spawn_agent_asking(
        &mut self,
        checkout: CheckoutId,
        template: String,
        prompt: Option<String>,
        then: Made,
    ) -> Vec<ClientMsg> {
        if !self.agents.offers(&template) {
            return self.note(format!("argusd has no agent template called {template}"));
        }
        let prompt = prompt.filter(|text| !text.trim().is_empty());
        self.spawn(checkout, then, |request_id| ClientMsg::SpawnAgent {
            checkout,
            template,
            prompt,
            request_id,
        })
    }

    fn spawn(&mut self, checkout: CheckoutId, then: Made, ask: impl FnOnce(u64) -> ClientMsg) -> Vec<ClientMsg> {
        let exists = self
            .tree
            .iter()
            .flat_map(|p| &p.repositories)
            .flat_map(|r| &r.checkouts)
            .any(|c| c.id == checkout);
        if exists {
            let request_id = self.awaited.ask(then);
            self.out.push(ask(request_id));
        }
        self.finish()
    }

    /// Ends a pane's program and removes it. The pane stays on screen until
    /// a tree without it arrives, so a refused close leaves it showing.
    fn close(&mut self, pane: PaneId) -> Vec<ClientMsg> {
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
    fn focus(&mut self, focused: bool) -> Vec<ClientMsg> {
        self.focused = focused;
        self.finish()
    }

    fn select_project(&mut self, project: ProjectId) -> Vec<ClientMsg> {
        if self.project != Some(project) && self.tree.iter().any(|p| p.id == project) {
            self.project = Some(project);
            self.hide();
        }
        self.finish()
    }

    fn toggle_repository(&mut self, repository: RepositoryId) -> Vec<ClientMsg> {
        if !self.toggled.remove(&repository) {
            self.toggled.insert(repository);
        }
        self.rail_changed = true;
        self.finish()
    }

    /// A click on a repository's row: the first chooses it, for the
    /// Feature stage to read, and opens it; a second on the chosen one
    /// opens or closes it.
    fn choose_repository(&mut self, repository: RepositoryId) -> Vec<ClientMsg> {
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

    // ---- The stages ----------------------------------------------------------

    /// Puts `stage` up, and whichever was up away.
    fn go(&mut self, stage: Stage) {
        if self.stage != stage {
            self.stage = stage;
            self.rail_changed = true;
        }
    }

    /// Leaves whichever stage is open for the Workspace.
    fn leave_stages(&mut self) {
        self.go(Stage::Workspace);
    }

    /// Shows the Feature stage, or goes back to the Workspace.
    fn show_features(&mut self, open: bool) -> Vec<ClientMsg> {
        self.go(if open { Stage::Feature } else { Stage::Workspace });
        self.finish()
    }

    /// Shows the composer, or goes back to the Workspace.
    fn show_composer(&mut self, open: bool) -> Vec<ClientMsg> {
        self.go(if open { Stage::Composer } else { Stage::Workspace });
        self.finish()
    }

    /// Shows the Checkouts stage, or puts it away for the Workspace.
    fn show_checkouts(&mut self, open: bool) -> Vec<ClientMsg> {
        if open {
            self.go(Stage::Checkouts);
        } else if self.stage == Stage::Checkouts {
            self.go(Stage::Workspace);
        }
        self.finish()
    }

    fn peek(&mut self, slug: &str) -> Vec<ClientMsg> {
        self.features.peek(slug, &mut self.out);
        self.finish()
    }

    fn set_layout(&mut self, layout: Layout) -> Vec<ClientMsg> {
        let scope = self.scope();
        self.features.set_layout(layout, scope.as_ref());
        self.finish()
    }

    fn toggle_folded(&mut self) -> Vec<ClientMsg> {
        self.features.toggle_folded(&self.tree, &mut self.out);
        self.finish()
    }

    fn hold(&mut self, slug: &str, reason: Option<String>) -> Vec<ClientMsg> {
        let scope = self.scope();
        self.features.hold(slug, reason, scope.as_ref(), &mut self.out);
        self.finish()
    }

    fn archive(&mut self, slug: &str, archived: bool) -> Vec<ClientMsg> {
        let scope = self.scope();
        self.features.archive(slug, archived, scope.as_ref(), &mut self.out);
        self.finish()
    }

    /// Leaves the Feature stage for a pane working a feature.
    fn go_to_pane(&mut self, pane: PaneId) -> Vec<ClientMsg> {
        if self.project_of(pane).is_some() {
            self.leave_stages();
            self.show(pane);
        }
        self.finish()
    }

    /// The composer's start: a shell, or an agent from `template` opening
    /// with `prompt`. The composer stays up until the daemon makes the pane.
    fn compose(&mut self, checkout: CheckoutId, template: Option<String>, prompt: Option<String>) -> Vec<ClientMsg> {
        match template {
            Some(template) => self.spawn_agent_asking(checkout, template, prompt, Made::ShowOverStages),
            None => self.spawn(checkout, Made::ShowOverStages, |request_id| ClientMsg::SpawnShell { checkout, request_id }),
        }
    }

    /// `x` on a checkout in the rail: the Checkouts stage opens on its row
    /// and asks, since its question is the stage's and is drawn there.
    fn remove_checkout(&mut self, checkout: CheckoutId) -> Vec<ClientMsg> {
        let Some(repo) = self.in_project(checkout) else {
            return self.finish();
        };
        if let Err(text) = self.checkouts.ask_remove(&repo, checkout) {
            return self.note(text);
        }
        self.open_on_row(repo.id, checkout);
        self.finish()
    }

    /// A checkout chosen in the palette: the Checkouts stage opens on its
    /// row, where its keys are.
    fn go_to_checkout(&mut self, checkout: CheckoutId) -> Vec<ClientMsg> {
        if let Some(argus_protocol::RepositoryInfo { id: repository, .. }) = self.in_project(checkout) {
            self.open_on_row(repository, checkout);
        }
        self.finish()
    }

    /// A checkout of the rail's project, and the repository holding it.
    fn in_project(&self, checkout: CheckoutId) -> Option<argus_protocol::RepositoryInfo> {
        self.current_project()?.repositories.iter().find(|r| r.checkouts.iter().any(|c| c.id == checkout)).cloned()
    }

    fn open_on_row(&mut self, repository: RepositoryId, checkout: CheckoutId) {
        if self.repository != Some(repository) {
            self.repository = Some(repository);
            self.rail_changed = true;
        }
        self.go(Stage::Checkouts);
        self.checkouts.select(format!("c{}", checkout.0));
    }

    /// The answer to the stage's question: a yes does what it asked about.
    fn answer_checkouts(&mut self, yes: bool) -> Vec<ClientMsg> {
        if let Some(msg) = self.checkouts.answer(yes) {
            self.out.push(msg);
        }
        self.finish()
    }

    /// A worktree for the selected row: `name` is the new branch for a
    /// checkout row, and unused for a branch row, which brings its own.
    fn new_worktree(&mut self, name: Option<String>) -> Vec<ClientMsg> {
        let Some(repo) = self.chosen_repository().cloned() else {
            return self.finish();
        };
        let request_id = self.awaited.ask(Made::SelectRow);
        match self.checkouts.act(&repo, Verb::NewWorktree { name: name.as_deref(), request_id }) {
            Ok(msg) => self.out.push(msg),
            Err(text) => self.passing.push(Event::Notice { text }),
        }
        self.finish()
    }

    fn select_checkout_row(&mut self, key: String) -> Vec<ClientMsg> {
        self.checkouts.select(key);
        self.finish()
    }

    fn filter_checkouts(&mut self, filter: String) -> Vec<ClientMsg> {
        self.checkouts.set_filter(filter);
        self.finish()
    }

    /// A key or button of the Checkouts stage, acting on its selected row.
    fn checkouts_action(&mut self, action: Action) -> Vec<ClientMsg> {
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
                Some(Opened::Send(msg)) => self.out.push(*msg),
                Some(Opened::Say(text)) => self.passing.push(Event::Notice { text }),
                None => {}
            },
            Action::Fetch | Action::Pull => {
                let verb = if action == Action::Fetch { Verb::Fetch } else { Verb::Pull };
                match self.checkouts.act(&repo, verb) {
                    Ok(msg) => self.out.push(msg),
                    Err(text) => self.passing.push(Event::Notice { text }),
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
                Ok(Some(msg)) => self.out.push(msg),
                Ok(None) => {}
                Err(text) => self.passing.push(Event::Notice { text }),
            },
            Action::StartShell | Action::StartAgent => {
                let Some(checkout) = self.checkouts.checkout(&repo) else {
                    return self.note("give the branch a worktree first (N)");
                };
                return self.start_on(checkout, action);
            }
            _ => {}
        }
        self.finish()
    }

    /// A shell, or the agent offered first, on a checkout.
    fn start_on(&mut self, checkout: CheckoutId, action: Action) -> Vec<ClientMsg> {
        match (action, self.agents.first()) {
            (Action::StartShell, _) => self.spawn_shell(checkout),
            (_, Some(agent)) => self.spawn_agent(checkout, agent),
            (_, None) => self.note("argusd offers no agent to start"),
        }
    }

    // ---- The dialogs ---------------------------------------------------------

    /// Opens the review of a checkout's working changes.
    fn open_review(&mut self, checkout: CheckoutId) -> Vec<ClientMsg> {
        self.review_checkout(checkout);
        self.finish()
    }

    fn review_checkout(&mut self, checkout: CheckoutId) {
        self.dialogs.open_review(checkout, &self.tree, || self.awaited.name(), &mut self.out);
    }

    fn open_history(&mut self, checkout: CheckoutId) -> Vec<ClientMsg> {
        self.dialogs.open_history(checkout, &self.tree, || self.awaited.name(), &mut self.out);
        self.finish()
    }

    /// A key or click of the history dialog; `row` is a clicked row.
    fn history_action(&mut self, action: Action, row: Option<usize>) -> Vec<ClientMsg> {
        self.dialogs.history_action(action, row, &self.tree, || self.awaited.name(), &mut self.out);
        self.finish()
    }

    /// What the history's filter field says now.
    fn filter_history(&mut self, filter: String) -> Vec<ClientMsg> {
        self.dialogs.filter_history(filter, || self.awaited.name(), &mut self.out);
        self.finish()
    }

    /// A key or button of the review dialog. `E` asks for the file at the
    /// cursor in the person's editor; the daemon decides whether the editor
    /// is a pane or brings its own window, and a pane leaves the stages
    /// once shown, since E was pressed to look at it.
    fn review_action(&mut self, action: Action) -> Vec<ClientMsg> {
        if let Some((checkout, path, line)) = self.dialogs.review_action(action, &self.tree, &mut self.passing) {
            let request_id = self.awaited.ask(Made::ShowOverStages);
            self.out.push(ClientMsg::OpenInEditor { checkout, path, line, external: false, command: None, request_id });
        }
        self.finish()
    }

    fn review_jump(&mut self, gesture: ReviewJumpGesture) -> Vec<ClientMsg> {
        self.dialogs.review_jump(gesture);
        self.finish()
    }

    fn review_click(&mut self, target: ReviewClick) -> Vec<ClientMsg> {
        self.dialogs.review_click(target, &mut self.passing);
        self.finish()
    }

    fn review_refresh(&mut self) -> Vec<ClientMsg> {
        self.dialogs.review_refresh(|| self.awaited.name(), &mut self.out);
        self.finish()
    }

    fn review_comment(&mut self, body: Option<String>) -> Vec<ClientMsg> {
        self.dialogs.review_comment(body, &self.tree, &mut self.out, &mut self.passing);
        self.finish()
    }

    // ---- A repository's sessions --------------------------------------------

    /// Opens a repository's recent agent sessions, asking the daemon for
    /// them and for how each harness resumes one.
    fn open_sessions(&mut self, repository: RepositoryId) -> Vec<ClientMsg> {
        let name = self
            .current_project()
            .and_then(|p| p.repositories.iter().find(|r| r.id == repository))
            .map(|r| r.name.clone());
        let Some(name) = name else {
            return self.finish();
        };
        let asked = self.ask_sessions(repository);
        self.recent = Some(SessionsDialog::new(repository, name, !asked));
        self.finish()
    }

    /// Whether the daemon was asked: one that cannot list sessions is not.
    fn ask_sessions(&mut self, repository: RepositoryId) -> bool {
        let (list, resumes) = (self.awaited.name(), self.awaited.name());
        let Some(msg) = self.sessions.list(&self.status, repository, list) else {
            return false;
        };
        self.out.push(msg);
        self.out.extend(self.sessions.ask_resumes(&self.status, resumes));
        true
    }

    /// A key or click of the sessions dialog; `row` is a clicked row.
    fn sessions_action(&mut self, action: Action, row: Option<usize>) -> Vec<ClientMsg> {
        let Some(dialog) = &mut self.recent else {
            return self.finish();
        };
        let (sessions, tree) = (&self.sessions, self.tree.as_slice());
        let then = match (action, row) {
            (_, Some(row)) => {
                dialog.select_row(row, sessions, tree);
                SessionsThen::Nothing
            }
            (Action::Next | Action::Previous, _) => {
                dialog.move_by(if action == Action::Next { 1 } else { -1 }, sessions, tree);
                SessionsThen::Nothing
            }
            (Action::First | Action::Last, _) => {
                dialog.ends(action == Action::Last, sessions, tree);
                SessionsThen::Nothing
            }
            (Action::FilterHarness, _) => {
                dialog.cycle_harness(sessions);
                SessionsThen::Nothing
            }
            (Action::FilterCheckout, _) => {
                dialog.cycle_checkout(sessions, tree);
                SessionsThen::Nothing
            }
            (Action::Transcript, _) => {
                dialog.toggle_full();
                SessionsThen::Nothing
            }
            (Action::Choose, _) => dialog.choose(sessions, tree),
            (Action::Resume, _) => dialog.resume(sessions, tree),
            (Action::CloseLayer, _) => {
                self.recent = None;
                SessionsThen::Nothing
            }
            _ => SessionsThen::Nothing,
        };
        match then {
            SessionsThen::Nothing => {}
            SessionsThen::Say(text) => self.passing.push(Event::Notice { text }),
            // Only a pane the tree still holds: the listing may be older
            // than the pane's end.
            SessionsThen::GoTo(pane) if self.project_of(pane).is_some() => {
                self.recent = None;
                self.leave_stages();
                self.show(pane);
            }
            SessionsThen::GoTo(_) => self.passing.push(Event::Notice { text: "that session's pane has ended".into() }),
            // The dialog stays up until the pane is made and the tree
            // holds it, so a refused resume leaves it where it was.
            SessionsThen::Resume { checkout, harness, session_id } => {
                let request_id = self.awaited.ask(Made::Resumed);
                self.out.push(ClientMsg::ResumeSession { request_id, checkout, harness, session_id });
            }
        }
        self.finish()
    }

    /// What the sessions dialog's search field says now.
    fn search_sessions(&mut self, search: String) -> Vec<ClientMsg> {
        if let Some(dialog) = &mut self.recent {
            dialog.set_search(search);
        }
        self.finish()
    }

    fn filter_sessions(&mut self, harness: Option<String>, checkout: Option<String>) -> Vec<ClientMsg> {
        if let Some(dialog) = &mut self.recent {
            dialog.set_filters(harness, checkout);
        }
        self.finish()
    }

    // ---- The command palette ------------------------------------------------

    /// The palette opened or closed. While it is open it is sent what it
    /// can go to, and the chosen repository's board is asked for, so its
    /// features are among them wherever the palette opened.
    fn palette(&mut self, open: bool) -> Vec<ClientMsg> {
        self.palette = open;
        self.finish()
    }

    /// A feature chosen in the palette: the Feature stage opens on it, with
    /// held and archived features shown if it is one of them.
    fn go_to_feature(&mut self, slug: &str) -> Vec<ClientMsg> {
        self.go(Stage::Feature);
        self.features.reveal(slug, &self.tree, &mut self.out);
        self.finish()
    }

    /// A command chosen in the palette. One with nothing to act on says
    /// so, rather than leaving the person to wonder whether it ran.
    fn command(&mut self, action: Action) -> Vec<ClientMsg> {
        if !self.run_command(action) {
            self.passing.push(Event::Notice { text: format!("the palette has no command {action:?}") });
        }
        self.finish()
    }

    /// The pane the person is looking at: on screen, in a window they are
    /// in. It needs no chime, as the terminal client rings for no pane
    /// being typed in.
    fn watched(&self) -> Option<PaneId> {
        self.on_screen().filter(|_| self.focused)
    }

    /// The shown pane while the Workspace is up. A stage keeps the pane to
    /// come back to, but hides it and takes its keys away.
    fn on_screen(&self) -> Option<PaneId> {
        self.shown.pane().filter(|_| !matches!(self.stage, Stage::Feature | Stage::Checkouts))
    }

    fn set_chime(&mut self, on: bool) -> Vec<ClientMsg> {
        self.status.set_chime(on);
        self.finish()
    }

    /// Runs a palette command, saying whether `action` is one. Each acts
    /// where its key would, the stages' keys on their stage, or on this
    /// checkout (`on_this_checkout`).
    fn run_command(&mut self, action: Action) -> bool {
        if !keymap::is_command(action) {
            return false;
        }
        match action {
            Action::Workspace | Action::Feature | Action::Checkouts | Action::Compose | Action::Shortcuts => {
                self.run(action)
            }
            Action::Fill | Action::Close if self.on_screen().is_none() => {
                self.passing.push(Event::Notice { text: "no pane is on screen".into() });
            }
            Action::Fill | Action::Close => self.run(action),
            Action::Layout => {
                let scope = self.scope();
                let other = match self.features.layout(scope.as_ref()) {
                    Layout::List => Layout::Board,
                    Layout::Board => Layout::List,
                };
                self.features.set_layout(other, scope.as_ref());
            }
            Action::Folded => {
                self.features.toggle_folded(&self.tree, &mut self.out);
            }
            Action::Branches => {
                self.go(Stage::Checkouts);
                self.checkouts.toggle_branches();
            }
            Action::Fetch | Action::Pull | Action::OpenReview | Action::OpenHistory | Action::StartShell | Action::StartAgent => {
                self.on_this_checkout(action);
            }
            // The repository the rail has chosen, as the Feature stage
            // reads; its sessions span every one of its checkouts.
            Action::OpenSessions => match self.chosen_repository().map(|r| r.id) {
                Some(repository) => {
                    let sent = self.open_sessions(repository);
                    self.out.extend(sent);
                }
                None => self.passing.push(Event::Notice { text: "no repository to list the sessions of".into() }),
            },
            _ => return false,
        }
        true
    }

    /// A checkout command acts on the Checkouts stage's selected row while
    /// that stage is open, as its key there does, and else on the checkout
    /// of the pane on screen.
    fn on_this_checkout(&mut self, action: Action) {
        // Each of these finishes on its own; what it sent is this
        // command's, and goes out when the command finishes.
        let sent = if self.stage == Stage::Checkouts {
            self.checkouts_action(action)
        } else if let Some(checkout) = self.shown_checkout() {
            match action {
                Action::Fetch => vec![ClientMsg::Fetch { checkout }],
                Action::Pull => vec![ClientMsg::Pull { checkout }],
                Action::OpenReview => self.open_review(checkout),
                Action::OpenHistory => self.open_history(checkout),
                _ => self.start_on(checkout, action),
            }
        } else {
            self.note("show a pane, or pick a checkout in Checkouts, first")
        };
        self.out.extend(sent);
    }

    fn shown_checkout(&self) -> Option<CheckoutId> {
        let pane = self.on_screen()?;
        self.tree
            .iter()
            .flat_map(|p| &p.repositories)
            .flat_map(|r| &r.checkouts)
            .find(|c| c.panes.iter().any(|p| p.id == pane))
            .map(|c| c.id)
    }

    /// The settings, when a change asks for them to be saved.
    pub fn take_settings(&mut self) -> Option<Settings> {
        // All four, not short-circuited: each flag is cleared as it is read.
        let layouts = self.features.take_layouts_changed();
        let agent = self.agents.take_changed();
        let chime = self.status.take_chime_changed();
        let review = self.dialogs.take_layout_changed();
        (layouts || agent || chime || review).then(|| Settings {
            layouts: self.features.layouts().clone(),
            last_agent: self.agents.last().cloned(),
            chime: self.status.chimes(),
            review: self.dialogs.layout(),
        })
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
        let holding_shown = self.shown.pane().and_then(|pane| {
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
        // The session under the dialog's cursor is read once it is there,
        // however the cursor got there: a key, a filter, or the listing.
        if let Some(wanted) = self.recent.as_ref().and_then(|d| d.wants_read(&self.sessions, &self.tree)).cloned() {
            let request_id = self.awaited.name();
            self.out.extend(self.sessions.read(&self.status, &wanted, request_id));
        }
        let reads_board = matches!(self.stage, Stage::Feature | Stage::Checkouts) || self.palette;
        self.features.set_board_wanted(reads_board);
        // Whatever changed may have changed which board the stage reads.
        let scope = self.scope();
        if self.status.is_connected() {
            self.features.refresh(scope.as_ref(), &mut self.out);
        }
        if self.status.daemon_can(FOCUS) {
            let view = self.view();
            if self.reported.as_ref() != Some(&view) {
                self.reported = Some(view.clone());
                self.out.push(ClientMsg::View(view));
            }
        }
        std::mem::take(&mut self.out)
    }

    fn view(&self) -> View {
        View {
            project: self.project,
            checkout: self.shown_checkout(),
            pane: self.shown.pane(),
            active: self.focused,
        }
    }

    /// What the page has not heard yet: the status and the rail when they
    /// changed, and whatever passed through. `now` is seconds since the
    /// Unix epoch.
    pub fn events(&mut self, now: u64) -> Vec<Event> {
        let mut events = Vec::new();
        if let Some(status) = self.status.take() {
            events.push(Event::Status(status));
        }
        if std::mem::take(&mut self.rail_changed) {
            events.push(Event::Rail(Box::new(self.rail(now))));
        }
        let scope = self.scope();
        if let Some(features) = self.features.take(scope.as_ref(), &self.tree) {
            events.push(Event::Features(Box::new(features)));
        }
        events.extend(self.dialogs.take(&self.tree, now));
        let recent = self.recent.as_ref().map(|d| d.page(&self.sessions, &self.tree, now));
        if let Some(page) = self.recent_sent.take(recent) {
            events.push(Event::Sessions(Box::new(page)));
        }
        let listing = self.palette.then(|| palette::build(self.current_project(), self.features.board(), &self.tree));
        if let Some(listing) = self.palette_sent.take(listing) {
            events.push(Event::Palette(Box::new(listing)));
        }
        if self.stage == Stage::Checkouts {
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
        self.status.resend();
        self.dialogs.reload();
        self.recent_sent.forget();
        // A page that has just loaded has no palette open.
        self.palette = false;
        self.palette_sent.forget();
        self.rail_changed = true;
        self.shown.refresh();
        let mut events = vec![Event::Keymap { groups: keymap::groups(), commands: keymap::commands() }];
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
        self.shown.take_frame().map(Event::Frame)
    }

    fn rail(&self, now: u64) -> page::Rail {
        let choices = Choices {
            project: self.project,
            shown: self.shown.pane(),
            toggled: &self.toggled,
            now,
        };
        let mut rail = rail::build(&self.tree, &choices, self.loaded);
        rail.repository = self.chosen_repository().map(|r| r.id.0);
        rail.stage = self.stage;
        rail.filled = self.shown.filled() && self.stage == Stage::Workspace && self.shown.pane().is_some();
        rail.templates = self.agents.offered();
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
        assert_eq!(s.shown.pane(), Some(PaneId(1)), "nothing is predicted");
        let mut gone = tree();
        gone[0].repositories[0].checkouts[0].panes.remove(0);
        s.receive(with_tree(gone));
        assert_eq!(s.shown.pane(), None);
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

    fn leader_then(s: &mut Session, key: &str) -> Vec<ClientMsg> {
        s.key(&PageKey { key: " ".into(), ctrl: true, ..PageKey::default() });
        s.key(&PageKey { key: key.into(), ..PageKey::default() })
    }

    fn reviews(msgs: &[ClientMsg]) -> Vec<(CheckoutId, argus_protocol::ReviewBase)> {
        msgs.iter()
            .filter_map(|m| match m {
                ClientMsg::Review { checkout, base, .. } => Some((*checkout, *base)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_leader_and_tab_review_the_shown_panes_checkout() {
        let mut s = session(without_focus());
        assert!(reviews(&leader_then(&mut s, "Tab")).is_empty(), "no pane, no checkout");
        s.select(PaneId(3));
        let sent = leader_then(&mut s, "Tab");
        assert_eq!(reviews(&sent).len(), 2, "both sides of checkout 200: {:?}", names(&sent));
        assert!(reviews(&sent).iter().all(|(c, _)| *c == CheckoutId(200)));
        assert!(s.dialogs.review().is_some());
    }



    fn reviewed(base: argus_protocol::ReviewBase, request_id: u64) -> ServerMsg {
        use argus_protocol::{ChangeKind, DiffLine, FileDiff, Hunk, LineKind, Review};
        ServerMsg::Review(Review {
            request_id,
            checkout: CheckoutId(100),
            base,
            files: vec![FileDiff {
                path: "src/brief.rs".into(),
                old_path: None,
                kind: ChangeKind::Modified,
                hunks: vec![Hunk {
                    header: "@@ -1 +1 @@".into(),
                    lines: vec![DiffLine { kind: LineKind::Added, old_lineno: None, new_lineno: Some(4), text: "x".into(), spans: Vec::new() }],
                }],
                note: None,
            }],
            commit: None,
        })
    }

    #[test]
    fn e_in_the_review_opens_the_cursors_line_and_its_pane_leaves_the_stage() {
        let mut s = session(without_focus());
        s.show_checkouts(true);
        let sent = s.open_review(CheckoutId(100));
        let [ClientMsg::Review { request_id: unstaged, .. }, ..] = sent.as_slice() else {
            panic!("{:?}", names(&sent));
        };
        s.receive(reviewed(argus_protocol::ReviewBase::Unstaged, *unstaged));
        s.review_action(Action::Next);
        let sent = s.review_action(Action::OpenEditor);
        let [ClientMsg::OpenInEditor { checkout, path, line, external: false, command: None, request_id }] = sent.as_slice() else {
            panic!("{:?}", names(&sent));
        };
        assert_eq!((*checkout, path.as_str(), *line), (CheckoutId(100), "src/brief.rs", Some(4)));
        assert!(s.dialogs.review().is_none(), "the review gives way");
        assert!(s.stage == Stage::Checkouts, "nothing predicted");
        s.receive(ServerMsg::Created { request_id: *request_id, created: Some(Created::Pane(PaneId(9))) });
        let mut grown = tree();
        grown[0].repositories[0].checkouts[0].panes.push(pane(9, PaneKind::Editor, "brief.rs", PaneStatus::Idle));
        s.receive(with_tree(grown));
        assert_eq!(s.shown.pane(), Some(PaneId(9)));
        assert!(s.stage != Stage::Checkouts, "the editor is on screen");
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


    fn chimes(s: &mut Session) -> usize {
        s.events(0).iter().filter(|e| matches!(e, Event::Chime)).count()
    }

    fn with_status(pane: usize, status: PaneStatus) -> ServerMsg {
        let mut next = tree();
        next[0].repositories[0].checkouts[0].panes[pane].status = status;
        with_tree(next)
    }

    #[test]
    fn an_agent_coming_to_need_you_chimes_once() {
        let mut s = session(without_focus());
        s.events(0);
        s.receive(with_status(0, PaneStatus::NeedsReview));
        assert_eq!(chimes(&mut s), 1);
        s.receive(with_status(0, PaneStatus::NeedsReview));
        assert_eq!(chimes(&mut s), 0, "still needing you is not news");
        s.receive(with_status(0, PaneStatus::Working));
        assert_eq!(chimes(&mut s), 0, "starting work is not worth a sound");
    }

    #[test]
    fn the_first_tree_is_quiet_however_many_need_you() {
        let mut s = Session::default();
        s.connected(Some(without_focus()), None, vec![with_status(0, PaneStatus::Waiting)]);
        assert_eq!(chimes(&mut s), 0);
    }

    #[test]
    fn the_pane_you_are_looking_at_does_not_chime_until_you_look_away() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.events(0);
        s.receive(with_status(0, PaneStatus::NeedsReview));
        assert_eq!(chimes(&mut s), 0, "on screen in a focused window");

        s.receive(with_status(0, PaneStatus::Working));
        s.focus(false);
        s.receive(with_status(0, PaneStatus::NeedsReview));
        assert_eq!(chimes(&mut s), 1, "the window is in the background");

        s.receive(with_status(0, PaneStatus::Working));
        s.focus(true);
        s.show_features(true);
        s.receive(with_status(0, PaneStatus::NeedsReview));
        assert_eq!(chimes(&mut s), 1, "another stage hides it");
    }

    #[test]
    fn an_agent_that_finished_while_the_daemon_was_away_still_chimes() {
        let mut s = session(without_focus());
        s.connecting(None);
        s.connected(Some(without_focus()), None, vec![with_status(0, PaneStatus::NeedsReview)]);
        assert_eq!(chimes(&mut s), 1);
    }

    #[test]
    fn turning_the_chime_off_silences_it_and_asks_to_be_saved() {
        let mut s = session(without_focus());
        assert!(s.take_settings().is_none());
        s.set_chime(false);
        let Some(Event::Status(status)) = s.events(0).into_iter().next() else { panic!() };
        assert!(!status.chime);
        assert_eq!(s.take_settings().map(|s| s.chime), Some(false));
        assert!(s.take_settings().is_none(), "saved once");

        s.receive(with_status(0, PaneStatus::Failed));
        assert_eq!(chimes(&mut s), 0);
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
        assert_eq!((rail.stage, rail.repository), (Stage::Feature, Some(10)));
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
        assert!(s.stage != Stage::Feature);
        s.show_features(true);
        s.select(PaneId(99));
        assert!(s.stage == Stage::Feature, "a pane that is not there changes nothing");
    }

    #[test]
    fn going_to_a_features_pane_leaves_the_stage_for_it() {
        let mut s = session(without_focus());
        s.show_features(true);
        assert_eq!(names(&s.go_to_pane(PaneId(1))), ["subscribe 1"]);
        assert!(s.stage != Stage::Feature);
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

    // How the wheel and a parked view behave is `pane_view`'s and tested
    // there; these say the window wires its gestures and messages through.

    #[test]
    fn the_wheel_parks_the_view_and_asks_further_up_by_line() {
        let mut s = showing(5, false);
        assert_eq!(scrollback(&s.wheel(-1, (0, 0))), [(3, None)]);
        s.receive(history(3, 50, Some(100)));
        assert_eq!(scrollback(&s.wheel(-1, (0, 0))), [(6, Some(97))]);
        assert_eq!(scrollback(&s.wheel(1, (0, 0))), [(3, Some(100))]);
    }

    #[test]
    fn a_resize_re_reads_the_parked_rows_at_the_new_width() {
        let mut s = showing(5, false);
        s.wheel(-1, (0, 0));
        s.receive(history(3, 50, Some(100)));
        let resized = s.receive(ServerMsg::PaneRows {
            pane: PaneId(2),
            rows: 8,
            cols: 20,
            runs: Vec::new(),
            cursor: argus_protocol::Cursor::default(),
            mouse: argus_protocol::MouseTracking::default(),
            alternate_screen: false,
        });
        assert_eq!(scrollback(&resized), [(3, Some(100))]);
    }

    #[test]
    fn typing_or_pasting_returns_the_view_to_the_live_screen() {
        let mut s = showing(5, false);
        s.wheel(-1, (0, 0));
        s.key(&PageKey { key: "a".into(), ..PageKey::default() });
        assert!(s.shown.view().unwrap().parked().is_none());

        s.wheel(-1, (0, 0));
        s.paste("x".into());
        assert!(s.shown.view().unwrap().parked().is_none());
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
        assert!(s.stage == Stage::Feature);
        s.key(&ctrl_space());
        s.key(&press("1"));
        assert!(s.stage != Stage::Feature);
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
    fn a_history_with_nothing_behind_the_screen_leaves_it_live_and_says_so() {
        let mut s = showing(5, false);
        s.events(0);
        s.wheel(-1, (0, 0));
        s.receive(history(0, 0, None));
        assert!(s.shown.view().unwrap().parked().is_none());
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text == "nothing further back")));
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
        let Some(ClientMsg::SpawnAgent { checkout, template, request_id, .. }) = sent.first() else {
            panic!("{sent:?}");
        };
        assert_eq!((*checkout, template.as_str()), (CheckoutId(100), "claude"));
        s.receive(ServerMsg::Created { request_id: *request_id, created: Some(Created::Pane(PaneId(2))) });
        assert_eq!(s.shown.pane(), Some(PaneId(2)));

        s.events(0);
        assert!(s.spawn_agent(CheckoutId(100), "nobody".into()).is_empty());
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text.contains("nobody"))));
    }

    #[test]
    fn the_composer_stays_up_until_the_daemon_makes_its_pane() {
        let mut s = session(without_focus());
        s.receive(ServerMsg::Templates(vec!["claude".into()]));
        s.show_composer(true);
        assert_eq!(s.rail(0).stage, Stage::Composer);

        let sent = s.compose(CheckoutId(100), Some("claude".into()), Some("fix the build".into()));
        let Some(ClientMsg::SpawnAgent { prompt, request_id, .. }) = sent.first() else {
            panic!("{sent:?}");
        };
        assert_eq!(prompt.as_deref(), Some("fix the build"));
        assert_eq!(s.rail(0).stage, Stage::Composer, "a refused start keeps what was typed");

        s.receive(ServerMsg::Created { request_id: *request_id, created: Some(Created::Pane(PaneId(2))) });
        assert_eq!((s.rail(0).stage, s.shown.pane()), (Stage::Workspace, Some(PaneId(2))));
    }

    #[test]
    fn an_empty_message_starts_the_agent_idle() {
        let mut s = session(without_focus());
        s.receive(ServerMsg::Templates(vec!["claude".into()]));
        let sent = s.compose(CheckoutId(100), Some("claude".into()), Some("  \n".into()));
        assert!(matches!(sent.first(), Some(ClientMsg::SpawnAgent { prompt: None, .. })), "{sent:?}");
    }

    #[test]
    fn the_composer_starts_a_shell_without_a_template() {
        let mut s = session(without_focus());
        let sent = s.compose(CheckoutId(100), None, Some("ignored".into()));
        assert!(matches!(sent.first(), Some(ClientMsg::SpawnShell { checkout: CheckoutId(100), .. })), "{sent:?}");
    }

    #[test]
    fn another_stage_puts_the_composer_away() {
        let mut s = session(without_focus());
        s.show_composer(true);
        s.show_checkouts(true);
        assert_eq!(s.rail(0).stage, Stage::Checkouts);
        s.show_composer(true);
        s.show_features(false);
        assert_eq!(s.rail(0).stage, Stage::Workspace);
    }

    fn offered(s: &mut Session) -> Vec<String> {
        s.events(0)
            .into_iter()
            .find_map(|e| match e {
                Event::Rail(rail) => Some(rail.templates),
                _ => None,
            })
            .expect("the rail is drawn again")
    }

    /// Asks for `template` on checkout 100 and has the daemon make pane 7
    /// running it.
    fn start(s: &mut Session, template: &str) {
        let sent = s.spawn_agent(CheckoutId(100), template.into());
        let Some(ClientMsg::SpawnAgent { request_id, .. }) = sent.first() else {
            panic!("{sent:?}");
        };
        s.receive(ServerMsg::Created { request_id: *request_id, created: Some(Created::Pane(PaneId(7))) });
        let mut grown = tree();
        let made = argus_protocol::PaneInfo {
            template: Some(template.into()),
            ..pane(7, PaneKind::Agent, "Trim context", PaneStatus::Working)
        };
        grown[0].repositories[0].checkouts[0].panes.push(made);
        s.receive(with_tree(grown));
    }

    #[test]
    fn the_agent_last_started_is_offered_first_and_remembered() {
        let mut s = session(without_focus());
        s.receive(ServerMsg::Templates(vec!["claude".into(), "codex".into()]));
        assert_eq!(offered(&mut s), ["claude", "codex"]);

        start(&mut s, "codex");

        assert_eq!(s.shown.pane(), Some(PaneId(7)));
        assert_eq!(offered(&mut s), ["codex", "claude"]);
        let saved = s.take_settings().expect("a new last agent is saved");
        assert_eq!(saved.last_agent.as_deref(), Some("codex"));
        assert!(s.take_settings().is_none(), "saved once");
    }


    #[test]
    fn a_refused_agent_is_said_and_changes_nothing() {
        let mut s = session(without_focus());
        s.receive(ServerMsg::Templates(vec!["claude".into(), "codex".into()]));
        s.events(0);
        let sent = s.spawn_agent(CheckoutId(100), "codex".into());
        let Some(ClientMsg::SpawnAgent { request_id, .. }) = sent.first() else {
            panic!("{sent:?}");
        };

        let refusal = "claude is already working here, and this project allows one agent per checkout — make a worktree for another";
        let mut answered = s.receive(ServerMsg::Created { request_id: *request_id, created: None });
        answered.extend(s.receive(ServerMsg::Error { message: refusal.into() }));

        assert!(answered.is_empty(), "nothing to subscribe to: {:?}", names(&answered));
        assert_eq!(s.shown.pane(), None);
        let events = s.events(0);
        assert!(events.iter().any(|e| matches!(e, Event::Notice { text } if text == refusal)));
        s.receive(with_tree(tree()));
        assert_eq!(offered(&mut s), ["claude", "codex"], "a refused agent is not the last one started");
        assert!(s.take_settings().is_none());
    }

    #[test]
    fn the_checkouts_stage_opens_alone_and_asks_for_the_board_it_names_features_from() {
        let mut s = session(without_focus());
        s.go(Stage::Feature);
        let sent = s.show_checkouts(true);
        assert!(s.stage == Stage::Checkouts, "one stage at a time");
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::GetDecisions { .. })), "{sent:?}");
        let events = s.events(0);
        assert!(events.iter().any(|e| matches!(e, Event::Checkouts(table) if table.repository.as_deref() == Some("argus"))));
        let Some(Event::Rail(rail)) = events.iter().find(|e| matches!(e, Event::Rail(_))) else { panic!() };
        assert_eq!(rail.stage, Stage::Checkouts);
    }

    #[test]
    fn enter_on_a_checkout_row_shows_its_pane_in_the_workspace() {
        let mut s = session(without_focus());
        s.show_checkouts(true);
        s.select_checkout_row("c100".into());
        s.checkouts_action(Action::Choose);
        assert_eq!(s.shown.pane(), Some(PaneId(1)));
        assert!(s.stage != Stage::Checkouts);
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
        assert!(s.stage == Stage::Checkouts);
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

    /// Each command the palette lists runs something here, from the pane
    /// on screen's Workspace as from either stage; none falls through to
    /// "no command".
    #[test]
    fn every_palette_command_dispatches() {
        for command in keymap::commands() {
            for stage in [Stage::Workspace, Stage::Feature, Stage::Checkouts] {
                let mut s = session(without_focus());
                s.receive(ServerMsg::Templates(vec!["claude".into()]));
                s.select(PaneId(1));
                s.go(stage);
                assert!(s.run_command(command.action), "{} does not dispatch in {stage:?}", command.label);
            }
        }
    }

    /// Every call the page can make, written as the page writes it.
    const CALLS: [&str; 56] = [
        r#"{"call":"select","pane":1}"#,
        r#"{"call":"resize","rows":30,"cols":100}"#,
        r#"{"call":"key","key":{"key":"a","code":"KeyA","ctrl":false,"alt":false,"shift":false,"meta":false}}"#,
        r#"{"call":"text","text":"é"}"#,
        r#"{"call":"paste","text":"ls"}"#,
        r#"{"call":"paste_clipboard"}"#,
        r#"{"call":"wheel","notches":-1,"at":[0,0]}"#,
        r#"{"call":"mouse","gesture":"down","button":0,"at":[1,3]}"#,
        r#"{"call":"copy","from":[0,0],"to":[0,4]}"#,
        r#"{"call":"spawn_shell","checkout":100}"#,
        r#"{"call":"spawn_agent","checkout":100,"template":"claude"}"#,
        r#"{"call":"close","pane":2}"#,
        r#"{"call":"focus","focused":false}"#,
        r#"{"call":"select_project","project":2}"#,
        r#"{"call":"toggle_repository","repository":10}"#,
        r#"{"call":"choose_repository","repository":10}"#,
        r#"{"call":"show_features","open":true}"#,
        r#"{"call":"show_checkouts","open":true}"#,
        r#"{"call":"show_composer","open":true}"#,
        r#"{"call":"compose","checkout":100,"template":null,"prompt":null}"#,
        r#"{"call":"select_checkout_row","key":"c100"}"#,
        r#"{"call":"filter_checkouts","filter":"ma"}"#,
        r#"{"call":"checkouts_action","action":"next"}"#,
        r#"{"call":"remove_checkout","checkout":100}"#,
        r#"{"call":"answer_checkouts","yes":false}"#,
        r#"{"call":"new_worktree","name":"spike"}"#,
        r#"{"call":"open_review","checkout":100}"#,
        r#"{"call":"review_action","action":"split"}"#,
        r#"{"call":"review_click","target":{"on":"row","row":0,"extend":true}}"#,
        r#"{"call":"review_refresh"}"#,
        r#"{"call":"review_comment","body":null}"#,
        r#"{"call":"review_jump","gesture":{"on":"query","text":"br"}}"#,
        r#"{"call":"open_history","checkout":100}"#,
        r#"{"call":"history_action","action":"next","row":null}"#,
        r#"{"call":"filter_history","filter":"fix"}"#,
        r#"{"call":"open_sessions","repository":10}"#,
        r#"{"call":"sessions_action","action":"next","row":null}"#,
        r#"{"call":"search_sessions","search":"fix"}"#,
        r#"{"call":"filter_sessions","harness":"claude","checkout":null}"#,
        r#"{"call":"peek","slug":"trim"}"#,
        r#"{"call":"set_layout","layout":"board"}"#,
        r#"{"call":"toggle_folded"}"#,
        r#"{"call":"set_chime","on":false}"#,
        r#"{"call":"hold","slug":"trim","reason":"waiting"}"#,
        r#"{"call":"archive","slug":"trim","archived":true}"#,
        r#"{"call":"go_to_pane","pane":1}"#,
        r#"{"call":"palette","open":true}"#,
        r#"{"call":"command","action":"fill"}"#,
        r#"{"call":"go_to_checkout","checkout":100}"#,
        r#"{"call":"go_to_feature","slug":"trim"}"#,
        r#"{"call":"reconnect"}"#,
        r#"{"call":"minimize"}"#,
        r#"{"call":"toggle_maximize"}"#,
        r#"{"call":"close_window"}"#,
        r#"{"call":"drag_window"}"#,
        r#"{"call":"snap_layouts"}"#,
    ];

    /// Each call reads as the page writes it and goes through the session,
    /// from the Workspace as from either stage. The names are read off the
    /// generated type, so a call added without a line in `CALLS` fails here.
    #[test]
    fn every_call_the_page_makes_is_read_and_dispatched() {
        let union = <Call as ts_rs::TS>::inline(&ts_rs::Config::default());
        let declared: HashSet<&str> = union.split(r#"{ "call": ""#).skip(1).filter_map(|s| s.split('"').next()).collect();
        let written: HashSet<String> = CALLS.iter().map(|c| serde_json::from_str::<serde_json::Value>(c).unwrap()["call"].as_str().unwrap().to_owned()).collect();
        assert_eq!(declared.len(), written.len(), "declared {declared:?}");
        assert!(declared.iter().all(|name| written.contains(*name)), "declared {declared:?}");
        for stage in [Stage::Workspace, Stage::Feature, Stage::Checkouts] {
            let mut s = session(Hello::this_build());
            s.receive(ServerMsg::Templates(vec!["claude".into()]));
            s.select(PaneId(1));
            s.go(stage);
            for json in CALLS {
                let call: Call = serde_json::from_str(json).unwrap_or_else(|e| panic!("{json}: {e}"));
                s.call(call);
            }
        }
    }

    /// A stage hides the pane it will come back to, so the palette's pane
    /// and checkout commands find none on screen there.
    #[test]
    fn a_stage_leaves_no_pane_on_screen_for_the_palettes_commands() {
        for stage in ["feature", "checkouts"] {
            let mut s = session(without_focus());
            s.select(PaneId(1));
            if stage == "feature" {
                s.show_features(true);
            } else {
                s.show_checkouts(true);
            }
            s.events(0);
            let sent = s.command(Action::Close);
            assert!(!sent.iter().any(|m| matches!(m, ClientMsg::Kill { .. })), "{stage}: {sent:?}");
            assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text == "no pane is on screen")));
        }
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.show_features(true);
        assert!(!s.command(Action::Fetch).iter().any(|m| matches!(m, ClientMsg::Fetch { .. })));
    }

    #[test]
    fn an_action_the_palette_does_not_list_is_refused() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.events(0);
        assert!(s.command(Action::Delete).is_empty());
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text.contains("no command"))));
    }

    #[test]
    fn a_checkout_command_acts_on_the_pane_on_screens_checkout() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        assert!(matches!(s.command(Action::Fetch).as_slice(), [ClientMsg::Fetch { checkout: CheckoutId(100) }]));
        let sent = s.command(Action::StartShell);
        assert!(matches!(sent.as_slice(), [ClientMsg::SpawnShell { checkout: CheckoutId(100), .. }]), "{sent:?}");
        s.command(Action::OpenReview);
        assert_eq!(s.dialogs.review().map(|r| r.checkout()), Some(CheckoutId(100)));
    }

    #[test]
    fn a_checkout_command_with_no_checkout_says_so() {
        let mut s = session(without_focus());
        s.events(0);
        assert!(s.command(Action::Pull).is_empty());
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text.contains("show a pane"))));
    }

    #[test]
    fn the_leader_and_question_mark_ask_the_page_for_the_sheet() {
        let mut s = showing(5, false);
        s.events(0);
        s.key(&ctrl_space());
        assert!(s.key(&press("?")).is_empty(), "nothing typed");
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Run { action: Action::Shortcuts })));
        s.key(&ctrl_space());
        s.key(&PageKey { ctrl: true, ..press("k") });
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Run { action: Action::Palette })));
    }

    #[test]
    fn an_open_palette_is_sent_what_it_can_go_to_and_asks_for_the_board() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.events(0);
        let sent = s.palette(true);
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::GetDecisions { .. })), "{sent:?}");
        let listed = s.events(0).into_iter().find_map(|e| match e {
            Event::Palette(palette) => Some(palette),
            _ => None,
        });
        let listed = listed.expect("the palette is sent");
        assert!(listed.open && !listed.features_loaded);
        assert_eq!(listed.panes.iter().map(|p| p.id).collect::<Vec<_>>(), [1, 2], "the rail's project's panes");
        assert!(s.events(0).iter().all(|e| !matches!(e, Event::Palette(_))), "unchanged, not sent again");
        s.palette(false);
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Palette(p) if !p.open)));
    }

    #[test]
    fn a_checkout_chosen_in_the_palette_opens_the_checkouts_stage_on_it() {
        let mut s = session(without_focus());
        let mut more = tree();
        more[0].repositories[0].checkouts.push(checkout(101, "spike", false, Vec::new()));
        s.receive(with_tree(more));
        s.select(PaneId(1));
        s.go_to_checkout(CheckoutId(101));
        assert!(s.stage == Stage::Checkouts);
        assert_eq!(s.checkouts.selected_key(), Some("c101"));
    }

    fn listed_session(id: &str, pane: Option<u64>) -> argus_protocol::sessions::Session {
        argus_protocol::sessions::Session {
            harness: "claude".into(),
            session_id: id.into(),
            checkout: CheckoutId(100),
            removed_worktree: None,
            title: Some(format!("{id} work")),
            started: None,
            ended: None,
            modified: None,
            turns: None,
            pane: pane.map(PaneId),
        }
    }

    /// A session whose sessions dialog is open on `argus` and has heard
    /// the listing — a live session on pane 1, then an ended one — and
    /// that claude resumes by id.
    fn listing_sessions() -> Session {
        let mut s = session(Hello::this_build());
        let sent = s.call(Call::OpenSessions { repository: 10 });
        let [ClientMsg::ListSessions { request_id: list, repository: RepositoryId(10) }, ClientMsg::ListResumes { request_id: resumes }] =
            sent.as_slice()
        else {
            panic!("{sent:?}");
        };
        let (list, resumes) = (*list, *resumes);
        let argv = vec!["claude".to_string(), "--resume".into(), "{session_id}".into()];
        s.receive(ServerMsg::Resumes { request_id: resumes, resumes: vec![("claude".into(), argv)] });
        let read = s.receive(ServerMsg::Sessions {
            request_id: list,
            repository: RepositoryId(10),
            sessions: vec![listed_session("ended", None), listed_session("live", Some(1))],
        });
        assert!(
            matches!(read.as_slice(), [ClientMsg::ReadSession { session_id, .. }] if session_id == "live"),
            "the session under the cursor is read once listed: {read:?}"
        );
        s
    }

    fn sessions_page(s: &mut Session) -> Option<page::SessionsPage> {
        s.events(0).into_iter().find_map(|e| match e {
            Event::Sessions(page) => Some(*page),
            _ => None,
        })
    }

    #[test]
    fn the_sessions_dialog_lists_live_sessions_first_and_reads_the_one_chosen() {
        let mut s = listing_sessions();
        let page = sessions_page(&mut s).expect("the dialog is sent");
        assert!(page.open);
        assert_eq!(page.rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["live work", "ended work"]);
        let sent = s.call(Call::SessionsAction { action: Action::Next, row: None });
        assert!(matches!(sent.as_slice(), [ClientMsg::ReadSession { session_id, .. }] if session_id == "ended"), "{sent:?}");
        s.call(Call::SessionsAction { action: Action::CloseLayer, row: None });
        assert!(sessions_page(&mut s).is_some_and(|p| !p.open), "its closing is sent once");
    }

    #[test]
    fn a_live_session_chosen_goes_to_its_pane() {
        let mut s = listing_sessions();
        s.call(Call::SessionsAction { action: Action::Choose, row: None });
        assert_eq!(s.shown.pane(), Some(PaneId(1)));
        assert!(s.recent.is_none(), "the dialog gave way to the pane");
    }

    #[test]
    fn a_resumed_session_is_shown_only_once_made_and_in_the_tree() {
        let mut s = listing_sessions();
        s.call(Call::SessionsAction { action: Action::Next, row: None });
        let sent = s.call(Call::SessionsAction { action: Action::Resume, row: None });
        let [ClientMsg::ResumeSession { request_id, checkout: CheckoutId(100), harness, session_id }] = sent.as_slice() else {
            panic!("{sent:?}");
        };
        assert_eq!((harness.as_str(), session_id.as_str()), ("claude", "ended"));
        assert!(s.recent.is_some(), "nothing is predicted");
        s.receive(ServerMsg::Created { request_id: *request_id, created: Some(Created::Pane(PaneId(9))) });
        assert!(s.recent.is_some() && s.shown.pane().is_none(), "made, but not in a tree yet");
        let mut grown = tree();
        grown[0].repositories[0].checkouts[0].panes.push(pane(9, PaneKind::Agent, "claude", PaneStatus::Working));
        s.receive(with_tree(grown));
        assert_eq!(s.shown.pane(), Some(PaneId(9)));
        assert!(s.recent.is_none());
    }

    #[test]
    fn a_refused_resume_leaves_the_dialog_up() {
        let mut s = listing_sessions();
        s.call(Call::SessionsAction { action: Action::Next, row: None });
        let sent = s.call(Call::SessionsAction { action: Action::Resume, row: None });
        let [ClientMsg::ResumeSession { request_id, .. }] = sent.as_slice() else {
            panic!("{sent:?}");
        };
        s.receive(ServerMsg::Created { request_id: *request_id, created: None });
        assert!(s.recent.is_some() && s.shown.pane().is_none());
    }

    #[test]
    fn a_daemon_that_cannot_list_sessions_is_not_asked_and_the_dialog_says_so() {
        let mut s = session(without_focus());
        assert!(s.call(Call::OpenSessions { repository: 10 }).is_empty());
        let page = sessions_page(&mut s).expect("the dialog opens");
        assert!(page.empty.is_some_and(|e| e.contains("cannot list sessions")));
        assert!(s.call(Call::OpenSessions { repository: 99 }).is_empty(), "no such repository");
    }

    #[test]
    fn a_reconnect_asks_the_new_daemon_for_the_open_dialogs_sessions() {
        let mut s = listing_sessions();
        let sent = s.connected(Some(Hello::this_build()), None, vec![with_tree(tree())]);
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::ListSessions { repository: RepositoryId(10), .. })), "{sent:?}");
        assert!(sessions_page(&mut s).is_some_and(|p| p.loading), "what the last daemon said is gone");
    }
}
