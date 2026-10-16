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
//! the status bar (`status`), the panes on screen (`shown`), the agents
//! offered (`agents`), the review and history (`dialogs`), the link and
//! reconcile dialogs (`tracker_dialogs`), the two stages
//! (`feature_stage`, `checkouts_stage`) and the note a new feature is
//! written as (`capture_note`), a repository's past agent
//! sessions (`sessions`) and the dialog listing them (`sessions_dialog`),
//! the project switcher (`switcher`) and the dialog adding a project
//! (`add_project`), the Settings dialog's choices (`preferences`), the
//! console (`console`), the views agents showed (`agent_views`), and what
//! the daemon is making
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
    RepositoryId, ServerMsg, View, WorkspaceId, ADD_PROJECT, AGENT_VIEWS, VIEW_ACTIONS, CONSOLES, FOCUS, PASTE_FILE,
    RENAME_PANE, SAVED_VIEWS, CODE, CodeVersion,
};

use crate::add_project::{AddProject, AddProjectGesture, Adding};
use crate::agent_views::AgentViews;
use crate::extension_dialog::ExtensionDialogs;
use crate::extension_keys::ExtensionKeys;
use crate::extension_views::{first_place, ExtensionViews, Here};
use crate::agents::Agents;
use crate::capture_note::{CaptureGesture, Capturing};
use crate::checkouts_stage::{CheckoutsStage, Opened};
use crate::console::{Console, ConsoleGesture, ConsolePlace, OnScreen};
use crate::docks::{Dock, DockGesture, Docks, Panel, Place};
use crate::dialogs::Dialogs;
use crate::feature_stage::{FeatureStage, Scope};
use crate::features::Layout;
use crate::keymap::{self, Action};
use crate::notifications::{self, Alert, Looking};
use crate::keys::{self, PageKey};
use crate::link_dialog::LinkGesture;
use crate::code_stage::{Back, CodeStage};
use crate::shown::workspace_grid::{self, Dir, Zone};
use crate::stage::{Settled, Stages};
use crate::page::{
    self, Call, CodeClick, Event, FeatureEdit, PageMouse, Reference, ReviewClick, ReviewJumpGesture, SettingChange, Side,
    Stage,
};
use crate::palette;
use crate::preferences::Chosen;
use crate::reconcile_dialog::ReconcileGesture;
use crate::rail::{self, Choices};
use crate::saved_views::{Closed, SavedColumn, SavedViews, Whereabouts};
use crate::dragged_width::{self, DraggedWidth};
use crate::sent::Sent;
use crate::sessions::Sessions;
use crate::sessions_dialog::{Recent, SessionsDialog, Then as SessionsThen};
use crate::settings::{Backdrop, SavedLinks, Settings, Theme};
use crate::shown::ShownPane;
use crate::status::StatusBar;
use crate::switcher::Switcher;
use crate::view_refs::{self, Found, PendingFeature, Target};
use crate::tracker_dialogs::{self, TrackerDialogs};

/// What to do with a thing the daemon made for this window, once the tree
/// holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// dialog away with whichever stage is up, and say in its header what
    /// it was resumed from.
    Resumed(page::Resumed),
    /// Put the added project on the rail, closing the dialog that added it.
    SelectProject,
    /// Show the new console in the console, which shows itself for it.
    ShowConsole,
    /// Show the new console on the second side of the split asked for.
    ShowBeside,
}

/// Said where the Code stage is asked of a daemon that cannot read code.
const TOO_OLD_FOR_CODE: &str = "argusd is too old to read code; restart it to update";

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
    /// Checkouts the person folded; open is the default.
    folded: HashSet<CheckoutId>,
    stages: Stages,
    features: FeatureStage,
    /// The note a new feature is being written as.
    capture: Capturing,
    checkouts: CheckoutsStage,
    shown: ShownPane,
    console: Console,
    docks: Docks,
    awaited: Awaited<Made>,
    agents: Agents,
    views: AgentViews,
    /// The views agents saved, and the columns of each panel showing them.
    saved: SavedViews,
    /// What the extensions publish: their views, the one out in the side
    /// panel in place of an agent's, their commands and their dialogs.
    extension_views: ExtensionViews,
    /// A feature a reference in an agent's view named, to reveal once the
    /// board of its repository has arrived.
    opening_feature: PendingFeature,
    extension_keys: ExtensionKeys,
    extensions_sent: Sent<page::ExtensionsPage>,
    extension_dialogs: ExtensionDialogs,
    rail_width: DraggedWidth,
    outline_width: DraggedWidth,
    brief_height: DraggedWidth,
    dialogs: Dialogs,
    sessions: Sessions,
    /// The recent-sessions dialog.
    recent: Recent,
    /// Text a gesture asked to put on the clipboard, which the window
    /// writes once the change is made.
    copied: Option<String>,
    /// A ticket a gesture asked to open in the browser, which the window
    /// opens once the change is made.
    opening: Option<String>,
    tracker_dialogs: TrackerDialogs,
    switcher: Switcher,
    /// The add-a-project dialog.
    adding: Adding,
    /// The command palette is open, so what it can go to is sent.
    palette: bool,
    /// The host was switched since the settings were saved.
    host_unsaved: bool,
    palette_sent: Sent<page::Palette>,
    /// The Code stage, once opened: it keeps its checkout and file while
    /// another stage is up.
    code: Option<CodeStage>,
    code_sent: Sent<page::CodePage>,
    focused: bool,
    choices: Chosen,
    /// Notifications for the system, raised by the last tree.
    alerts: Vec<Alert>,
    /// The view last reported, so an unchanged one is not sent again.
    reported: Option<View>,
    /// The rail draws from nearly everything here, so it is built each
    /// time the page is told and sent only when it differs.
    rail_sent: Sent<page::Rail>,
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
            status: StatusBar::new(settings.chime).on_host(settings.host.clone()),
            host_unsaved: false,
            project: None,
            repository: None,
            toggled: HashSet::new(),
            folded: HashSet::new(),
            stages: Stages::default(),
            features: {
                let mut stage = FeatureStage::new(settings.layouts);
                stage.set_opens_as(settings.preferences.feature_layout);
                stage
            },
            capture: Capturing::default(),
            checkouts: CheckoutsStage::default(),
            shown: ShownPane::default(),
            console: Console::default(),
            docks: Docks::new(settings.docks.unwrap_or_default()),
            awaited: Awaited::default(),
            agents: Agents::new(settings.last_agent),
            views: AgentViews::default(),
            saved: SavedViews::default(),
            extension_views: ExtensionViews::new(settings.extensions),
            opening_feature: PendingFeature::default(),
            extension_keys: ExtensionKeys::default(),
            extensions_sent: Sent::default(),
            extension_dialogs: ExtensionDialogs::default(),
            rail_width: DraggedWidth::new(dragged_width::RAIL, settings.rail_width),
            outline_width: DraggedWidth::new(dragged_width::OUTLINE, settings.outline_width),
            brief_height: DraggedWidth::new(dragged_width::BRIEF, settings.brief_height),
            dialogs: Dialogs::new(settings.review),
            sessions: Sessions::default(),
            recent: Recent::default(),
            copied: None,
            opening: None,
            tracker_dialogs: TrackerDialogs::default(),
            switcher: Switcher::with_visited(settings.hosts.clone()),
            adding: Adding::default(),
            palette: false,
            palette_sent: Sent::default(),
            code: None,
            code_sent: Sent::default(),
            focused: true,
            choices: Chosen::new(settings.preferences),
            alerts: Vec::new(),
            reported: None,
            rail_sent: Sent::default(),
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

    /// The window is to reach another daemon: `host`, or this machine's.
    /// What the last one's tree named is not the next one's, and a stale
    /// rail would let a gesture reach for a pane that is not there, so the
    /// window goes back to knowing nothing until the new daemon speaks.
    pub fn switch_host(&mut self, host: Option<String>) -> Vec<ClientMsg> {
        if self.status.host() == host.as_deref() {
            return self.finish();
        }
        if let Some(host) = &host {
            self.switcher.visit(host);
        }
        self.status.switch_host(host);
        self.host_unsaved = true;
        self.take_tree(Vec::new());
        self.take_consoles(Vec::new());
        self.loaded = false;
        // Nothing about the last daemon's panes is worth saying now.
        self.alerts.clear();
        self.passing.retain(|event| !matches!(event, Event::Chime));
        self.finish()
    }

    /// The hosts the ssh config names, read after the window opened.
    pub fn found_hosts(&mut self, hosts: Vec<String>) {
        self.switcher.found_ssh_config(hosts);
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
        // What another daemon published is not this one's; it sends its
        // own after its greeting, and the dialogs it put here it cancelled
        // when the connection went.
        self.extension_views.clear();
        self.extension_keys.clear();
        self.extension_dialogs.clear();
        // A new connection knows nothing this window asked of the last.
        self.reported = None;
        self.awaited.forget_asked();
        self.features.forget_asked();
        self.sessions = Sessions::default();
        for msg in opening {
            self.apply(msg);
        }
        // An open sessions dialog asks this daemon again, or says it cannot.
        if let Some(repository) = self.recent.get().map(SessionsDialog::repository) {
            let asked = self.ask_sessions(repository);
            if let Some(dialog) = self.recent.get_mut() {
                dialog.reasked(asked);
            }
        }
        if let Some(dialog) = self.adding.get_mut() {
            dialog.reconnected(self.awaited.name(), &mut self.out);
        }
        self.shown.reconnected(&mut self.out);
        if self.status.daemon_can(AGENT_VIEWS) {
            self.views.connected(&mut self.out);
        }
        if self.status.daemon_can(SAVED_VIEWS) {
            self.saved.connected(&mut self.out);
        } else {
            self.saved.clear();
        }
        self.finish()
    }

    /// A message from the daemon.
    pub fn receive(&mut self, msg: ServerMsg) -> Vec<ClientMsg> {
        self.apply(msg);
        self.finish()
    }

    fn apply(&mut self, msg: ServerMsg) {
        // The review's comment path is the Code stage's too: whichever box
        // was open is the one the daemon saved.
        if let (ServerMsg::ReviewCommentSaved { .. }, Some(code)) = (&msg, &mut self.code) {
            code.commented();
        }
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
            ServerMsg::Consoles(panes) => self.take_consoles(panes),
            ServerMsg::PaneTelemetry { pane, telemetry } => {
                // The rail says nothing of telemetry; the tree stays true.
                self.tree.as_mut_slice().apply_telemetry(pane, telemetry);
            }
            ServerMsg::Created { request_id, created } => {
                if let (Some(dialog), None) = (self.adding.get_mut(), created) {
                    dialog.refused(request_id);
                }
                self.awaited.answer(request_id, created);
                self.settle_made();
            }
            ServerMsg::Workspaces(workspaces) => self.switcher.receive_workspaces(workspaces),
            ServerMsg::Directories(listing) => {
                if let Some(dialog) = self.adding.get_mut() {
                    dialog.listed(listing, || self.awaited.name(), &mut self.out);
                }
            }
            ServerMsg::ProjectPreview(preview) => {
                if let Some(dialog) = self.adding.get_mut() {
                    dialog.previewed(preview);
                }
            }
            // A refused add's reason is said in its dialog, which is over
            // the notices.
            ServerMsg::Error { message } if self.adding.get_mut().is_some_and(|d| d.take_reason(&message)) => {}
            ServerMsg::BranchNotMerged { checkout, branch, commits } => {
                self.checkouts.refused(checkout, branch, &commits, argus_protocol::ago::now());
            }
            ServerMsg::Clipboard { pane, text } if Some(pane) == self.shown.pane() => {
                self.passing.push(Event::Clipboard { text });
            }
            // The note stays open to be fixed, and Create can be pressed
            // again; the reason is said like any other.
            ServerMsg::Error { message } => {
                if let Some(note) = self.capture.get_mut() {
                    note.refused();
                }
                self.tracker_dialogs.refused();
                self.passing.push(Event::Notice { text: message });
            }
            ServerMsg::Hello(hello) => self.status.greeted(hello),
            ServerMsg::Restarting => self.passing.push(Event::Notice {
                text: "argusd is restarting".into(),
            }),
            ServerMsg::Decisions(board) => {
                let scope = self.scope();
                self.features.receive_board(*board, scope.as_ref(), &self.tree, &mut self.out);
                self.capture_landed();
                if let Some(board) = self.features.board() {
                    self.tracker_dialogs.settle(board);
                }
            }
            ServerMsg::Templates(names) => self.agents.receive(names),
            ServerMsg::Tasks(list) => self.features.receive_tasks(*list),
            ServerMsg::ViewShown(view) => {
                // Whatever the views' dock shows is not taken from under
                // the person by an agent's view arriving.
                if self.views.shown(*view, self.shown.pane(), self.docks.dock_out(&Panel::Views)) {
                    self.docks.show(&Panel::Views);
                }
            }
            ServerMsg::SavedViews(views) => self.saved.receive_all(views),
            ServerMsg::SavedViewChanged(view) => self.saved.changed(*view),
            ServerMsg::SavedViewRemoved { id } => {
                self.saved.removed(&id);
                self.docks.forget(&Panel::Saved(id));
            }
            // What was asked of the view did not happen, so the person is
            // told why and nothing waits on it.
            ServerMsg::SavedViewFailed { id, reason } => {
                let text = self.saved.failed(&id, &reason);
                self.passing.push(Event::Notice { text });
            }
            ServerMsg::ExtensionViews(views) => self.extension_views.receive(views),
            ServerMsg::ExtensionCommands(commands) => self.extension_keys.receive(commands),
            ServerMsg::ExtensionNotice { extension, text } => {
                self.passing.push(Event::Notice { text: format!("{extension}: {text}") });
            }
            ServerMsg::ExtensionDialog { extension, id, dialog, prompted } => {
                self.extension_dialogs.asked(extension, id, dialog, prompted);
            }
            ServerMsg::ExtensionDialogWithdrawn { id } => self.extension_dialogs.withdrawn(id),
            ServerMsg::ViewClosed { pane, id } => {
                if let Some(title) = self.views.closed(pane, &id) {
                    self.passing.push(Event::Notice { text: format!("{title} was closed") });
                }
            }
            // The press never reached the agent, so the button comes back
            // and the person is told why.
            ServerMsg::ViewPressFailed { pane, view, action, reason } => {
                let text = self.views.press_failed(pane, &view, &action, &reason);
                self.passing.push(Event::Notice { text });
            }
            ServerMsg::SequenceDiagrams(list) => self.features.receive_diagrams(*list),
            ServerMsg::CodeTree(tree) => {
                if let Some(code) = &mut self.code {
                    code.receive_tree(tree);
                }
            }
            ServerMsg::Code(file) => {
                if let Some(code) = &mut self.code {
                    code.receive_file(*file);
                }
            }
            ServerMsg::CodeMatches(found) => {
                if let Some(code) = &mut self.code {
                    code.receive_matches(found);
                }
            }
            ServerMsg::CodeFailed { request_id, checkout, message } => {
                if let Some(code) = &mut self.code {
                    code.failed(checkout, request_id, message);
                }
            }
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
        let changes = transitions(self.tree.as_slice(), tree.as_slice());
        let rings = changes.iter().any(|t| t.after.status.needs_you() && Some(t.pane.id) != watched);
        if rings && self.status.chimes() {
            self.passing.push(Event::Chime);
        }
        let looking = Looking { focused: self.focused, watched };
        let raised = self.alerts.len();
        self.alerts.extend(notifications::alerts(&changes, &tree, self.choices.get(), &looking));
        if let Some(host) = self.status.host() {
            notifications::on_host(&mut self.alerts[raised..], host);
        }
        self.tree = tree;
        self.loaded = true;
        self.features.tree_changed();
        if !self.tree.iter().any(|p| Some(p.id) == self.project) {
            self.project = self.tree.first().map(|p| p.id);
        }
        // A pane that is gone is gone from the daemon's subscriptions too.
        // A console is not in the tree, and is let go by `take_consoles`.
        for pane in self.ended() {
            self.shown.gone(pane, &mut self.out);
        }
        let tree = &self.tree;
        self.sessions.forget_resumed(|pane| tree.panes().any(|p| p.id == pane));
        self.settle_made();
    }

    /// The daemon's consoles, after its greeting and after every tree. A
    /// console on screen that ended hands its side to what the console
    /// chose in its place — the other side of a split, or another tab —
    /// and keys it held go to the console's other side, or else the
    /// Workspace's pane.
    fn take_consoles(&mut self, panes: Vec<argus_protocol::PaneInfo>) {
        self.console.receive(panes);
        let ended = self.ended();
        if !ended.is_empty() {
            let typing = self.console_typing();
            for pane in ended {
                self.shown.gone(pane, &mut self.out);
            }
            if self.console.chosen().is_some() {
                self.console_on_screen(self.shown.pane().filter(|_| !typing));
            }
        }
        self.settle_made();
    }

    /// The panes on screen that neither the tree nor the consoles hold.
    fn ended(&self) -> Vec<PaneId> {
        let held = |pane: PaneId| self.project_of(pane).is_some() || self.console.holds(pane);
        self.shown.panes().into_iter().filter(|&pane| !held(pane)).collect()
    }

    /// Acts on whatever the daemon made that the tree, or the consoles
    /// beside it, now hold.
    fn settle_made(&mut self) {
        let (tree, console) = (&self.tree, &self.console);
        let seen = |created: Created| {
            argus_protocol::awaited::holds(tree, created) || matches!(created, Created::Pane(pane) if console.holds(pane))
        };
        for (created, then) in self.awaited.settle_by(seen) {
            let over_stages = then == Made::ShowOverStages;
            match (created, then) {
                (Created::Pane(pane), Made::Show | Made::ShowOverStages) => self.show_made(pane, over_stages),
                (Created::Pane(pane), Made::Resumed(from)) => {
                    self.recent.close();
                    self.sessions.resumed_in(pane, from);
                    self.show_made(pane, true);
                }
                // A worktree made from the Checkouts stage is the row it
                // lands on.
                (Created::Checkout(checkout), Made::SelectRow) => self.checkouts.select(format!("c{}", checkout.0)),
                (Created::Project(project), Made::SelectProject) => {
                    self.adding.close();
                    self.choose_project(project);
                }
                (Created::Pane(pane), Made::ShowConsole) => self.show_console(pane),
                (Created::Pane(pane), Made::ShowBeside) => self.show_beside(pane),
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
        if let Some(template) = template {
            self.agents.started(&template);
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
            Call::Resize { side, rows, cols } => self.resize(side, rows, cols),
            Call::FocusView { pane } => self.focus_view(PaneId(pane)),
            Call::OpenBeside { pane } => self.open_beside(PaneId(pane)),
            Call::CloseBeside { side } => self.close_beside(side),
            Call::SetShares { heights, widths } => self.set_shares(&heights, &widths),
            Call::EvenShares { row } => self.even_shares(row.map(usize::from)),
            Call::StepRow { by } => self.step_row(isize::from(by)),
            Call::MovePane { pane, onto, zone } => self.move_pane(PaneId(pane), PaneId(onto), zone),
            Call::DropPane { pane, onto, zone } => self.drop_pane(PaneId(pane), onto.map(PaneId), zone),
            Call::MoveHeld { dir } => self.move_held(dir),
            Call::FocusTowards { dir } => self.focus_towards(dir),
            Call::Key { key } => self.key(&key),
            Call::Text { text } => self.text(&text),
            Call::Paste { text } => self.paste(text),
            Call::Wheel { pane, notches, at } => self.wheel(PaneId(pane), notches, at),
            Call::Mouse { gesture, button, at } => self.mouse(gesture, button, at),
            Call::SpawnShell { checkout } => self.spawn_shell(CheckoutId(checkout)),
            Call::SpawnAgent { checkout, template } => self.spawn_agent(CheckoutId(checkout), template),
            Call::Close { pane } => self.close(PaneId(pane)),
            Call::RenamePane { pane, title } => self.rename(PaneId(pane), title),
            Call::Focus { focused } => self.focus(focused),
            Call::SelectProject { project } => self.select_project(ProjectId(project)),
            Call::OpenWorkspace { workspace } => self.open_workspace(WorkspaceId(workspace)),
            Call::RemoveProject { project } => self.remove_project(ProjectId(project)),
            Call::AnswerRemoveProject { yes } => self.answer_remove_project(yes),
            Call::OpenAddProject => self.open_add_project(),
            Call::AddProject { gesture } => self.add_project(gesture),
            Call::ToggleRepository { repository } => self.toggle_repository(RepositoryId(repository)),
            Call::ToggleCheckout { checkout } => self.toggle_checkout(CheckoutId(checkout)),
            Call::ChooseRepository { repository } => self.choose_repository(RepositoryId(repository)),
            Call::ShowFeatures { open } => self.show_features(open),
            Call::ShowCheckouts { open } => self.show_checkouts(open),
            Call::ShowCode { open } => self.show_code(open),
            Call::CodeClick { target } => self.code_click(target),
            Call::CodeAction { action } => self.code_action(action),
            Call::CodeJump { gesture } => self.code_field(gesture, false),
            Call::CodeSearch { gesture } => self.code_field(gesture, true),
            Call::CodeComment { body } => self.code_comment(body),
            Call::CodeSearchGlob { glob } => {
                if let Some(code) = &mut self.code {
                    code.set_glob(glob);
                }
                self.finish()
            }
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
            Call::FindInTranscript { find } => self.find_in_transcript(find),
            Call::FilterSessions { harness, checkout } => self.filter_sessions(harness, checkout),
            Call::Peek { slug } => self.peek(&slug),
            Call::SetLayout { layout } => self.set_layout(layout),
            Call::FilterFeatures { filter } => self.filter_features(filter),
            Call::FeatureAction { action } => self.feature_action(action),
            Call::SetChime { on } => self.set_chime(on),
            Call::Setting { change } => self.set(change),
            Call::Hold { slug, reason } => self.hold(&slug, reason),
            Call::Archive { slug, archived } => self.archive(&slug, archived),
            Call::Capture { gesture } => self.capture_gesture(gesture),
            Call::OpenFeature { slug } => self.open_feature(slug.as_deref()),
            Call::SelectWork { at } => {
                self.features.select(at);
                self.finish()
            }
            Call::SetLens { lens } => {
                self.features.set_lens(lens);
                self.finish()
            }
            Call::SelectDecision { id } => {
                self.features.select_decision(id);
                self.finish()
            }
            Call::SelectStep { diagram, step } => {
                self.features.select_step(diagram, step);
                self.finish()
            }
            Call::Wait { slug, on, waits } => {
                let scope = self.scope();
                self.features.wait(&slug, &on, waits, scope.as_ref(), &mut self.out);
                self.finish()
            }
            Call::EditFeature { edit } => self.edit_feature(edit),
            Call::GoToPane { pane } => self.go_to_pane(PaneId(pane)),
            Call::Palette { open } => self.palette(open),
            Call::Command { action } => self.command(action),
            Call::GoToCheckout { checkout } => self.go_to_checkout(CheckoutId(checkout)),
            Call::GoToFeature { slug } => self.go_to_feature(&slug),
            Call::Console { gesture } => self.console_gesture(gesture),
            Call::Docks { gesture } => self.dock_gesture(gesture),
            Call::PasteClipboard
            | Call::Copy { .. }
            | Call::CopyText { .. }
            | Call::Reconnect
            | Call::SetHost { .. }
            | Call::Minimize
            | Call::ToggleMaximize
            | Call::CloseWindow
            | Call::DragWindow
            | Call::SnapLayouts
            | Call::OpenUrl { .. } => self.finish(),
            Call::OpenView { pane, id } => {
                if self.views.open(PaneId(pane), id) {
                    self.docks.show(&Panel::Views);
                }
                self.finish()
            }
            Call::PopOutView { pane, id } => {
                if self.views.pop_out(PaneId(pane), id) {
                    self.docks.hide(&Panel::Views);
                    self.show_view_page();
                }
                self.finish()
            }
            Call::OpenRef { pane, reference } => self.open_ref(pane.map(PaneId), reference),
            Call::OpenSavedView { id, from } => self.open_saved(id, from),
            Call::CloseSavedColumn { panel, column } => {
                if let Panel::Saved(owner) = panel {
                    if self.saved.close_column(&owner, column) == Closed::Panel {
                        self.docks.forget(&Panel::Saved(owner));
                    }
                }
                self.finish()
            }
            Call::FloatSavedColumn { panel, column } => {
                if let Panel::Saved(owner) = panel {
                    match column {
                        0 => self.docks.float(&Panel::Saved(owner)),
                        _ => {
                            if let Some(id) = self.saved.pop_out(&owner, column) {
                                self.float_saved(id);
                            }
                        }
                    }
                }
                self.finish()
            }
            Call::PopOutSavedColumn { panel, column } => {
                if let Panel::Saved(owner) = panel {
                    if let Some(id) = self.saved.pop_out(&owner, column) {
                        self.show_saved(id);
                    }
                }
                self.finish()
            }
            Call::RefreshSavedView { id } => {
                self.saved.refresh(id, &mut self.out);
                self.finish()
            }
            Call::MarkSavedViewStale { id } => {
                self.saved.mark_stale(id, &mut self.out);
                self.finish()
            }
            Call::DeleteSavedView { id } => {
                self.saved.delete(id, &mut self.out);
                self.finish()
            }
            Call::PressSavedView { id, press } => {
                self.saved.press(id, press, &mut self.out);
                self.finish()
            }
            Call::HideViews => {
                self.docks.hide(&Panel::Views);
                self.finish()
            }
            Call::ToggleViews => {
                self.toggle_views();
                self.finish()
            }
            Call::RailWidth { width } => {
                self.rail_width.set(width);
                self.finish()
            }
            Call::OutlineWidth { width } => {
                self.outline_width.set(width);
                self.finish()
            }
            Call::BriefHeight { height } => {
                self.brief_height.set(height);
                self.finish()
            }
            Call::RefreshView { pane, id } => {
                self.views.refresh(PaneId(pane), id, &mut self.out);
                self.finish()
            }
            Call::PressView { pane, press } => {
                if self.status.daemon_can(VIEW_ACTIONS) {
                    self.views.press(PaneId(pane), press, &mut self.out);
                }
                self.finish()
            }
            Call::OpenExtensionView { extension, view } => self.open_extension_view(&extension, &view),
            Call::SelectExtensionRow { panel, key } => {
                self.extension_views.touch(&panel);
                self.extension_views.select(&panel, key);
                self.finish()
            }
            Call::ExtensionAction { panel, action } => self.extension_action(&panel, &action),
            Call::ExtensionDefault { panel, row } => {
                self.extension_views.select(&panel, row);
                self.extension_default(&panel)
            }
            Call::ExtensionKey { panel, action } => self.extension_key(&panel, action),
            Call::AnswerExtension { yes } => {
                if let Some(msg) = self.extension_views.answer(yes) {
                    self.out.push(msg);
                }
                self.finish()
            }
            Call::ExtensionCommand { extension, command, checkout, pane } => {
                self.extension_command(&extension, &command, checkout.map(CheckoutId), pane.map(PaneId));
                self.finish()
            }
            Call::ExtensionMode { panel, page } => {
                self.extension_mode(&panel, page);
                self.finish()
            }
            Call::ToggleExtensions => {
                self.extension_views.toggle_listed();
                self.finish()
            }
            Call::AnswerDialog { dialog, answer } => {
                if let Some(msg) = self.extension_dialogs.answer(dialog, answer) {
                    self.out.push(msg);
                }
                self.finish()
            }
            Call::CloseView { pane, id } => {
                self.views.close(PaneId(pane), id, &mut self.out);
                self.finish()
            }
            Call::OpenLink { map } => {
                self.open_link(map);
                self.finish()
            }
            Call::Link { gesture } => self.link_gesture(gesture),
            Call::OpenReconcile => {
                self.open_reconcile();
                self.finish()
            }
            Call::Reconcile { gesture } => self.reconcile_gesture(gesture),
        }
    }

    // ---- The tracker dialogs --------------------------------------------------

    fn open_link(&mut self, map: bool) {
        if let Err(text) = self.tracker_dialogs.open_link(self.features.opened(), self.features.board(), map) {
            self.passing.push(Event::Notice { text: text.into() });
        }
    }

    fn open_reconcile(&mut self) {
        if let Err(text) = self.tracker_dialogs.open_reconcile(self.features.opened(), self.features.board()) {
            self.passing.push(Event::Notice { text: text.into() });
        }
    }

    fn link_gesture(&mut self, gesture: LinkGesture) -> Vec<ClientMsg> {
        if let Some(scope) = self.scope() {
            let then = self.tracker_dialogs.link_gesture(gesture, self.features.board(), &scope, &mut self.out);
            self.tracker_then(then);
        }
        self.finish()
    }

    fn reconcile_gesture(&mut self, gesture: ReconcileGesture) -> Vec<ClientMsg> {
        if let Some(scope) = self.scope() {
            let board = self.features.board();
            let then = self.tracker_dialogs.reconcile_gesture(gesture, board, &self.tree, &scope, &mut self.out);
            self.tracker_then(then);
        }
        self.finish()
    }

    fn tracker_then(&mut self, then: tracker_dialogs::Then) {
        match then {
            tracker_dialogs::Then::Nothing => {}
            tracker_dialogs::Then::Open(url) => self.opening = Some(url),
            tracker_dialogs::Then::GoTo(pane) => {
                let sent = self.go_to_pane(pane);
                self.out.extend(sent);
            }
            tracker_dialogs::Then::Say(text) => self.passing.push(Event::Notice { text }),
        }
    }

    /// A ticket a gesture asked to open, once: the browser is the window's
    /// to start, not the session's.
    pub fn take_opening(&mut self) -> Option<String> {
        self.opening.take()
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

    /// Puts a checkout's pane on the Workspace with the keys, beside a
    /// docked console that stays. The Console stage and fullscreen cover
    /// the Workspace, so it leaves them.
    fn show(&mut self, pane: PaneId) {
        let Some(project) = self.project_of(pane) else {
            return;
        };
        // A pane already side by side with the others is only given the
        // keys: showing it on the Workspace would take it out of the row.
        let changed = !self.shown.grid().holds(pane) && self.shown.show_on(Side::Workspace, pane, &mut self.out);
        let keyed = self.shown.keys() != Side::Workspace;
        self.shown.focus(pane);
        if changed {
            self.project = Some(project);
            self.repository = None;
        }
        if changed || keyed {
            self.console.set_fullscreen(false);
            if self.stages.now() == Stage::Console {
                self.go(Stage::Workspace);
            }
        }
    }

    /// Lets the Workspace's pane go, as choosing another project does. The
    /// console's own panes are no project's, and stay.
    fn hide(&mut self) {
        self.shown.close(Side::Workspace, &mut self.out);
    }

    fn resize(&mut self, side: Side, rows: u16, cols: u16) -> Vec<ClientMsg> {
        self.shown.resize(side, rows, cols, &mut self.out);
        // A view's screen is made by the page when it hears of the view, and
        // reports its room once it exists: it has none of the rows yet, and
        // may have been sent a frame before it was there to draw it.
        if matches!(side, Side::View(_)) {
            self.shown.refresh_side(side);
        }
        self.finish()
    }

    /// Splits `pane` off beside the Workspace's, and gives it the keys. The
    /// Workspace needs a pane of its own to split from, and the row a pane's
    /// worth of room: a refusal says which, and the row stays as it was.
    fn open_beside(&mut self, pane: PaneId) -> Vec<ClientMsg> {
        match self.join_grid(pane) {
            Err(Some(refusal)) => self.note(refusal.to_string()),
            _ => self.finish(),
        }
    }

    /// Puts `pane` in the grid and gives it the keys, or says why not: a
    /// refusal is the note to show, and none is a pane that is not there.
    /// Nothing moves on a refusal.
    fn join_grid(&mut self, pane: PaneId) -> Result<(), Option<workspace_grid::Refusal>> {
        if self.project_of(pane).is_none() {
            return Err(None);
        }
        self.shown.open_beside(pane, &mut self.out).map_err(Some)?;
        self.leave_stages();
        self.console.set_fullscreen(false);
        self.shown.focus(pane);
        Ok(())
    }

    /// A pane dragged from the rail and let go over the Workspace. With no
    /// pane there it is simply shown; otherwise it joins the grid by the
    /// grid rule and is then placed where it was dropped, so a drop on a
    /// pane's edge splits that pane and one on its centre swaps them.
    fn drop_pane(&mut self, pane: PaneId, onto: Option<PaneId>, zone: Zone) -> Vec<ClientMsg> {
        if self.shown.on(Side::Workspace).is_none() {
            return self.select(pane);
        }
        if let Err(refusal) = self.join_grid(pane) {
            return match refusal {
                Some(refusal) => self.note(refusal.to_string()),
                None => self.finish(),
            };
        }
        if let Some(onto) = onto.filter(|&onto| onto != pane) {
            self.shown.grid_mut().place(pane, onto, zone);
        }
        self.finish()
    }

    /// Takes a split out of the grid. Its pane keeps running, and is on the
    /// rail to be shown again.
    fn close_beside(&mut self, side: Side) -> Vec<ClientMsg> {
        self.shown.close_split(side, &mut self.out);
        self.finish()
    }

    fn set_shares(&mut self, heights: &[u16], widths: &[Vec<u16>]) -> Vec<ClientMsg> {
        self.shown.grid_mut().share(heights, widths);
        self.finish()
    }

    fn even_shares(&mut self, row: Option<usize>) -> Vec<ClientMsg> {
        self.shown.grid_mut().even_out(row);
        self.finish()
    }

    /// A pane dropped on another's centre or edge, from its header.
    fn move_pane(&mut self, pane: PaneId, onto: PaneId, zone: Zone) -> Vec<ClientMsg> {
        self.shown.grid_mut().place(pane, onto, zone);
        self.finish()
    }

    /// The pane with the keys trades places with its neighbour: the
    /// keyboard's way of what a drop does.
    fn move_held(&mut self, dir: Dir) -> Vec<ClientMsg> {
        self.shown.step_pane(dir);
        self.finish()
    }

    fn focus_towards(&mut self, dir: Dir) -> Vec<ClientMsg> {
        self.focus_in(dir);
        self.finish()
    }

    /// The keys go to the pane next to theirs in `dir`, or stay if none is.
    fn focus_in(&mut self, dir: Dir) {
        self.shown.focus_towards(dir);
    }

    fn move_in(&mut self, dir: Dir) {
        self.shown.step_pane(dir);
    }

    fn step_row(&mut self, by: isize) -> Vec<ClientMsg> {
        self.shown.step_keys(by);
        self.finish()
    }

    /// A press on an extension console's screen gives it the keys.
    fn focus_view(&mut self, pane: PaneId) -> Vec<ClientMsg> {
        self.shown.focus(pane);
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
            // An extension's command heard anywhere, or on a pane, is
            // reached through the leader as the window's own keys are.
            None if after_leader => {
                if let Some(command) = self.extension_keys.after_leader(key) {
                    let (extension, name) = (command.extension.clone(), command.name.clone());
                    self.extension_command(&extension, &name, None, None);
                }
            }
            None if keymap::is_leader(key) => self.shown.await_leader(),
            None => {
                let mode = self.shown.keyboard();
                let bytes = keys::to_key(key).map(|k| argus_protocol::keys::encode_with(&k, mode)).unwrap_or_default();
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
            Action::Leader => {
                let ctrl_space = Key::new(KeyCode::Char(' '), Mods::CTRL);
                self.input(argus_protocol::keys::encode_with(&ctrl_space, self.shown.keyboard()))
            }
            // A console has no sidebar of its own to put away: filling the
            // window is its fullscreen.
            Action::Fill if self.console_typing() => self.toggle_console_fullscreen(),
            Action::Fill => {
                self.shown.toggle_fill();
            }
            Action::Close => {
                if let Some(pane) = self.shown.pane() {
                    self.kill(pane);
                }
            }
            Action::Workspace => self.back_to_workspace(),
            Action::Feature => self.go(Stage::Feature),
            Action::Checkouts => self.go(Stage::Checkouts),
            Action::Code => self.open_code(),
            Action::Compose => self.go(Stage::Composer),
            Action::Console => self.open_console_tab(),
            Action::ToggleConsole => self.toggle_console(),
            Action::Views => self.toggle_views(),
            Action::ExtensionPage | Action::ExtensionDock => match self.extension_views.last().cloned() {
                Some(panel) => self.extension_mode(&panel, action == Action::ExtensionPage),
                None => self.passing.push(Event::Notice { text: "no extension view has been opened".into() }),
            },
            Action::ConsoleFullscreen => self.toggle_console_fullscreen(),
            Action::NewConsole => self.spawn_console(Made::ShowConsole),
            Action::SplitConsole => self.toggle_split(),
            Action::DockLeft => self.move_panel(&Panel::Console, Dock::Left),
            Action::DockRight => self.move_panel(&Panel::Console, Dock::Right),
            Action::DockBottom => self.move_panel(&Panel::Console, Dock::Bottom),
            Action::OtherSide => self.other_side(),
            Action::PreviousSide => {
                self.shown.step_keys(-1);
            }
            Action::FocusLeft => self.focus_in(Dir::Left),
            Action::FocusRight => self.focus_in(Dir::Right),
            Action::FocusUp => self.focus_in(Dir::Up),
            Action::FocusDown => self.focus_in(Dir::Down),
            Action::MovePaneLeft => self.move_in(Dir::Left),
            Action::MovePaneRight => self.move_in(Dir::Right),
            Action::MovePaneUp => self.move_in(Dir::Up),
            Action::MovePaneDown => self.move_in(Dir::Down),
            Action::CloseSplit => {
                let keys = self.shown.keys();
                if !self.shown.close_split(keys, &mut self.out) {
                    self.passing.push(Event::Notice { text: "the pane with the keys is not split beside another".into() });
                }
            }
            Action::OpenReview => {
                if let Some(checkout) = self.view().checkout {
                    self.review_checkout(checkout);
                }
            }
            Action::PageUp => self.on_view(|view| view.page(-1)),
            Action::PageDown => self.on_view(|view| view.page(1)),
            Action::Palette | Action::Shortcuts | Action::Settings => self.passing.push(Event::Run { action }),
            // `commands` reads the clipboard before the key gets here; the
            // rest are the page's, for keys a pane never hears.
            _ => {}
        }
    }

    /// The wheel over `pane`, `notches` up when negative, at the cell `at`
    /// under the pointer, decided as the terminal client decides it
    /// (`PaneView::wheel`). It scrolls the side it is over without taking
    /// the keys there, as a click would.
    fn wheel(&mut self, pane: PaneId, notches: i32, at: (u16, u16)) -> Vec<ClientMsg> {
        if let Some(effect) = self.shown.gesture_on(pane, |view| view.wheel(notches, at)) {
            self.take(effect);
        }
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
        if self.tree.checkout(checkout).is_some() {
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

    /// Names a pane; like a close, the panel shows the name once a tree
    /// carrying it comes back.
    fn rename(&mut self, pane: PaneId, title: String) -> Vec<ClientMsg> {
        if !self.status.daemon_can(RENAME_PANE) {
            return self.note("argusd is too old to rename panes: run `argus server restart`");
        }
        if self.project_of(pane).is_some() || self.console.holds(pane) {
            self.out.push(ClientMsg::RenamePane { pane, title });
        }
        self.finish()
    }

    fn kill(&mut self, pane: PaneId) {
        if self.project_of(pane).is_some() || self.console.holds(pane) {
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
        self.choose_project(project);
        self.finish()
    }

    fn choose_project(&mut self, project: ProjectId) {
        if self.project != Some(project) && self.tree.iter().any(|p| p.id == project) {
            self.project = Some(project);
            self.hide();
        }
    }

    // ---- The project switcher ----------------------------------------------

    /// Asks the daemon to open a workspace; the rail follows its tree.
    fn open_workspace(&mut self, workspace: WorkspaceId) -> Vec<ClientMsg> {
        self.out.extend(self.switcher.open_workspace(workspace));
        self.finish()
    }

    fn remove_project(&mut self, project: ProjectId) -> Vec<ClientMsg> {
        self.switcher.ask_remove(project, &self.tree);
        self.finish()
    }

    /// A yes asks the daemon; the project leaves the rail when its tree
    /// does, and a refusal is said as a notice.
    fn answer_remove_project(&mut self, yes: bool) -> Vec<ClientMsg> {
        self.out.extend(self.switcher.answer(yes));
        self.finish()
    }

    fn open_add_project(&mut self) -> Vec<ClientMsg> {
        if !self.status.daemon_can(ADD_PROJECT) {
            return self.note("argusd is too old to add a project from here: run `argus server restart`");
        }
        if self.adding.get().is_none() {
            self.adding.open(AddProject::new(self.awaited.name(), &mut self.out));
        }
        self.finish()
    }

    fn add_project(&mut self, gesture: AddProjectGesture) -> Vec<ClientMsg> {
        let Some(dialog) = self.adding.get_mut() else {
            return self.finish();
        };
        match gesture {
            AddProjectGesture::Kind { kind } => dialog.set_kind(kind),
            AddProjectGesture::Browse { path } => dialog.browse(path, self.awaited.name(), &mut self.out),
            AddProjectGesture::Into { name } => dialog.open(&name, self.awaited.name(), &mut self.out),
            AddProjectGesture::Fields { name, skip, folder } => {
                dialog.set_fields(name, skip, folder, || self.awaited.name(), &mut self.out);
            }
            AddProjectGesture::Add { name, skip, folder } => {
                dialog.set_fields(name, skip, folder, || self.awaited.name(), &mut self.out);
                let request_id = self.awaited.ask(Made::SelectProject);
                match dialog.add(request_id) {
                    Ok(msg) => self.out.push(msg),
                    // The dialog says why; nothing was asked.
                    Err(()) => self.awaited.answer(request_id, None),
                }
            }
            AddProjectGesture::Close => self.adding.close(),
        }
        self.finish()
    }

    fn toggle_repository(&mut self, repository: RepositoryId) -> Vec<ClientMsg> {
        if !self.toggled.remove(&repository) {
            self.toggled.insert(repository);
        }
        self.finish()
    }

    fn toggle_checkout(&mut self, checkout: CheckoutId) -> Vec<ClientMsg> {
        if !self.folded.remove(&checkout) {
            self.folded.insert(checkout);
        }
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
        self.finish()
    }

    // ---- The stages ----------------------------------------------------------

    /// Puts `stage` up, and whichever was up away.
    fn go(&mut self, stage: Stage) {
        self.stages.go(stage);
    }

    /// Leaves whichever stage is open for the Workspace.
    fn leave_stages(&mut self) {
        self.go(Stage::Workspace);
    }

    /// The Workspace asked for by its key or its nav: its pane takes the
    /// keys back, even from a console docked under it.
    fn back_to_workspace(&mut self) {
        self.console.set_fullscreen(false);
        self.leave_stages();
        self.give_back();
    }

    /// Shows the Feature stage, or goes back to the Workspace.
    fn show_features(&mut self, open: bool) -> Vec<ClientMsg> {
        if open {
            self.go(Stage::Feature);
        } else {
            self.back_to_workspace();
        }
        self.finish()
    }

    // ---- What extensions publish ------------------------------------------

    /// Where the person is, for a view scoped to a repository or a
    /// checkout: the repository the rail reads, and the checkout of the pane
    /// on screen, else that repository's own.
    fn here(&self) -> Here {
        let repository = self.chosen_repository();
        let shown = self
            .shown
            .on(Side::Workspace)
            .and_then(|pane| self.tree.lineage_of_pane(pane))
            .map(|(lineage, _)| lineage.checkout);
        let checkout = shown.or_else(|| repository.and_then(|r| r.checkouts.iter().find(|c| c.primary)));
        Here { repository: repository.map(|r| r.id), checkout: checkout.map(|c| c.id) }
    }

    /// The views away, or out on the one chosen, the newest at first.
    fn toggle_views(&mut self) {
        if self.docks.shows(&Panel::Views) {
            self.docks.hide(&Panel::Views);
        } else if !self.views.is_empty() {
            self.views.end_pop_out();
            self.docks.show(&Panel::Views);
        }
    }

    /// An agent's view over the stage up, which it goes back to when the
    /// view docks or closes. An extension's page gives way to it.
    fn show_view_page(&mut self) {
        if let Some(stage) = self.stages.over.open_view(self.stages.now()) {
            self.go(stage);
        }
    }

    fn settle_view_page(&mut self) {
        match self.stages.over.settle_view(self.views.popped(), self.stages.now()) {
            Some(Settled::EndPopOut) => self.views.end_pop_out(),
            Some(Settled::Back(stage)) => self.go(stage),
            None => {}
        }
    }

    /// A reference pressed in a view: what it points at is found from the
    /// pane that showed the view, or from the checkout on screen when no
    /// pane did. A reference that cannot be found is said so, and nothing
    /// else happens.
    fn open_ref(&mut self, from: Option<PaneId>, reference: Reference) -> Vec<ClientMsg> {
        match view_refs::resolve(&reference, from, self.here().checkout, &self.tree) {
            Err(text) => return self.note(text),
            Ok(Target::Pane(pane)) => return self.go_to_pane(pane),
            Ok(Target::Editor { checkout, path, line }) => self.open_in_editor(checkout, path, line),
            Ok(Target::History { checkout, sha }) => {
                self.dialogs.open_history_at(checkout, &sha, &self.tree, || self.awaited.name(), &mut self.out);
            }
            Ok(Target::Feature { repository, number }) => {
                if self.repository != Some(repository) {
                    self.repository = Some(repository);
                }
                self.opening_feature.wait_for(repository, number);
                self.go(Stage::Feature);
            }
        }
        self.finish()
    }

    /// Opens the saved view `id` where a link to it was followed, or in a
    /// panel of its own: a link followed in a saved panel's column gives
    /// the target the column after it; one followed in an agent's view or
    /// an extension's table brings out an open panel showing it, else
    /// opens it as a first column. A view the daemon does not hold is said
    /// so, since there is nothing to open.
    fn open_saved(&mut self, id: String, from: Option<SavedColumn>) -> Vec<ClientMsg> {
        if self.saved.get(&id).is_none() {
            self.passing.push(Event::Notice { text: format!("no saved view {id}") });
            return self.finish();
        }
        let columns = self.choices.get().saved_links == SavedLinks::Column;
        match from {
            Some(SavedColumn { panel: Panel::Saved(owner), column }) if columns && self.saved.follow(&owner, column, id.clone()) => {
                self.show_saved(owner);
            }
            Some(SavedColumn { panel: Panel::Saved(_), .. }) if !columns => match self.placed_panel(&id) {
                Some(placed) => self.show_saved(placed),
                None => self.float_saved(id),
            },
            _ => {
                let placed = self.placed_panel(&id);
                self.show_saved(placed.unwrap_or(id));
            }
        }
        self.finish()
    }

    /// The placed saved panel showing the view `id`, the one named for it
    /// first.
    fn placed_panel(&self, id: &str) -> Option<String> {
        let showing = self.saved.showing(id);
        showing.into_iter().find(|panel| self.docks.place_of(&Panel::Saved(panel.clone())).is_some())
    }

    /// Brings out the saved panel named `owner` as a dialog in front,
    /// floating it the first time.
    fn float_saved(&mut self, owner: String) {
        self.saved.open(&owner);
        let panel = Panel::Saved(owner);
        self.docks.place(&panel, Place::Float);
        self.docks.show(&panel);
    }

    /// Brings out the saved panel named `owner`, placing it on the right
    /// the first time and starting its trail if it has none.
    fn show_saved(&mut self, owner: String) {
        self.saved.open(&owner);
        let panel = Panel::Saved(owner);
        self.docks.place(&panel, Place::Dock(Dock::Right));
        self.docks.show(&panel);
    }

    /// Where the person is, for the saved views listed as here.
    fn whereabouts(&self) -> Whereabouts {
        let feature = self.opened_feature_number();
        Whereabouts { repository: self.chosen_repository().map(|r| r.id), feature }
    }

    fn opened_feature_number(&self) -> Option<i64> {
        let slug = self.features.opened().filter(|_| self.stages.now() == Stage::Feature)?;
        self.features.board()?.features.iter().find(|f| f.slug == slug).map(|f| f.number)
    }

    /// Reveals the feature a reference named, once the board of its
    /// repository is here to say which one is numbered so. The person going
    /// elsewhere meanwhile abandons it.
    fn settle_feature_ref(&mut self) {
        let (stage, chosen) = (self.stages.now(), self.repository);
        let there = |repository| stage == Stage::Feature && chosen == Some(repository);
        let scope = self.scope();
        match self.opening_feature.settle(there, self.features.board()) {
            Some(Found::Slug(slug)) => self.features.reveal(&slug, scope.as_ref(), &self.tree, &mut self.out),
            Some(Found::Missing(number)) => {
                self.passing.push(Event::Notice { text: format!("no feature #{number} in this repository") })
            }
            None => {}
        }
    }

    /// Tells the Feature stage which views name its repository's features.
    fn sync_feature_views(&mut self) {
        let views = self.scope().map(|scope| self.views.linked(&self.tree, scope.repository)).unwrap_or_default();
        self.features.set_views(views);
    }

    /// Opens an extension's view where it is: placed where its extension
    /// says the first time, then where the person put it — its dock, or a
    /// page over the stage up.
    fn open_extension_view(&mut self, extension: &str, view: &str) -> Vec<ClientMsg> {
        let here = self.here();
        let panel = Panel::extension(extension, view);
        let Some(found) = self.extension_views.get(&panel, &here) else {
            self.passing.push(Event::Notice { text: format!("{extension} shows no {view} here") });
            return self.finish();
        };
        self.docks.place(&panel, first_place(found.place));
        self.extension_views.touch(&panel);
        if self.docks.place_of(&panel) == Some(Place::Stage) {
            self.extension_views.set_page(Some(&panel));
            self.show_extension_page();
        } else {
            self.docks.show(&panel);
        }
        self.finish()
    }

    fn show_extension_page(&mut self) {
        if let Some(stage) = self.stages.over.open_extension(self.stages.now()) {
            self.go(stage);
        }
        self.key_page_console();
    }

    /// A page that is a console has the keys the moment it is up, as a
    /// table page does: the Workspace's pane is hidden behind it, and keys
    /// typed into the page would otherwise go there. A console still
    /// waiting for its pane is clicked into once it has one.
    fn key_page_console(&mut self) {
        self.settle_view_consoles();
        let page = self.extension_views.page().cloned().filter(|_| self.stages.now() == Stage::Extension);
        let name = page.and_then(|panel| match panel {
            Panel::Extension(name) => Some(name),
            _ => None,
        });
        let pane = name.and_then(|name| self.shown.view_side(&name)).and_then(|side| self.shown.on(side));
        if let Some(pane) = pane {
            self.shown.focus(pane);
        }
    }

    /// Back to the stage the page opened over, or the Workspace.
    fn leave_extension_page(&mut self) {
        if let Some(stage) = self.stages.over.leave_extension(self.stages.now()) {
            self.go(stage);
        }
    }

    /// Moves an extension's view to a page, or back into a dock, and
    /// remembers it there.
    fn extension_mode(&mut self, panel: &Panel, page: bool) {
        if self.extension_views.get(panel, &self.here()).is_none() {
            self.passing.push(Event::Notice { text: "that extension's view is not shown here".into() });
            return;
        }
        self.extension_views.touch(panel);
        if page {
            self.docks.stage(panel);
            self.extension_views.set_page(Some(panel));
            self.show_extension_page();
        } else {
            self.dock_extension(panel, None);
        }
    }

    /// Docks an extension's view in `dock`, or else the dock it is in, or
    /// where its extension says, or on the right; a page of it goes back
    /// to the stage it opened over.
    fn dock_extension(&mut self, panel: &Panel, dock: Option<Dock>) {
        let asked = self.extension_views.get(panel, &self.here()).map(|v| first_place(v.place));
        let dock = dock.or(self.docks.dock_of(panel)).unwrap_or(match asked {
            Some(Place::Dock(dock)) => dock,
            _ => Dock::Right,
        });
        self.docks.move_to(panel, dock);
        if self.extension_views.page() == Some(panel) {
            self.extension_views.set_page(None);
            self.leave_extension_page();
        }
    }

    /// Keeps the page level with what the daemon publishes and where the
    /// person went: one taken away, or out of scope where the person now
    /// is, goes back to the stage it opened over saying so; a page the
    /// person left for another stage closes; and the extension stage is
    /// never up with no page on it. A docked view gone or out of scope is
    /// only not drawn, by `docks`.
    fn settle_extension(&mut self) {
        let paged = self.stages.now() == Stage::Extension;
        match self.extension_views.page().cloned() {
            Some(panel) if self.extension_views.get(&panel, &self.here()).is_none() => {
                if paged {
                    if let Some((extension, title)) = self.extension_views.page_title() {
                        self.passing.push(Event::Notice { text: format!("{extension}: {title} is no longer shown here") });
                    }
                }
                self.extension_views.set_page(None);
                self.leave_extension_page();
            }
            Some(_) if !paged => self.extension_views.set_page(None),
            Some(_) => {}
            None => self.leave_extension_page(),
        }
    }

    /// One of the view's actions; the view changes when its extension
    /// publishes it again, never before.
    fn extension_action(&mut self, panel: &Panel, action: &str) -> Vec<ClientMsg> {
        let here = self.here();
        self.extension_views.touch(panel);
        match self.extension_views.act(panel, &here, action) {
            Ok(Some(msg)) => self.out.push(msg),
            Ok(None) => {}
            Err(text) => self.passing.push(Event::Notice { text }),
        }
        self.finish()
    }

    fn extension_key(&mut self, panel: &Panel, action: Action) -> Vec<ClientMsg> {
        let here = self.here();
        self.extension_views.touch(panel);
        match action {
            Action::Next => self.extension_views.step(panel, &here, 1),
            Action::Previous => self.extension_views.step(panel, &here, -1),
            Action::First => self.extension_views.select_end(panel, &here, false),
            Action::Last => self.extension_views.select_end(panel, &here, true),
            Action::Choose => return self.extension_default(panel),
            _ => {}
        }
        self.finish()
    }

    /// The view's default action on its selected row; a view without one
    /// leaves Enter and a double-click doing nothing, said nowhere.
    fn extension_default(&mut self, panel: &Panel) -> Vec<ClientMsg> {
        let here = self.here();
        match self.extension_views.default_action(panel, &here) {
            Some(action) => self.extension_action(panel, &action),
            None => self.finish(),
        }
    }

    /// Runs an extension's command with where the person is: the row the
    /// page says the rail has picked, else the Checkouts stage's row, else
    /// the pane on screen. The daemon reads it against its own tree.
    fn extension_command(&mut self, extension: &str, command: &str, checkout: Option<CheckoutId>, pane: Option<PaneId>) {
        if !self.extension_keys.holds(extension, command) {
            self.passing.push(Event::Notice { text: format!("{extension} has no command {command}") });
            return;
        }
        let staged = (self.stages.now() == Stage::Checkouts)
            .then(|| self.chosen_repository().and_then(|repo| self.checkouts.checkout(repo)))
            .flatten();
        let pane = pane.or_else(|| checkout.is_none().then(|| self.shown.pane()).flatten());
        let context = View {
            project: self.project,
            checkout: checkout.or(staged).or_else(|| self.shown_checkout()),
            pane,
            active: self.focused,
        };
        self.out.push(ClientMsg::ExtensionCommand { extension: extension.into(), command: command.into(), context });
    }

    /// Whether an extension's view is on screen: shown in its dock, or the
    /// page the stage is up on.
    fn extension_on(&self, panel: &Panel) -> bool {
        self.docks.shows(panel) || (self.stages.now() == Stage::Extension && self.extension_views.page() == Some(panel))
    }

    /// Gives each console view on screen a screen of its own, and takes it
    /// away with the view, as the daemon's views say.
    fn settle_view_consoles(&mut self) {
        let wanted = self.extension_views.consoles_on(&self.here(), |panel| self.extension_on(panel));
        self.shown.sync_views(&wanted, &mut self.out);
    }

    fn extensions_page(&self) -> page::ExtensionsPage {
        let here = self.here();
        let docks = &self.docks;
        let page = self.extension_views.page().filter(|_| self.stages.now() == Stage::Extension);
        let on = |panel: &Panel| self.extension_on(panel);
        let with_screen = |mut table: page::ExtTable| {
            if let (Some(console), Panel::Extension(name)) = (&mut table.console, &table.panel) {
                let side = self.shown.view_side(name);
                console.side = side;
                console.keyed = side.is_some_and(|side| side == self.shown.keys());
            }
            table
        };
        page::ExtensionsPage {
            views: self.extension_views.tabs(&here, |panel| docks.place_of(panel) == Some(Place::Stage), on),
            tables: self
                .extension_views
                .present(&here)
                .filter(|panel| on(panel))
                .filter_map(|panel| self.extension_views.table(&panel, &here, page == Some(&panel)))
                .map(with_screen)
                .collect(),
            confirm: self.extension_views.confirm(),
            listed: self.extension_views.listed(),
            commands: self.extension_keys.page(),
            groups: self.extension_keys.groups(self.extension_views.views()),
        }
    }

    // ---- The console ---------------------------------------------------------

    fn console_gesture(&mut self, gesture: ConsoleGesture) -> Vec<ClientMsg> {
        match gesture {
            ConsoleGesture::Toggle => self.toggle_console(),
            ConsoleGesture::Tab => self.open_console_tab(),
            ConsoleGesture::Dock => self.dock_console(),
            ConsoleGesture::Fullscreen => self.toggle_console_fullscreen(),
            ConsoleGesture::Hide => self.hide_console(),
            ConsoleGesture::Select { pane } => self.show_console(PaneId(pane)),
            ConsoleGesture::Spawn => self.spawn_console(Made::ShowConsole),
            ConsoleGesture::Close { pane } => {
                if self.console.holds(PaneId(pane)) {
                    self.out.push(ClientMsg::Kill { pane: PaneId(pane) });
                }
            }
            ConsoleGesture::Split => self.toggle_split(),
            ConsoleGesture::CloseSide { pane } => self.close_side(PaneId(pane)),
        }
        self.finish()
    }

    fn dock_gesture(&mut self, gesture: DockGesture) -> Vec<ClientMsg> {
        match gesture {
            DockGesture::Move { panel, dock } => self.move_panel(&panel, dock),
            DockGesture::Show { panel } => self.show_panel(&panel),
            DockGesture::Hide { panel } => match panel {
                Panel::Console => self.hide_console(),
                _ => self.docks.hide(&panel),
            },
            DockGesture::Size { dock, size } => self.docks.set_size(dock, size),
            DockGesture::Float { panel } => self.docks.float(&panel),
            DockGesture::Bounds { panel, bounds } => self.docks.set_bounds(&panel, bounds),
            DockGesture::Raise { panel } => self.docks.raise(&panel),
        }
        self.finish()
    }

    /// The panel goes to `dock` and is shown there, as if shown from its
    /// key: the console out of fullscreen or the Console stage, an
    /// extension's view off the stage.
    fn move_panel(&mut self, panel: &Panel, dock: Dock) {
        match panel {
            Panel::Console => {
                self.docks.move_to(panel, dock);
                self.dock_console();
            }
            Panel::Views | Panel::Saved(_) => self.docks.move_to(panel, dock),
            Panel::Extension(_) => {
                self.extension_views.touch(panel);
                self.dock_extension(panel, Some(dock));
            }
        }
    }

    /// A dock's tab picked: the panel on screen in its dock, and the
    /// console with the keys, as its own key shows it.
    fn show_panel(&mut self, panel: &Panel) {
        match panel {
            Panel::Console => self.dock_console(),
            _ => {
                if let Panel::Extension(_) = panel {
                    self.extension_views.touch(panel);
                }
                self.docks.show(panel);
            }
        }
    }

    /// The panels that exist now: the console always, the views while an
    /// agent has one up, and each extension's view listed where the person
    /// is.
    fn present(&self) -> Vec<Panel> {
        let mut present = vec![Panel::Console];
        if !self.views.is_empty() {
            present.push(Panel::Views);
        }
        present.extend(self.extension_views.present(&self.here()));
        present.extend(self.saved.present());
        present
    }

    /// Where the console is with the stage up now.
    fn console_place(&self) -> ConsolePlace {
        self.console.place(self.stages.now(), self.docks.shows(&Panel::Console))
    }

    /// Whether the keys are a console's rather than the Workspace's pane's.
    /// An extension's console view is neither: its keys leave the console's
    /// own choices (fullscreen, the split) alone.
    fn console_typing(&self) -> bool {
        matches!(self.shown.keys(), Side::First | Side::Second)
    }

    /// Whether the console's sides are on screen.
    fn console_on(&self) -> bool {
        self.shown.on(Side::First).is_some()
    }

    /// A hidden console is shown docked for a console asked for: one the
    /// person means to type into.
    fn reveal_console(&mut self) {
        if self.console_place() == ConsolePlace::Hidden {
            self.docks.show(&Panel::Console);
            if self.stages.now() == Stage::Composer {
                self.go(Stage::Workspace);
            }
        }
    }

    /// Puts a console on screen with the keys. One already on a side of
    /// the split takes the keys there; another takes the place of the side
    /// with the keys, as picking a file in a split editor does.
    fn show_console(&mut self, pane: PaneId) {
        if !self.console.holds(pane) {
            return;
        }
        self.reveal_console();
        let showing = self.console_on();
        if !(showing && self.shown.focus(pane)) {
            if showing && self.shown.keys() == Side::Second {
                self.console.set_beside(pane);
            } else if self.console.beside() != Some(pane) {
                self.console.choose(pane);
            }
            self.console_on_screen(Some(pane));
        }
    }

    /// Whatever the console has chosen goes on screen: its tab on the
    /// first side and the split's on the second. `keys` takes the keys —
    /// the Workspace's pane among them — or else the console's pane that
    /// had them if it is still there, or else the first.
    fn console_on_screen(&mut self, keys: Option<PaneId>) {
        let Some(first) = self.console.chosen() else {
            return;
        };
        let keys = keys.or(self.shown.pane().filter(|_| self.console_typing()));
        self.shown.show_on(Side::First, first, &mut self.out);
        match self.console.beside() {
            Some(second) => {
                self.shown.show_on(Side::Second, second, &mut self.out);
            }
            None => self.shown.close(Side::Second, &mut self.out),
        }
        if !keys.is_some_and(|pane| self.shown.focus(pane)) {
            self.shown.focus(first);
        }
    }

    /// Splits the console: its second side shows the first tab on neither
    /// side, or a new console when every one is shown, and takes the keys.
    /// Split already, the split goes away and the side with the keys fills
    /// the console. The split is the run's, never `desktop.json`'s.
    fn toggle_split(&mut self) {
        if let (Some(first), Some(second)) = (self.console.chosen(), self.console.beside()) {
            let keyed = self.shown.pane().filter(|_| self.console_typing());
            self.close_side(if keyed == Some(second) { first } else { second });
            return;
        }
        if self.console.chosen().is_none() {
            // Nothing to split beside: the first console is asked for, and
            // the split waits for the person to ask again.
            self.console_takes();
            return;
        }
        self.reveal_console();
        match self.console.unshown() {
            Some(other) => self.show_beside(other),
            None => {
                self.console_on_screen(None);
                self.spawn_console(Made::ShowBeside);
            }
        }
    }

    fn show_beside(&mut self, pane: PaneId) {
        if self.console.set_beside(pane) {
            self.reveal_console();
            self.console_on_screen(Some(pane));
        }
    }

    /// A split side's close: that side leaves the split and the other
    /// fills the console. Its console stays a tab, and the daemon keeps it.
    fn close_side(&mut self, pane: PaneId) {
        let (Some(first), Some(second)) = (self.console.chosen(), self.console.beside()) else {
            return;
        };
        let keep = match pane {
            _ if pane == first => second,
            _ if pane == second => first,
            _ => return,
        };
        self.console.unsplit(keep);
        if self.console_on() {
            let keys = if self.console_typing() { keep } else { self.shown.pane().unwrap_or(keep) };
            self.console_on_screen(Some(keys));
        }
    }

    /// The next pane on screen takes the keys, in the order they are
    /// drawn: the Workspace's and the panes split beside it, then the
    /// console's sides.
    fn other_side(&mut self) {
        let sides = self.shown.grid().sides().into_iter().chain([Side::First, Side::Second]).chain(self.shown.view_sides());
        let visible: Vec<PaneId> = sides
            .filter(|&side| self.side_visible(side))
            .filter_map(|side| self.shown.on(side))
            .collect();
        let at = visible.iter().position(|&pane| Some(pane) == self.shown.pane());
        let next = at.map_or(0, |at| (at + 1) % visible.len());
        if let Some(&pane) = visible.get(next) {
            self.shown.focus(pane);
        }
    }

    /// Whether `side` is drawn: the Workspace's while no stage covers it,
    /// and the console's while it is not hidden.
    fn side_visible(&self, side: Side) -> bool {
        match side {
            Side::Workspace | Side::Beside(_) => {
                !matches!(
                    self.stages.now(),
                    Stage::Feature | Stage::Checkouts | Stage::Code | Stage::Console | Stage::Extension | Stage::View
                )
                    && !self.console.fullscreen()
            }
            Side::First | Side::Second => self.console_place() != ConsolePlace::Hidden,
            // A view's screen is kept only while its view is on screen.
            Side::View(_) => true,
        }
    }

    /// The keys go back to the Workspace's pane, when it has one.
    fn give_back(&mut self) {
        if let Some(pane) = self.shown.on(Side::Workspace) {
            self.shown.focus(pane);
        }
    }

    /// The console comes on screen with the keys: its chosen tabs, or a
    /// new console when there is none yet.
    fn console_takes(&mut self) {
        match self.console.chosen() {
            Some(_) => {
                self.reveal_console();
                self.console_on_screen(None);
            }
            None => self.spawn_console(Made::ShowConsole),
        }
    }

    /// Asks the daemon for a console, shown as `then` says once the daemon
    /// says it exists.
    fn spawn_console(&mut self, then: Made) {
        if !self.status.daemon_can(CONSOLES) {
            self.passing.push(Event::Notice {
                text: "argusd is too old to start a console: run `argus server restart`".into(),
            });
            return;
        }
        let request_id = self.awaited.ask(then);
        self.out.push(ClientMsg::SpawnConsole { request_id });
    }

    fn toggle_console(&mut self) {
        match self.console_place() {
            ConsolePlace::Hidden => self.dock_console(),
            _ => self.hide_console(),
        }
    }

    /// In its dock: from hidden, from the Console stage, or out of
    /// fullscreen.
    fn dock_console(&mut self) {
        self.console.set_fullscreen(false);
        self.docks.show(&Panel::Console);
        if matches!(self.stages.now(), Stage::Console | Stage::Composer) {
            self.go(Stage::Workspace);
        }
        self.console_takes();
    }

    /// Hidden wherever it was; its panes go and the keys go back as
    /// `finish` settles it.
    fn hide_console(&mut self) {
        self.console.set_fullscreen(false);
        self.docks.hide(&Panel::Console);
        if self.stages.now() == Stage::Console {
            self.go(Stage::Workspace);
        }
    }

    fn open_console_tab(&mut self) {
        self.console.set_fullscreen(false);
        self.go(Stage::Console);
        self.console_takes();
    }

    /// Fullscreen beside the sidebar, and back to where it was entered from.
    fn toggle_console_fullscreen(&mut self) {
        if self.console.fullscreen() {
            self.console.set_fullscreen(false);
        } else {
            self.reveal_console();
            self.console.set_fullscreen(true);
            self.console_takes();
        }
    }

    /// A hidden console lets its panes go and gives the keys back,
    /// however it came to be hidden: its key, the composer, or leaving the
    /// Console stage.
    fn settle_console(&mut self) {
        if self.console_place() == ConsolePlace::Hidden && self.console_on() {
            self.shown.close(Side::Second, &mut self.out);
            self.shown.close(Side::First, &mut self.out);
            self.give_back();
        }
    }

    /// Shows the composer, or goes back to the Workspace.
    fn show_composer(&mut self, open: bool) -> Vec<ClientMsg> {
        self.go(if open { Stage::Composer } else { Stage::Workspace });
        self.finish()
    }

    /// A notice on the bar, said partway through a gesture: `note` would
    /// finish the gesture there and then.
    /// Asks the daemon to open `path` in the person's editor. The daemon
    /// opens it on its own machine, so an editor that brings a window of its
    /// own would put that window on a host nobody is sitting at: over ssh
    /// only an editor in a pane, which this window draws, is asked for.
    fn open_in_editor(&mut self, checkout: CheckoutId, path: String, line: Option<u32>) {
        let (external, command) = self.choices.editor();
        if let (true, Some(host)) = (external, self.status.host()) {
            let said = format!("an editor with a window of its own would open on {host}: set it to open in a pane in Settings");
            self.say(&said);
            return;
        }
        let request_id = self.awaited.ask(Made::ShowOverStages);
        self.out.push(ClientMsg::OpenInEditor { checkout, path, line, external, command, request_id });
    }

    fn say(&mut self, text: &str) {
        self.passing.push(Event::Notice { text: text.to_owned() });
    }

    /// Shows the Code stage, or puts it away for the Workspace.
    fn show_code(&mut self, open: bool) -> Vec<ClientMsg> {
        if open {
            self.open_code();
        } else if self.stages.now() == Stage::Code {
            self.go(Stage::Workspace);
        }
        self.finish()
    }

    /// The Code stage as it was left. Only a checkout picked in Checkouts
    /// moves it to another; the pane on screen picks one only the first
    /// time, since the Workspace showing a pane of another checkout is not
    /// a request to read it, and reading it would lose the file and the
    /// place in it.
    fn open_code(&mut self) {
        if !self.status.daemon_can(CODE) {
            self.say(TOO_OLD_FOR_CODE);
            return;
        }
        let picked = match self.stages.now() {
            Stage::Checkouts => self.chosen_repository().cloned().and_then(|repo| self.checkouts.checkout(&repo)),
            _ => None,
        };
        match (picked, self.code.as_ref().map(CodeStage::checkout)) {
            (Some(picked), Some(open)) if open != picked => {
                self.open_code_at(picked, CodeVersion::WorkingTree, None, None, None)
            }
            (_, Some(_)) => self.enter_code(),
            (picked, None) => match picked.or_else(|| self.shown_checkout()) {
                Some(checkout) => self.open_code_at(checkout, CodeVersion::WorkingTree, None, None, None),
                None => self.say("show a pane, or pick a checkout in Checkouts, first"),
            },
        }
    }

    fn open_code_at(&mut self, checkout: CheckoutId, version: CodeVersion, path: Option<String>, line: Option<u32>, back: Option<Back>) {
        let Some(info) = self.tree.checkout(checkout) else {
            return;
        };
        let (code, msgs) = CodeStage::open(info, version, path, line, back, || self.awaited.name());
        self.code = Some(code);
        self.out.extend(msgs);
        self.enter_code();
    }

    fn enter_code(&mut self) {
        self.stages.enter_code();
    }

    /// Escape: back to the review the stage was opened from, as it was,
    /// over the stage it was opened over; or only to that stage.
    fn leave_code(&mut self) {
        if self.code.as_ref().is_some_and(|c| c.back().is_some()) && self.dialogs.unpark() {
            if let Some(code) = &mut self.code {
                code.forget_back();
            }
        }
        self.stages.leave_code();
    }

    fn code_click(&mut self, target: CodeClick) -> Vec<ClientMsg> {
        if matches!(target, CodeClick::Back) {
            self.leave_code();
        } else if let Some(code) = &mut self.code {
            let msgs = code.click(target, || self.awaited.name());
            self.out.extend(msgs);
        }
        self.finish()
    }

    /// Sends the Code stage's comment, or with no `body` puts the box away.
    fn code_comment(&mut self, body: Option<String>) -> Vec<ClientMsg> {
        let Some(code) = &mut self.code else {
            return self.finish();
        };
        let Some(body) = body else {
            code.cancel_comment();
            return self.finish();
        };
        match code.comment(self.tree.checkout(code.checkout()), &body) {
            Ok(msg) => self.out.push(msg),
            Err(text) => self.say(text),
        }
        self.finish()
    }

    fn code_field(&mut self, gesture: ReviewJumpGesture, search: bool) -> Vec<ClientMsg> {
        if let Some(code) = &mut self.code {
            let msgs = if search {
                code.search(gesture, || self.awaited.name())
            } else {
                code.jump(gesture, || self.awaited.name())
            };
            self.out.extend(msgs);
        }
        self.finish()
    }

    /// A key of the Code stage. E opens the file at the cursor in the
    /// person's editor, as the review's does.
    fn code_action(&mut self, action: Action) -> Vec<ClientMsg> {
        if action == Action::GoBack {
            if self.code.as_ref().is_some_and(|c| c.back().is_some()) {
                self.leave_code();
            }
            return self.finish();
        }
        let Some(code) = &mut self.code else {
            return self.finish();
        };
        match action {
            // Escape closes what is over the file and leaves the stage up:
            // the stage keys and B are the ways out.
            Action::CloseLayer => {
                code.escape();
            }
            Action::ToggleOutline => code.toggle_outline(),
            Action::Next => code.move_by(1),
            Action::Previous => code.move_by(-1),
            Action::First => code.ends(false),
            Action::Last => code.ends(true),
            Action::NextFile => code.next_change(true),
            Action::PreviousFile => code.next_change(false),
            Action::Filter => code.open_jump(),
            Action::Search => code.open_search(),
            Action::Pick => code.toggle_pick(),
            Action::Comment => {
                if let Err(text) = code.compose(self.tree.checkout(code.checkout())) {
                    self.passing.push(Event::Notice { text: text.into() });
                }
            }
            Action::Refresh => {
                let msgs = code.refresh(|| self.awaited.name());
                self.out.extend(msgs);
            }
            Action::OpenEditor => match code.editor_target() {
                Some((checkout, path, line)) => self.open_in_editor(checkout, path, line),
                None => self.say("no file to open"),
            },
            _ => {}
        }
        self.finish()
    }

    /// Shows the Checkouts stage, or puts it away for the Workspace.
    fn show_checkouts(&mut self, open: bool) -> Vec<ClientMsg> {
        if open {
            self.go(Stage::Checkouts);
        } else if self.stages.now() == Stage::Checkouts {
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
        self.features.set_layout(layout, scope.as_ref(), &self.tree, &mut self.out);
        self.finish()
    }

    fn filter_features(&mut self, filter: String) -> Vec<ClientMsg> {
        let scope = self.scope();
        self.features.set_filter(filter, scope.as_ref(), &self.tree, &mut self.out);
        self.finish()
    }

    /// A key of the Feature stage that moves a cursor or changes what its
    /// index lists. The layout and the fold are the palette's commands too.
    fn feature_action(&mut self, action: Action) -> Vec<ClientMsg> {
        let by = match action {
            Action::Next => 1,
            Action::Previous => -1,
            Action::First => isize::MIN,
            Action::Last => isize::MAX,
            Action::NextFlow | Action::PreviousFlow => {
                self.features.step_flow(if action == Action::NextFlow { 1 } else { -1 });
                return self.finish();
            }
            Action::Layout | Action::Folded => {
                self.run_command(action);
                return self.finish();
            }
            _ => return self.finish(),
        };
        let scope = self.scope();
        self.features.step(by, scope.as_ref(), &self.tree, &mut self.out);
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

    /// A gesture in the note a new feature is written as. Create waits on
    /// the board: the note closes only once a pushed board holds the
    /// feature it became, which then opens.
    fn capture_gesture(&mut self, gesture: CaptureGesture) -> Vec<ClientMsg> {
        match gesture {
            CaptureGesture::Open => self.open_capture(),
            CaptureGesture::Note { note } => {
                if let Some(capture) = self.capture.get_mut() {
                    capture.write(note);
                }
            }
            CaptureGesture::Create { note } => {
                let scope = self.scope();
                let (Some(capture), Some(scope)) = (self.capture.get_mut(), scope) else {
                    return self.note("no repository to write the feature in");
                };
                if let Err(why) = capture.create(note, &scope, self.features.board(), &mut self.out) {
                    self.passing.push(Event::Notice { text: why });
                }
            }
            CaptureGesture::Discard => self.capture.close(),
        }
        self.finish()
    }

    fn open_capture(&mut self) {
        if self.scope().is_none() {
            self.passing.push(Event::Notice { text: "choose a repository to write a feature in".into() });
            return;
        }
        self.go(Stage::Feature);
        self.capture.open();
    }

    /// The note became a feature on the board: it closes, and the feature
    /// opens.
    fn capture_landed(&mut self) {
        let landed = self.features.board().and_then(|board| self.capture.landed(board));
        if let Some(slug) = landed {
            let scope = self.scope();
            self.features.open(Some(&slug), scope.as_ref(), &self.tree, &mut self.out);
        }
    }

    fn open_feature(&mut self, slug: Option<&str>) -> Vec<ClientMsg> {
        let scope = self.scope();
        self.features.open(slug, scope.as_ref(), &self.tree, &mut self.out);
        self.finish()
    }

    fn edit_feature(&mut self, edit: FeatureEdit) -> Vec<ClientMsg> {
        let scope = self.scope();
        if let Err(why) = self.features.edit(edit, scope.as_ref(), &mut self.out) {
            self.passing.push(Event::Notice { text: why });
        }
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
        let Some(repo) = self.in_project(checkout).cloned() else {
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
        if let Some(repository) = self.in_project(checkout).map(|r| r.id) {
            self.open_on_row(repository, checkout);
        }
        self.finish()
    }

    /// A checkout of the rail's project, and the repository holding it.
    fn in_project(&self, checkout: CheckoutId) -> Option<&argus_protocol::RepositoryInfo> {
        let project = self.current_project()?.id;
        let lineage = self.tree.lineage_of_checkout(checkout).filter(|lineage| lineage.project.id == project)?;
        Some(lineage.repository)
    }

    fn open_on_row(&mut self, repository: RepositoryId, checkout: CheckoutId) {
        self.repository = Some(repository);
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
        if action == Action::OpenCode {
            if !self.status.daemon_can(CODE) {
                self.say(TOO_OLD_FOR_CODE);
            } else if let Some((checkout, oid, path)) = self.dialogs.park_history() {
                let back = Back { label: "history".into() };
                self.open_code_at(checkout, CodeVersion::Commit(oid), path, None, Some(back));
            }
            return self.finish();
        }
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
        if action == Action::OpenCode {
            if !self.status.daemon_can(CODE) {
                self.say(TOO_OLD_FOR_CODE);
            } else if let Some((checkout, path, line, version)) = self.dialogs.park_review() {
                let name = path.rsplit('/').next().unwrap_or(&path);
                let back = Back { label: format!("review · {name}:{line}") };
                self.open_code_at(checkout, version, Some(path), Some(line), Some(back));
            }
            return self.finish();
        }
        if matches!(action, Action::Unfold | Action::UnfoldAll) {
            self.dialogs.review_unfold(None, action == Action::UnfoldAll, || self.awaited.name(), &mut self.out);
            return self.finish();
        }
        if let Some((checkout, path, line)) = self.dialogs.review_action(action, &self.tree, &mut self.passing) {
            self.open_in_editor(checkout, path, line);
        }
        self.finish()
    }

    fn review_jump(&mut self, gesture: ReviewJumpGesture) -> Vec<ClientMsg> {
        self.dialogs.review_jump(gesture);
        self.finish()
    }

    fn review_click(&mut self, target: ReviewClick) -> Vec<ClientMsg> {
        if let ReviewClick::Fold { row, all } = target {
            self.dialogs.review_unfold(Some(row), all, || self.awaited.name(), &mut self.out);
            return self.finish();
        }
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
        self.recent.open(SessionsDialog::new(repository, name, !asked));
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
        let Some(dialog) = self.recent.get_mut() else {
            return self.finish();
        };
        let (sessions, tree) = (&self.sessions, self.tree.as_slice());
        let then = match (action, row) {
            // The full transcript holds the keys until Escape gives the
            // list back.
            (Action::CloseLayer, _) if dialog.close_reader() => SessionsThen::Nothing,
            _ if dialog.reading() => dialog.reader_action(action, row, sessions, tree),
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
            (Action::Transcript, _) => dialog.open_reader(sessions, tree),
            (Action::Choose, _) => dialog.choose(sessions, tree),
            (Action::Resume, _) => dialog.resume(sessions, tree),
            (Action::CloseLayer, _) => {
                self.recent.close();
                SessionsThen::Nothing
            }
            _ => SessionsThen::Nothing,
        };
        match then {
            SessionsThen::Nothing => {}
            SessionsThen::Say(text) => self.passing.push(Event::Notice { text }),
            SessionsThen::Copy(text) => self.copied = Some(text),
            // Only a pane the tree still holds: the listing may be older
            // than the pane's end.
            SessionsThen::GoTo(pane) if self.project_of(pane).is_some() => {
                self.recent.close();
                self.leave_stages();
                self.show(pane);
            }
            SessionsThen::GoTo(_) => self.passing.push(Event::Notice { text: "that session's pane has ended".into() }),
            // The dialog stays up until the pane is made and the tree
            // holds it, so a refused resume leaves it where it was.
            SessionsThen::Resume { checkout, harness, session_id, from } => {
                let request_id = self.awaited.ask(Made::Resumed(from));
                self.out.push(ClientMsg::ResumeSession { request_id, checkout, harness, session_id });
            }
        }
        self.finish()
    }

    /// What the sessions dialog's search field says now.
    fn search_sessions(&mut self, search: String) -> Vec<ClientMsg> {
        if let Some(dialog) = self.recent.get_mut() {
            dialog.set_search(search);
        }
        self.finish()
    }

    fn find_in_transcript(&mut self, find: String) -> Vec<ClientMsg> {
        if let Some(dialog) = self.recent.get_mut() {
            dialog.find(find, &self.sessions);
        }
        self.finish()
    }

    /// What a gesture asked to put on the clipboard, once: the clipboard
    /// is the window's to write, not the session's.
    pub fn take_copied(&mut self) -> Option<String> {
        self.copied.take()
    }

    fn filter_sessions(&mut self, harness: Option<String>, checkout: Option<String>) -> Vec<ClientMsg> {
        if let Some(dialog) = self.recent.get_mut() {
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

    /// A feature chosen in the palette: the Feature stage opens on it,
    /// listed, unfolded if the fold hides it and the filter cleared if that
    /// hides it.
    fn go_to_feature(&mut self, slug: &str) -> Vec<ClientMsg> {
        self.go(Stage::Feature);
        let scope = self.scope();
        self.features.reveal(slug, scope.as_ref(), &self.tree, &mut self.out);
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

    /// The pane with the keys, while its side is drawn. A stage keeps the
    /// Workspace's pane to come back to, but hides it and takes its keys
    /// away.
    fn on_screen(&self) -> Option<PaneId> {
        self.shown.pane().filter(|_| self.side_visible(self.shown.keys()))
    }

    pub fn found_fonts(&mut self, fonts: Vec<crate::page::FontFamily>) -> Vec<ClientMsg> {
        self.choices.found_fonts(fonts);
        Vec::new()
    }

    /// One choice in the Settings dialog. The page draws the look from
    /// what comes back, and `commands` puts the backdrop on the window.
    fn set(&mut self, change: SettingChange) -> Vec<ClientMsg> {
        if let Err(reason) = self.choices.change(change) {
            self.passing.push(Event::Notice { text: reason });
        }
        self.features.set_opens_as(self.choices.get().feature_layout);
        let scope = self.scope();
        self.features.settle(scope.as_ref(), &self.tree, &mut self.out);
        self.finish()
    }

    /// The theme and backdrop chosen, which the window's frame draws.
    pub fn look(&self) -> (Theme, Backdrop) {
        let chosen = self.choices.get();
        (chosen.theme, chosen.backdrop)
    }

    /// How large the window is drawn, in percent, which `commands` puts
    /// on the webview.
    pub fn zoom(&self) -> u16 {
        self.choices.get().zoom
    }

    /// The notifications the last trees raised, for the system to show.
    pub fn take_alerts(&mut self) -> Vec<Alert> {
        std::mem::take(&mut self.alerts)
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
            Action::Workspace
            | Action::Feature
            | Action::Checkouts
            | Action::Code
            | Action::Compose
            | Action::Shortcuts
            | Action::Settings
            | Action::Console
            | Action::ToggleConsole
            | Action::Views
            | Action::ExtensionPage
            | Action::ExtensionDock
            | Action::ConsoleFullscreen
            | Action::NewConsole
            | Action::SplitConsole
            | Action::DockLeft
            | Action::DockRight
            | Action::DockBottom
            | Action::OtherSide
            | Action::PreviousSide
            | Action::FocusLeft
            | Action::FocusRight
            | Action::FocusUp
            | Action::FocusDown
            | Action::MovePaneLeft
            | Action::MovePaneRight
            | Action::MovePaneUp
            | Action::MovePaneDown
            | Action::CloseSplit => self.run(action),
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
                self.features.set_layout(other, scope.as_ref(), &self.tree, &mut self.out);
            }
            Action::Folded => {
                let scope = self.scope();
                self.features.toggle_folded(scope.as_ref(), &self.tree, &mut self.out);
            }
            Action::NewFeature => self.open_capture(),
            Action::LinkTicket => self.open_link(false),
            Action::TrackerMap => self.open_link(true),
            Action::Reconcile => self.open_reconcile(),
            Action::ZoomIn => self.choices.zoom_in(),
            Action::ZoomOut => self.choices.zoom_out(),
            Action::ZoomReset => self.choices.reset_zoom(),
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
        let sent = if self.stages.now() == Stage::Checkouts {
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
        self.tree.lineage_of_pane(pane).map(|(lineage, _)| lineage.checkout.id)
    }

    /// The settings, when a change asks for them to be saved.
    pub fn take_settings(&mut self) -> Option<Settings> {
        // All eight, not short-circuited: each flag is cleared as it is read.
        let layouts = self.features.take_layouts_changed();
        let agent = self.agents.take_changed();
        let chime = self.status.take_chime_changed();
        let review = self.dialogs.take_layout_changed();
        let docks = self.docks.take_unsaved();
        let rail = self.rail_width.take_unsaved()
            | self.outline_width.take_unsaved()
            | self.brief_height.take_unsaved()
            | self.extension_views.take_unsaved();
        let chosen = self.choices.take_unsaved();
        let host = std::mem::take(&mut self.host_unsaved);
        (layouts || agent || chime || review || docks || rail || chosen || host).then(|| Settings {
            layouts: self.features.layouts().clone(),
            host: self.status.host().map(str::to_owned),
            hosts: self.switcher.visited().to_vec(),
            last_agent: self.agents.last().cloned(),
            chime: self.status.chimes(),
            review: self.dialogs.layout(),
            docks: Some(self.docks.layout().clone()),
            console: None,
            rail_width: self.rail_width.get(),
            views_width: None,
            outline_width: self.outline_width.get(),
            brief_height: self.brief_height.get(),
            extensions: self.extension_views.layout(),
            preferences: self.choices.get().clone(),
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
        let holding_shown = self
            .shown
            .on(Side::Workspace)
            .and_then(|pane| self.tree.lineage_of_pane(pane))
            .filter(|(lineage, _)| lineage.project.id == project.id)
            .map(|(lineage, _)| lineage.repository);
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
        let present = self.present();
        self.docks.set_present(present);
        self.settle_console();
        self.settle_extension();
        self.settle_view_page();
        self.settle_view_consoles();
        // The session under the dialog's cursor is read once it is there,
        // however the cursor got there: a key, a filter, or the listing.
        if let Some(wanted) = self.recent.get().and_then(|d| d.wants_read(&self.sessions, &self.tree)).cloned() {
            let request_id = self.awaited.name();
            self.out.extend(self.sessions.read(&self.status, &wanted, request_id));
        }
        let reads_board = matches!(self.stages.now(), Stage::Feature | Stage::Checkouts) || self.palette;
        self.features.set_board_wanted(reads_board);
        // Whatever changed may have changed which board the stage reads.
        let scope = self.scope();
        if self.status.is_connected() {
            self.features.refresh(scope.as_ref(), &mut self.out);
        }
        self.settle_feature_ref();
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
        if let Some(page) = self.choices.take() {
            events.push(Event::Settings(Box::new(page)));
        }
        // Before the rail, so the page knows which side has the keys when
        // the rail says which pane the Workspace shows.
        let present = self.present();
        self.docks.set_present(present);
        // Inline, not a method: `take` borrows the docks mutably while the
        // titles read the extension views.
        let (views, here, saved) = (&self.extension_views, self.here(), &self.saved);
        events.extend(self.docks.take(|panel| match panel {
            Panel::Console => "Console".into(),
            Panel::Views => "Views".into(),
            Panel::Extension(_) => views.get(panel, &here).map(|v| v.title.clone()).unwrap_or_default(),
            Panel::Saved(id) => saved.title(id),
        }));
        events.extend(self.console.take(self.console_place(), self.console_sides(), self.status.daemon_can(CONSOLES)));
        if let Some(rail) = self.rail_sent.take(Some(self.rail(now))) {
            events.push(Event::Rail(Box::new(rail)));
        }
        self.sync_feature_views();
        let scope = self.scope();
        if let Some(features) = self.features.take(scope.as_ref(), &self.tree, now) {
            events.push(Event::Features(Box::new(features)));
        }
        events.extend(self.dialogs.take(&self.tree, now));
        events.extend(self.recent.take(&self.sessions, &self.tree, now));
        events.extend(self.tracker_dialogs.take(self.features.board(), &self.tree, scope.as_ref(), now));
        let repository = scope.as_ref().map_or("", |scope| scope.repository_name.as_str());
        events.extend(self.capture.take(repository));
        events.extend(self.adding.take());
        let listing = self.palette.then(|| palette::build(self.current_project(), self.features.board(), &self.tree));
        if let Some(listing) = self.palette_sent.take(listing) {
            events.push(Event::Palette(Box::new(listing)));
        }
        if self.stages.now() == Stage::Checkouts {
            let repo = self.chosen_repository().cloned();
            events.extend(self.checkouts.take(repo.as_ref(), self.features.board(), self.features.peeked(), now));
        }
        // Sent only while the stage is up, and never taken back when it goes:
        // the page keeps what it last drew, scrolled where it was, for the
        // person coming back to it.
        if let Some(code) = self.code.as_ref().filter(|_| self.stages.now() == Stage::Code) {
            let page = code.page(self.tree.checkout(code.checkout()));
            if let Some(page) = self.code_sent.take(Some(page)) {
                events.push(Event::Code(Box::new(page)));
            }
        }
        let extensions = self.extensions_page();
        if let Some(page) = self.extensions_sent.take((extensions != page::ExtensionsPage::default()).then_some(extensions)) {
            events.push(Event::Extensions(Box::new(page)));
        }
        events.extend(self.extension_dialogs.take());
        let tree = &self.tree;
        let views_out = self.docks.dock_out(&Panel::Views);
        let can_press = self.status.daemon_can(VIEW_ACTIONS);
        if let Some(page) = self.views.take(views_out, can_press, |pane| tree.panes().find(|p| p.id == pane).map(|p| p.title.clone())) {
            events.push(Event::Views(Box::new(page)));
        }
        let placed = self.placed_saved();
        let (whereabouts, can_press) = (self.whereabouts(), self.status.daemon_can(SAVED_VIEWS));
        if let Some(page) = self.saved.take(whereabouts, &self.tree, &placed, can_press) {
            events.push(Event::SavedViews(Box::new(page)));
        }
        events.append(&mut self.passing);
        events
    }

    /// The saved views' panels the docks hold, docked or floating, by the
    /// id each is named with.
    fn placed_saved(&self) -> Vec<String> {
        let layout = self.docks.layout();
        [&layout.left, &layout.right, &layout.bottom]
            .into_iter()
            .flat_map(|dock| &dock.tabs)
            .chain(layout.floating.iter().map(|f| &f.panel))
            .filter_map(|panel| match panel {
                Panel::Saved(id) => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    /// Everything, for a page that has just loaded and holds nothing; the
    /// shown pane's whole screen follows with the next frame.
    pub fn everything(&mut self, now: u64) -> Vec<Event> {
        self.status.resend();
        self.choices.resend();
        self.dialogs.reload();
        self.recent.reload();
        self.tracker_dialogs.reload();
        self.adding.reload();
        self.capture.reload();
        self.console.reload();
        self.docks.reload();
        self.checkouts.reload();
        self.code_sent.forget();
        self.views.forget();
        self.saved.forget();
        self.extensions_sent.forget();
        self.extension_dialogs.reload();
        // A page that has just loaded has no palette open.
        self.palette = false;
        self.palette_sent.forget();
        self.rail_sent.forget();
        self.shown.refresh();
        let mut events = vec![Event::Keymap { groups: keymap::groups(), commands: keymap::commands(), layers: keymap::LAYERS }];
        events.append(&mut self.events(now));
        if !events.iter().any(|e| matches!(e, Event::Features(_))) {
            let scope = self.scope();
            events.push(Event::Features(Box::new(self.features.everything(scope.as_ref(), &self.tree, now))));
        }
        // The table is drawn only while its stage is up, but a page that
        // reloaded holds none to show when it goes up.
        if !events.iter().any(|e| matches!(e, Event::Checkouts(_))) {
            let repo = self.chosen_repository().cloned();
            events.extend(self.checkouts.take(repo.as_ref(), self.features.board(), self.features.peeked(), now));
        }
        events
    }

    /// Each side's changed rows, as a frame naming its pane.
    pub fn take_frames(&mut self) -> Vec<Event> {
        self.shown.take_frames().into_iter().map(Event::Frame).collect()
    }

    fn console_sides(&self) -> OnScreen {
        OnScreen {
            first: self.shown.on(Side::First),
            second: self.shown.on(Side::Second),
            keys: self.shown.pane(),
        }
    }

    fn rail(&self, now: u64) -> page::Rail {
        let choices = Choices {
            project: self.project,
            shown: self.shown.on(Side::Workspace),
            toggled: &self.toggled,
            folded: &self.folded,
            now,
        };
        let mut rail = rail::build(&self.tree, &choices, self.loaded);
        rail.repository = self.chosen_repository().map(|r| r.id.0);
        rail.stage = self.stages.now();
        rail.filled = self.shown.filled() && self.stages.now() == Stage::Workspace && self.on_screen().is_some() && !self.console_typing();
        rail.templates = self.agents.offered();
        rail.workspaces = self.switcher.workspaces();
        rail.hosts = self.switcher.hosts(self.status.host());
        let panes = rail.project.iter_mut().flat_map(|p| &mut p.repositories).flat_map(|r| &mut r.checkouts);
        for pane in panes.flat_map(|c| &mut c.panes) {
            pane.views = self.views.count(PaneId(pane.id));
        }
        rail.removing = self.switcher.removing(&self.tree);
        rail.width = self.rail_width.get();
        rail.outline_width = self.outline_width.get();
        rail.brief_height = self.brief_height.get();
        if let Some(shown) = &mut rail.shown {
            shown.resumed = self.sessions.resumed_from(PaneId(shown.pane)).cloned();
        }
        rail.row = self.shown.grid().page(self.shown.keys(), |pane| rail::shown(&self.tree, pane));
        rail
    }

    fn project_of(&self, pane: PaneId) -> Option<ProjectId> {
        self.tree.lineage_of_pane(pane).map(|(lineage, _)| lineage.project.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docks::FloatBounds;
    use crate::fixtures::{checkout, pane, project, repository};
    use crate::link_dialog::LinkPage;
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
        s.resize(Side::Workspace, 30, 100);
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
        s.resize(Side::Workspace, 30, 100);
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
    fn a_rename_is_asked_of_the_daemon_and_not_shown_before_it_answers() {
        let mut s = session(Hello::this_build());
        let sent = s.rename(PaneId(1), "the pty deadlock".into());
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::RenamePane { pane: PaneId(1), title }] if title == "the pty deadlock"
        ));
        assert!(s.rename(PaneId(99), "nowhere".into()).is_empty());
    }

    #[test]
    fn a_daemon_without_rename_is_not_sent_one() {
        let mut s = session(without_focus());
        assert!(s.rename(PaneId(1), "the pty deadlock".into()).is_empty());
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
                new_blob: None,
                new_lines: None,
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
        assert!(s.stages.now() == Stage::Checkouts, "nothing predicted");
        s.receive(ServerMsg::Created { request_id: *request_id, created: Some(Created::Pane(PaneId(9))) });
        let mut grown = tree();
        grown[0].repositories[0].checkouts[0].panes.push(pane(9, PaneKind::Editor, "brief.rs", PaneStatus::Idle));
        s.receive(with_tree(grown));
        assert_eq!(s.shown.pane(), Some(PaneId(9)));
        assert!(s.stages.now() != Stage::Checkouts, "the editor is on screen");
    }

    /// What a gesture asked of the daemon for the Code stage, the focus it
    /// reports along the way left out.
    fn code_asks(sent: Vec<ClientMsg>) -> Vec<ClientMsg> {
        sent.into_iter()
            .filter(|m| matches!(m, ClientMsg::ListCode { .. } | ClientMsg::ReadCode { .. } | ClientMsg::SearchCode { .. }))
            .collect()
    }

    #[test]
    fn o_in_the_review_reads_the_cursors_file_in_code_and_escape_comes_back_to_it() {
        let mut s = session(Hello::this_build());
        s.show_checkouts(true);
        let sent = s.open_review(CheckoutId(100));
        let [ClientMsg::Review { request_id: unstaged, .. }, ..] = sent.as_slice() else {
            panic!("{:?}", names(&sent));
        };
        s.receive(reviewed(argus_protocol::ReviewBase::Unstaged, *unstaged));
        s.review_action(Action::Next);
        let sent = code_asks(s.review_action(Action::OpenCode));
        assert!(matches!(
            sent.as_slice(),
            [
                ClientMsg::ListCode { checkout: CheckoutId(100), version: CodeVersion::WorkingTree, .. },
                ClientMsg::ReadCode { path, .. },
            ] if path == "src/brief.rs"
        ));
        assert!(s.stages.now() == Stage::Code);
        assert!(s.dialogs.review().is_none(), "put away, not closed");
        assert_eq!(s.code.as_ref().unwrap().page(None).back.as_deref(), Some("review · brief.rs:4"));

        assert!(code_asks(s.code_action(Action::CloseLayer)).is_empty());
        assert!(s.stages.now() == Stage::Code, "Escape stays in Code");
        let sent = code_asks(s.code_action(Action::GoBack));
        assert!(sent.is_empty(), "the review comes back as it was, not read again");
        assert!(s.dialogs.review().is_some());
        assert!(s.stages.now() == Stage::Checkouts, "over the stage it was opened over");
    }

    #[test]
    fn the_code_stage_comes_back_as_it_was_left_whatever_pane_is_on_screen() {
        let mut s = session(Hello::this_build());
        s.show_checkouts(true);
        s.show_code(true);
        s.show_features(false);
        s.call(Call::Select { pane: 3 });
        assert!(code_asks(s.show_code(true)).is_empty(), "not read again for another checkout's pane");
        assert_eq!(s.code.as_ref().unwrap().checkout(), CheckoutId(100));
    }

    #[test]
    fn leaving_the_code_stage_leaves_the_page_what_it_drew() {
        let mut s = session(Hello::this_build());
        s.show_checkouts(true);
        s.show_code(true);
        s.events(0);
        s.show_features(false);
        assert!(s.events(0).iter().all(|e| !matches!(e, Event::Code(_))), "no closed page sent");
    }

    #[test]
    fn the_code_stage_is_not_offered_by_a_daemon_that_cannot_read_code() {
        let mut s = session(Hello { capabilities: Vec::new(), ..Hello::this_build() });
        s.show_checkouts(true);
        let sent = s.show_code(true);
        assert!(sent.is_empty());
        assert!(s.stages.now() == Stage::Checkouts);
        assert!(notices(&mut s).iter().any(|n| n.contains("too old to read code")));
    }

    #[test]
    fn c_in_the_history_reads_its_commit_in_code_and_escape_brings_the_history_back() {
        let mut s = session(Hello::this_build());
        s.show_checkouts(true);
        let sent = s.open_history(CheckoutId(100));
        let Some(ClientMsg::ListCommits { request_id, .. }) = sent.iter().find(|m| matches!(m, ClientMsg::ListCommits { .. })) else {
            panic!("{:?}", names(&sent));
        };
        let commit = argus_protocol::CommitInfo {
            oid: "a".repeat(40),
            short: "aaaaaaa".into(),
            summary: "Wake a pane's pump".into(),
            author: "hunt".into(),
            time: 0,
        };
        s.receive(ServerMsg::Commits { request_id: *request_id, checkout: CheckoutId(100), commits: vec![commit] });
        let sent = code_asks(s.history_action(Action::OpenCode, None));
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::ListCode { version: CodeVersion::Commit(oid), .. }] if *oid == "a".repeat(40)
        ));
        assert!(s.stages.now() == Stage::Code);
        assert_eq!(s.code.as_ref().unwrap().page(None).back.as_deref(), Some("history"));
        s.code_action(Action::GoBack);
        assert!(s.stages.now() == Stage::Checkouts);
        assert!(s.dialogs.history_open(), "the history is back as it was");
    }

    #[test]
    fn the_code_stage_opens_on_the_checkout_picked_in_checkouts_and_keeps_it() {
        let mut s = session(Hello::this_build());
        s.show_checkouts(true);
        let sent = code_asks(s.show_code(true));
        assert!(matches!(sent.as_slice(), [ClientMsg::ListCode { checkout: CheckoutId(100), .. }]));
        s.show_features(false);
        assert!(code_asks(s.show_code(true)).is_empty(), "left as it was, not read again");
        assert!(s.stages.now() == Stage::Code);
    }

    #[test]
    fn the_editor_opens_as_the_settings_say() {
        let mut s = session(without_focus());
        s.set(SettingChange::Editor { mode: crate::settings::EditorMode::Separately });
        s.set(SettingChange::EditorCommand { command: "code -w".into() });
        let sent = s.open_review(CheckoutId(100));
        let [ClientMsg::Review { request_id, .. }, ..] = sent.as_slice() else {
            panic!("{:?}", names(&sent));
        };
        s.receive(reviewed(argus_protocol::ReviewBase::Unstaged, *request_id));
        s.review_action(Action::Next);
        let sent = s.review_action(Action::OpenEditor);
        let [ClientMsg::OpenInEditor { external: true, command: Some(command), .. }] = sent.as_slice() else {
            panic!("{:?}", names(&sent));
        };
        assert_eq!(command, "code -w");
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
        assert!(s.take_frames().is_empty());
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
    fn a_pane_opened_beside_joins_the_grid_and_takes_the_keys() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.resize(Side::Workspace, 30, 100);
        assert_eq!(names(&s.open_beside(PaneId(2))), ["subscribe 2", "resize 2 30x100"]);
        assert_eq!(s.shown.pane(), Some(PaneId(2)));
        let row = s.rail(0).row;
        assert_eq!((row.grid, row.keyed, row.beside.len()), (vec![vec![0, 1]], Some(0), 1));
        assert_eq!(row.widths, [[500, 500]]);
    }

    #[test]
    fn opening_beside_with_nothing_in_the_workspace_or_no_room_says_why_and_changes_nothing() {
        let mut s = session(without_focus());
        s.open_beside(PaneId(1));
        assert!(notices(&mut s).iter().any(|n| n.contains("Workspace first")));
        s.select(PaneId(1));
        s.resize(Side::Workspace, 30, 30);
        s.open_beside(PaneId(2));
        assert!(notices(&mut s).iter().any(|n| n.contains("20 columns by 6 rows")), "30 columns do not make two of 20");
        assert_eq!(s.shown.panes(), [PaneId(1)]);
    }

    #[test]
    fn a_pane_the_grid_refuses_leaves_the_stage_it_was_on() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.resize(Side::Workspace, 30, 30);
        s.show_features(true);
        s.open_beside(PaneId(2));
        assert!(notices(&mut s).iter().any(|n| n.contains("20 columns")));
        assert_eq!((s.stages.now(), s.shown.panes()), (Stage::Feature, vec![PaneId(1)]));
    }

    #[test]
    fn closing_a_split_lets_the_pane_go_without_killing_it_and_the_keys_return() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.open_beside(PaneId(2));
        let sent = s.close_beside(Side::Beside(0));
        assert_eq!(names(&sent), ["unsubscribe 2"]);
        assert_eq!((s.shown.pane(), s.rail(0).row.beside.len()), (Some(PaneId(1)), 0));
    }

    #[test]
    fn the_split_key_takes_the_keyed_split_out_and_says_so_for_the_workspaces_pane() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.open_beside(PaneId(2));
        assert_eq!(names(&s.command(Action::CloseSplit)), ["unsubscribe 2"]);
        assert_eq!((s.shown.pane(), s.shown.panes()), (Some(PaneId(1)), vec![PaneId(1)]));
        assert!(s.command(Action::CloseSplit).is_empty(), "the Workspace's own pane stays");
        assert!(notices(&mut s).iter().any(|n| n.contains("not split beside another")));
        assert_eq!(s.shown.panes(), [PaneId(1)]);
    }

    #[test]
    fn selecting_a_pane_already_in_the_grid_only_gives_it_the_keys() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.open_beside(PaneId(2));
        s.select(PaneId(1));
        assert_eq!((s.shown.pane(), s.shown.panes()), (Some(PaneId(1)), vec![PaneId(1), PaneId(2)]));
    }

    #[test]
    fn a_pane_dragged_from_the_rail_onto_an_empty_workspace_is_shown_there() {
        let mut s = session(without_focus());
        assert_eq!(names(&s.drop_pane(PaneId(1), None, Zone::Centre)), ["subscribe 1"]);
        assert_eq!((s.shown.pane(), s.shown.panes()), (Some(PaneId(1)), vec![PaneId(1)]));
    }

    #[test]
    fn a_pane_dragged_from_the_rail_lands_where_it_was_dropped() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.drop_pane(PaneId(2), Some(PaneId(1)), Zone::Top);
        assert_eq!(s.rail(0).row.grid, [vec![1], vec![0]], "above the pane it was dropped on");
        assert_eq!(s.shown.pane(), Some(PaneId(2)), "with the keys");
    }

    #[test]
    fn a_pane_dragged_from_the_rail_onto_no_pane_joins_by_the_grid_rule() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.drop_pane(PaneId(2), None, Zone::Centre);
        assert_eq!(s.rail(0).row.grid, [vec![0, 1]]);
    }

    #[test]
    fn dropping_a_pane_on_another_rearranges_the_grid_the_rail_reports() {
        let mut s = session(without_focus());
        s.select(PaneId(1));
        s.open_beside(PaneId(2));
        s.move_pane(PaneId(2), PaneId(1), Zone::Top);
        assert_eq!(s.rail(0).row.grid, [vec![1], vec![0]]);
        s.move_held(Dir::Down);
        assert_eq!(s.rail(0).row.grid, [vec![0], vec![1]]);
    }

    #[test]
    fn the_page_hears_each_change_once() {
        let mut s = session(without_focus());
        let first = s.events(0);
        assert!(
            matches!(
                first.as_slice(),
                [
                    Event::Status(_),
                    Event::Settings(_),
                    Event::Docks(_),
                    Event::Console(_),
                    Event::Rail(_),
                    Event::Features(_)
                ]
            ),
            "{first:?}"
        );
        assert!(s.events(0).is_empty());
        s.toggle_repository(RepositoryId(10));
        assert!(matches!(s.events(0).as_slice(), [Event::Rail(_)]));
        assert_eq!(s.everything(0).len(), 8, "a fresh page hears it all, the keymap first");
    }

    /// Each surface forgets what it sent when the page reloads: one that
    /// did not would leave its open dialog missing from the fresh page.
    #[test]
    fn a_reloaded_page_hears_each_open_dialog_again() {
        fn on_feature(s: &mut Session) {
            s.show_features(true);
            s.receive(ServerMsg::Decisions(Box::new(crate::fixtures::board())));
            s.call(Call::OpenFeature { slug: Some("trim".into()) });
        }
        type Case = (&'static str, Box<dyn Fn(&mut Session)>, fn(&Event) -> bool);
        let cases: Vec<Case> = vec![
            (
                "capture",
                Box::new(|s| {
                    on_feature(s);
                    s.call(Call::Capture { gesture: CaptureGesture::Open });
                }),
                |e| matches!(e, Event::Capture(p) if p.open),
            ),
            (
                "link",
                Box::new(|s| {
                    on_feature(s);
                    s.call(Call::OpenLink { map: false });
                }),
                |e| matches!(e, Event::Link(p) if p.open),
            ),
            (
                "reconcile",
                Box::new(|s| {
                    on_feature(s);
                    s.call(Call::OpenReconcile);
                }),
                |e| matches!(e, Event::Reconcile(p) if p.open),
            ),
            (
                "sessions",
                Box::new(|s| {
                    s.call(Call::OpenSessions { repository: 10 });
                }),
                |e| matches!(e, Event::Sessions(p) if p.open),
            ),
            (
                "add project",
                Box::new(|s| {
                    s.call(Call::OpenAddProject);
                }),
                |e| matches!(e, Event::AddProject(p) if p.open),
            ),
            (
                "review",
                Box::new(|s| {
                    s.call(Call::OpenReview { checkout: 100 });
                }),
                |e| matches!(e, Event::Review(p) if p.open),
            ),
            (
                "history",
                Box::new(|s| {
                    s.call(Call::OpenHistory { checkout: 100 });
                }),
                |e| matches!(e, Event::History(p) if p.open),
            ),
            (
                "extension dialog",
                Box::new(|s| {
                    let dialog = argus_protocol::ExtDialog {
                        title: "Restart".into(),
                        kind: argus_protocol::ExtDialogKind::Confirm { message: "sure?".into(), yes: None },
                        answers: None,
                    };
                    s.receive(ServerMsg::ExtensionDialog { extension: "docker".into(), id: 9, dialog, prompted: false });
                }),
                |e| matches!(e, Event::ExtensionDialog(p) if p.open),
            ),
        ];
        for (name, open, drawn) in cases {
            let mut s = session(Hello::this_build());
            open(&mut s);
            assert!(s.events(0).iter().any(drawn), "the {name} opens");
            assert!(!s.events(0).iter().any(drawn), "the {name} is heard once");
            assert!(s.everything(0).iter().any(drawn), "the {name} is drawn again on a reloaded page");
        }
    }

    #[test]
    fn a_reloaded_page_hears_the_console_the_docks_and_the_checkouts_table_again() {
        let mut s = session(Hello::this_build());
        s.show_checkouts(true);
        let drawn = |events: &[Event]| {
            let has = |f: fn(&Event) -> bool| events.iter().any(f);
            [
                has(|e| matches!(e, Event::Console(_))),
                has(|e| matches!(e, Event::Docks(_))),
                has(|e| matches!(e, Event::Checkouts(_))),
            ]
        };
        assert_eq!(drawn(&s.events(0)), [true; 3]);
        assert_eq!(drawn(&s.events(0)), [false; 3], "each is heard once");
        assert_eq!(drawn(&s.everything(0)), [true; 3], "a reloaded page is sent all three");
    }

    #[test]
    fn the_rail_is_sent_again_only_when_it_differs() {
        let mut s = session(without_focus());
        let mut next = tree();
        next[0].repositories.push(repository(11, "dotfiles", Some(0), vec![checkout(110, "main", true, vec![])]));
        s.receive(with_tree(next.clone()));
        let used = |events: &[Event]| {
            events.iter().find_map(|e| match e {
                Event::Rail(rail) => Some(rail.project.as_ref()?.repositories.iter().find(|r| r.id == 11)?.used.clone()),
                _ => None,
            })
        };
        assert_eq!(used(&s.events(60)), Some(Some("1m".into())));
        s.receive(with_tree(next));
        assert_eq!(used(&s.events(90)), None, "the same tree, and the same minute, is not sent again");
        assert_eq!(used(&s.events(7_200)), Some(Some("2h".into())), "but an age that moved on is");
    }

    #[test]
    fn a_window_asks_for_views_only_of_a_daemon_that_has_them_and_draws_what_comes() {
        let mut quiet = Session::default();
        let sent = quiet.connected(Some(without_focus()), None, vec![with_tree(tree())]);
        assert!(!sent.iter().any(|m| matches!(m, ClientMsg::WatchViews)));

        let mut s = Session::default();
        let sent = s.connected(Some(Hello::this_build()), None, vec![with_tree(tree())]);
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::WatchViews)));
        s.events(0);
        let view = argus_protocol::AgentView {
            id: "ci".into(),
            title: "CI on main".into(),
            ..Default::default()
        };
        s.receive(ServerMsg::ViewShown(Box::new(argus_protocol::ShownView { pane: PaneId(1), at: 5, subject: None, view })));
        let events = s.events(0);
        let Some(Event::Views(page)) = events.iter().find(|e| matches!(e, Event::Views(_))) else {
            panic!("{events:?}");
        };
        assert_eq!((page.views[0].from.as_str(), page.views[0].view.id.as_str()), ("claude", "ci"));
        let asked = s.call(Call::RefreshView { pane: 1, id: "ci".into() });
        assert!(matches!(&asked[..], [ClientMsg::RefreshView { pane: PaneId(1), id }] if id == "ci"));
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
    fn an_editor_with_a_window_of_its_own_is_not_opened_on_a_remote_host() {
        let mut s = session(without_focus());
        s.set(SettingChange::Editor { mode: crate::settings::EditorMode::Separately });
        s.switch_host(Some("devbox".into()));
        s.open_in_editor(CheckoutId(100), "src/lib.rs".into(), None);
        let sent = std::mem::take(&mut s.out);
        assert!(!sent.iter().any(|m| matches!(m, ClientMsg::OpenInEditor { .. })), "{sent:?}");
        let said = s.events(0).into_iter().any(|e| matches!(e, Event::Notice { text } if text.contains("devbox")));
        assert!(said, "the person is told why");

        s.switch_host(None);
        s.open_in_editor(CheckoutId(100), "src/lib.rs".into(), None);
        assert!(s.out.iter().any(|m| matches!(m, ClientMsg::OpenInEditor { external: true, .. })));
    }

    #[test]
    fn switching_host_forgets_the_last_daemon_s_tree_and_asks_to_be_saved() {
        let mut s = session(without_focus());
        s.take_settings();
        s.switch_host(Some("devbox".into()));
        let events = s.events(0);
        let status = events.iter().find_map(|e| match e {
            Event::Status(status) => Some(status),
            _ => None,
        });
        let status = status.expect("the page hears the new host");
        assert_eq!(status.host.as_deref(), Some("devbox"));
        assert_eq!(status.connection, crate::page::Connection::Connecting);
        let rail = events.iter().find_map(|e| match e {
            Event::Rail(rail) => Some(rail),
            _ => None,
        });
        assert!(rail.expect("the rail is sent again").project.is_none(), "nothing of the last daemon's tree stays");
        assert_eq!(s.take_settings().and_then(|s| s.host).as_deref(), Some("devbox"));
        assert!(s.take_settings().is_none(), "saved once");

        s.switch_host(Some("devbox".into()));
        assert!(s.take_settings().is_none(), "the same host changes nothing");
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
    fn an_agent_needing_you_raises_a_notification_while_the_window_is_away() {
        let mut s = session(without_focus());
        s.events(0);
        s.receive(with_status(0, PaneStatus::Waiting));
        assert!(s.take_alerts().is_empty(), "the window is in front");
        s.receive(with_status(0, PaneStatus::Working));
        s.focus(false);
        s.receive(with_status(0, PaneStatus::Waiting));
        let alerts = s.take_alerts();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].title, "claude");
        assert!(s.take_alerts().is_empty(), "raised once");
    }

    #[test]
    fn a_setting_is_heard_saved_and_followed() {
        let mut s = session(without_focus());
        s.events(0);
        s.set(SettingChange::FeatureLayout { layout: Layout::Board });
        let events = s.events(0);
        let Some(Event::Settings(page)) = events.iter().find(|e| matches!(e, Event::Settings(_))) else {
            panic!("{events:?}");
        };
        assert_eq!(page.chosen.feature_layout, Layout::Board);
        assert!(events.iter().any(|e| matches!(e, Event::Features(f) if f.layout == "board")), "the index follows");
        let saved = s.take_settings().expect("saved");
        assert_eq!(saved.preferences.feature_layout, Layout::Board);
        assert!(s.take_settings().is_none(), "saved once");
        s.set(SettingChange::Theme { theme: Theme::Latte });
        assert_eq!(s.look().0, Theme::Latte);
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

    fn capture_page(s: &mut Session) -> Option<crate::capture_note::CapturePage> {
        s.events(0).into_iter().find_map(|e| match e {
            Event::Capture(page) => Some(*page),
            _ => None,
        })
    }

    /// The note stays up after Create until the pushed board holds the
    /// feature it became, which then opens; a refusal leaves it to fix.
    #[test]
    fn a_note_becomes_the_feature_the_board_says_it_made_and_opens_it() {
        let mut s = session(without_focus());
        s.run_command(Action::NewFeature);
        s.finish();
        s.receive(ServerMsg::Decisions(Box::new(crate::fixtures::board())));
        let page = capture_page(&mut s).expect("the note opens");
        assert!(page.open && s.stages.now() == Stage::Feature);

        let note = "Bring the readers back\n- Restore them\n? Where is the transcript\n  - a key\n";
        s.call(Call::Capture { gesture: CaptureGesture::Note { note: note.into() } });
        let page = capture_page(&mut s).unwrap();
        assert_eq!((page.title.as_str(), page.tasks.len(), page.blocked), ("Bring the readers back", 1, None));

        let sent = s.call(Call::Capture { gesture: CaptureGesture::Create { note: note.into() } });
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::Feature { action: argus_protocol::FeatureAction::Capture { .. }, .. })));
        assert!(capture_page(&mut s).unwrap().creating);
        s.receive(ServerMsg::Error { message: "a feature with that title exists".into() });
        assert!(!capture_page(&mut s).unwrap().creating, "refused: still open, to send again");

        s.call(Call::Capture { gesture: CaptureGesture::Create { note: note.into() } });
        let mut board = crate::fixtures::board();
        board.features.push(argus_protocol::Feature {
            number: 11,
            slug: "readers".into(),
            title: "Bring the readers back".into(),
            ..argus_protocol::Feature::default()
        });
        s.receive(ServerMsg::Decisions(Box::new(board)));
        assert!(!capture_page(&mut s).unwrap().open, "the note is done with");
        assert_eq!(s.features.opened(), Some("readers"));
    }

    #[test]
    fn the_feature_filter_is_held_in_rust_and_echoed_to_the_page() {
        let mut s = session(without_focus());
        s.show_features(true);
        s.receive(ServerMsg::Decisions(Box::new(crate::fixtures::board())));
        s.events(0);
        s.call(Call::FilterFeatures { filter: "#8".into() });
        let Some(Event::Features(index)) = s.events(0).into_iter().find(|e| matches!(e, Event::Features(_))) else {
            panic!("the index is drawn again");
        };
        let rows: Vec<&str> = index.rows.iter().map(|row| row.slug.as_str()).collect();
        assert_eq!((index.filter.as_str(), rows), ("#8", vec!["bump"]));
    }

    #[test]
    fn a_feature_key_moves_the_peek_and_asks_for_its_lists() {
        let mut s = session(without_focus());
        s.show_features(true);
        s.receive(ServerMsg::Decisions(Box::new(crate::fixtures::board())));
        let sent = s.call(Call::FeatureAction { action: Action::Next });
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::Task { feature, .. } if feature == "bump")), "{:?}", names(&sent));
        s.events(0);
        s.call(Call::FeatureAction { action: Action::Folded });
        let folded = s.events(0).into_iter().find_map(|e| match e {
            Event::Features(f) => Some(f.show_folded),
            _ => None,
        });
        assert_eq!(folded, Some(true), "as the palette's command does");
    }

    #[test]
    fn an_edit_to_an_open_feature_is_sent_and_a_refused_one_is_said() {
        let mut s = session(without_focus());
        s.show_features(true);
        s.receive(ServerMsg::Decisions(Box::new(crate::fixtures::board())));
        s.call(Call::OpenFeature { slug: Some("trim".into()) });
        let sent = s.call(Call::EditFeature { edit: FeatureEdit::Withdraw { id: 1, withdrawn: true } });
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::ChangeDecision { .. })));
        s.events(0);
        let sent = s.call(Call::EditFeature { edit: FeatureEdit::Take { id: 1, option: 0 } });
        assert!(sent.is_empty(), "D1 is not open");
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { .. })));
    }

    fn link_page(s: &mut Session) -> Option<LinkPage> {
        s.events(0).into_iter().find_map(|e| match e {
            Event::Link(page) => Some(*page),
            _ => None,
        })
    }

    /// The palette's Link a ticket opens the dialog on the open feature;
    /// a link sent shows in its list only once the pushed board holds it,
    /// and the dialog goes with the feature.
    #[test]
    fn the_link_dialog_follows_the_board_and_closes_with_its_feature() {
        let mut s = session(without_focus());
        s.show_features(true);
        s.receive(ServerMsg::Decisions(Box::new(crate::fixtures::board())));
        s.call(Call::Command { action: Action::LinkTicket });
        assert!(link_page(&mut s).is_none(), "no feature is open");
        s.call(Call::OpenFeature { slug: Some("trim".into()) });
        s.call(Call::Command { action: Action::LinkTicket });
        let page = link_page(&mut s).expect("the dialog opens");
        assert_eq!((page.number, page.linked.len()), (7, 1));

        let url = "https://acme.atlassian.net/browse/ARG-142";
        let gesture = LinkGesture::Link { url: url.into(), role: "the PM's ticket".into() };
        let sent = s.call(Call::Link { gesture });
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::Feature { action: argus_protocol::FeatureAction::Link { .. }, .. })));
        assert_eq!(link_page(&mut s).unwrap().linked.len(), 1, "not linked until the board says so");

        let mut board = crate::fixtures::board();
        let trim = board.features.iter_mut().find(|f| f.slug == "trim").unwrap();
        trim.links.push(argus_protocol::TrackerLink { tracker: "Jira".into(), key: "ARG-142".into(), url: url.into(), role: "the PM's ticket".into() });
        s.receive(ServerMsg::Decisions(Box::new(board.clone())));
        let page = link_page(&mut s).unwrap();
        assert_eq!((page.linked.len(), page.url.as_str()), (2, ""));

        board.features.retain(|f| f.slug != "trim");
        s.receive(ServerMsg::Decisions(Box::new(board)));
        assert!(!link_page(&mut s).unwrap().open, "the feature went, and the dialog with it");
    }

    /// A drag on the diagram sends the flow rewritten, and the Flows lens
    /// draws the new order only once the daemon pushes the list back.
    #[test]
    fn a_dragged_step_shows_when_the_pushed_flows_say_so() {
        let mut s = session(without_focus());
        s.show_features(true);
        s.receive(ServerMsg::Decisions(Box::new(crate::fixtures::board())));
        s.call(Call::OpenFeature { slug: Some("trim".into()) });
        s.call(Call::SetLens { lens: crate::page::Lens::Flows });
        s.receive(ServerMsg::SequenceDiagrams(Box::new(crate::fixtures::diagrams("trim"))));
        let first = |events: &[Event]| {
            events.iter().find_map(|e| match e {
                Event::Features(f) => f.open.as_ref()?.flows.as_ref()?.flow.as_ref()?.rows.first().map(|r| r.text.clone()),
                _ => None,
            })
        };
        assert_eq!(first(&s.events(0)).as_deref(), Some("ask for the brief"));

        let sent = s.call(Call::EditFeature { edit: FeatureEdit::MoveStep { diagram: 11, step: 1, to: 2 } });
        let Some(ClientMsg::SequenceDiagram { action: argus_protocol::DiagramAction::Rewrite { write, .. }, .. }) = sent.last() else {
            panic!("{:?}", names(&sent));
        };
        assert_eq!(first(&s.events(0)), None, "nothing changed here yet");
        let mut pushed = crate::fixtures::diagrams("trim");
        pushed.diagrams[0].body = write.body.clone();
        s.receive(ServerMsg::SequenceDiagrams(Box::new(pushed)));
        assert_eq!(first(&s.events(0)).as_deref(), Some("brief request"));
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
        assert!(s.stages.now() != Stage::Feature);
        s.show_features(true);
        s.select(PaneId(99));
        assert!(s.stages.now() == Stage::Feature, "a pane that is not there changes nothing");
    }

    #[test]
    fn going_to_a_features_pane_leaves_the_stage_for_it() {
        let mut s = session(without_focus());
        s.show_features(true);
        assert_eq!(names(&s.go_to_pane(PaneId(1))), ["subscribe 1"]);
        assert!(s.stages.now() != Stage::Feature);
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
            keyboard: argus_protocol::KeyboardMode::default(),
        });
        s
    }

    fn input_bytes(msgs: &[ClientMsg]) -> Vec<u8> {
        msgs.iter()
            .filter_map(|m| match m {
                ClientMsg::Input { bytes, .. } => Some(bytes.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn rows_with_keyboard(flags: u8) -> ServerMsg {
        ServerMsg::PaneRows {
            pane: PaneId(2),
            rows: 5,
            cols: 10,
            runs: Vec::new(),
            cursor: argus_protocol::Cursor::default(),
            mouse: argus_protocol::MouseTracking::default(),
            alternate_screen: false,
            keyboard: argus_protocol::KeyboardMode(flags),
        }
    }

    #[test]
    fn shift_enter_is_distinct_once_the_child_pushes_kitty_flags() {
        let shift_enter = PageKey { key: "Enter".into(), shift: true, ..PageKey::default() };
        let mut s = showing(5, false);
        assert_eq!(input_bytes(&s.key(&shift_enter)), b"\r", "legacy until the child asks");

        s.receive(rows_with_keyboard(1));
        assert_eq!(input_bytes(&s.key(&shift_enter)), b"\x1b[13;2u");
        assert_eq!(input_bytes(&s.key(&press("Enter"))), b"\r", "a plain Enter still submits");

        s.receive(rows_with_keyboard(0));
        assert_eq!(input_bytes(&s.key(&shift_enter)), b"\r", "a pop returns to legacy");
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
        assert_eq!(scrollback(&s.wheel(PaneId(2), -1, (0, 0))), [(3, None)]);
        s.receive(history(3, 50, Some(100)));
        assert_eq!(scrollback(&s.wheel(PaneId(2), -1, (0, 0))), [(6, Some(97))]);
        assert_eq!(scrollback(&s.wheel(PaneId(2), 1, (0, 0))), [(3, Some(100))]);
    }

    #[test]
    fn a_resize_re_reads_the_parked_rows_at_the_new_width() {
        let mut s = showing(5, false);
        s.wheel(PaneId(2), -1, (0, 0));
        s.receive(history(3, 50, Some(100)));
        let resized = s.receive(ServerMsg::PaneRows {
            pane: PaneId(2),
            rows: 8,
            cols: 20,
            runs: Vec::new(),
            cursor: argus_protocol::Cursor::default(),
            mouse: argus_protocol::MouseTracking::default(),
            alternate_screen: false,
            keyboard: Default::default(),
        });
        assert_eq!(scrollback(&resized), [(3, Some(100))]);
    }

    #[test]
    fn typing_or_pasting_returns_the_view_to_the_live_screen() {
        let mut s = showing(5, false);
        s.wheel(PaneId(2), -1, (0, 0));
        s.key(&PageKey { key: "a".into(), ..PageKey::default() });
        assert!(s.shown.view().unwrap().parked().is_none());

        s.wheel(PaneId(2), -1, (0, 0));
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
        assert!(s.stages.now() == Stage::Feature);
        s.key(&ctrl_space());
        s.key(&press("1"));
        assert!(s.stages.now() != Stage::Feature);
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
        assert_eq!(names(&s.wheel(PaneId(2), -2, (0, 0))), ["input 2 [27, 91, 65, 27, 91, 65]"]);
        assert_eq!(names(&s.wheel(PaneId(2), 1, (0, 0))), ["input 2 [27, 91, 66]"]);
    }

    #[test]
    fn a_history_with_nothing_behind_the_screen_leaves_it_live_and_says_so() {
        let mut s = showing(5, false);
        s.events(0);
        s.wheel(PaneId(2), -1, (0, 0));
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
            keyboard: Default::default(),
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
        assert_eq!(s.rail(0).templates, ["claude", "codex"], "a refused agent is not the last one started");
        assert!(s.take_settings().is_none());
    }

    #[test]
    fn the_checkouts_stage_opens_alone_and_asks_for_the_board_it_names_features_from() {
        let mut s = session(without_focus());
        s.go(Stage::Feature);
        let sent = s.show_checkouts(true);
        assert!(s.stages.now() == Stage::Checkouts, "one stage at a time");
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
        assert!(s.stages.now() != Stage::Checkouts);
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
        assert_eq!(asked, Some(crate::checkouts_stage::Confirm::Force { branch: "spike".into(), commits: Vec::new() }));
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
        assert!(s.stages.now() == Stage::Checkouts);
        assert_eq!(s.checkouts.selected_key(), Some("c101"));
        let asked = s.events(0).into_iter().find_map(|e| match e {
            Event::Checkouts(table) => table.confirm,
            _ => None,
        });
        assert!(matches!(asked, Some(crate::checkouts_stage::Confirm::Remove { name, .. }) if name == "spike"));
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
    const CALLS: [&str; 140] = [
        r#"{"call":"select","pane":1}"#,
        r#"{"call":"focus_view","pane":1}"#,
        r#"{"call":"open_beside","pane":2}"#,
        r#"{"call":"close_beside","side":{"beside":0}}"#,
        r#"{"call":"set_shares","heights":[1000],"widths":[[500,500]]}"#,
        r#"{"call":"even_shares","row":null}"#,
        r#"{"call":"step_row","by":1}"#,
        r#"{"call":"move_pane","pane":2,"onto":1,"zone":"left"}"#,
        r#"{"call":"drop_pane","pane":2,"onto":1,"zone":"top"}"#,
        r#"{"call":"move_held","dir":"down"}"#,
        r#"{"call":"focus_towards","dir":"up"}"#,
        r#"{"call":"resize","side":"first","rows":30,"cols":100}"#,
        r#"{"call":"key","key":{"key":"a","code":"KeyA","ctrl":false,"alt":false,"shift":false,"meta":false}}"#,
        r#"{"call":"text","text":"é"}"#,
        r#"{"call":"paste","text":"ls"}"#,
        r#"{"call":"paste_clipboard"}"#,
        r#"{"call":"wheel","pane":1,"notches":-1,"at":[0,0]}"#,
        r#"{"call":"mouse","gesture":"down","button":0,"at":[1,3]}"#,
        r#"{"call":"copy","from":[0,0],"to":[0,4]}"#,
        r#"{"call":"spawn_shell","checkout":100}"#,
        r#"{"call":"spawn_agent","checkout":100,"template":"claude"}"#,
        r#"{"call":"close","pane":2}"#,
        r#"{"call":"focus","focused":false}"#,
        r#"{"call":"select_project","project":2}"#,
        r#"{"call":"open_workspace","workspace":3}"#,
        r#"{"call":"remove_project","project":2}"#,
        r#"{"call":"answer_remove_project","yes":false}"#,
        r#"{"call":"open_add_project"}"#,
        r#"{"call":"add_project","gesture":{"on":"fields","name":"work","skip":"vendor","folder":""}}"#,
        r#"{"call":"add_project","gesture":{"on":"close"}}"#,
        r#"{"call":"toggle_repository","repository":10}"#,
        r#"{"call":"toggle_checkout","checkout":100}"#,
        r#"{"call":"choose_repository","repository":10}"#,
        r#"{"call":"show_features","open":true}"#,
        r#"{"call":"show_checkouts","open":true}"#,
        r#"{"call":"show_code","open":true}"#,
        r#"{"call":"code_click","target":{"on":"tree","path":"src"}}"#,
        r#"{"call":"code_action","action":"next"}"#,
        r#"{"call":"code_jump","gesture":{"on":"query","text":"br"}}"#,
        r#"{"call":"code_search","gesture":{"on":"choose"}}"#,
        r#"{"call":"code_comment","body":null}"#,
        r#"{"call":"code_search_glob","glob":"src/**"}"#,
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
        r#"{"call":"find_in_transcript","find":"notify"}"#,
        r#"{"call":"filter_sessions","harness":"claude","checkout":null}"#,
        r#"{"call":"peek","slug":"trim"}"#,
        r#"{"call":"set_layout","layout":"board"}"#,
        r##"{"call":"filter_features","filter":"#7"}"##,
        r#"{"call":"feature_action","action":"next"}"#,
        r#"{"call":"set_chime","on":false}"#,
        r#"{"call":"setting","change":{"set":"opacity","percent":80}}"#,
        r#"{"call":"hold","slug":"trim","reason":"waiting"}"#,
        r#"{"call":"archive","slug":"trim","archived":true}"#,
        r#"{"call":"capture","gesture":{"on":"open"}}"#,
        r#"{"call":"capture","gesture":{"on":"note","note":"Title - a task"}}"#,
        r#"{"call":"capture","gesture":{"on":"create","note":"Title - a task"}}"#,
        r#"{"call":"capture","gesture":{"on":"discard"}}"#,
        r#"{"call":"open_feature","slug":"trim"}"#,
        r#"{"call":"select_work","at":{"kind":"decision","id":2}}"#,
        r#"{"call":"set_lens","lens":"flows"}"#,
        r#"{"call":"select_decision","id":2}"#,
        r#"{"call":"select_step","diagram":11,"step":null}"#,
        r#"{"call":"wait","slug":"bump","on":"trim","waits":true}"#,
        r#"{"call":"edit_feature","edit":{"on":"take","id":2,"option":0}}"#,
        r#"{"call":"go_to_pane","pane":1}"#,
        r#"{"call":"palette","open":true}"#,
        r#"{"call":"command","action":"fill"}"#,
        r#"{"call":"go_to_checkout","checkout":100}"#,
        r#"{"call":"go_to_feature","slug":"trim"}"#,
        r#"{"call":"console","gesture":{"on":"tab"}}"#,
        r#"{"call":"docks","gesture":{"on":"size","dock":"bottom","size":300}}"#,
        r#"{"call":"docks","gesture":{"on":"move","panel":"console","dock":"right"}}"#,
        r#"{"call":"docks","gesture":{"on":"show","panel":"views"}}"#,
        r#"{"call":"docks","gesture":{"on":"hide","panel":"extension:docker/containers"}}"#,
        r#"{"call":"console","gesture":{"on":"close","pane":1}}"#,
        r#"{"call":"console","gesture":{"on":"split"}}"#,
        r#"{"call":"console","gesture":{"on":"closeSide","pane":1}}"#,
        r#"{"call":"open_extension_view","extension":"docker","view":"containers"}"#,
        r#"{"call":"select_extension_row","panel":"extension:docker/containers","key":"web"}"#,
        r#"{"call":"extension_action","panel":"extension:docker/containers","action":"restart"}"#,
        r#"{"call":"extension_default","panel":"extension:docker/containers","row":"web"}"#,
        r#"{"call":"extension_key","panel":"extension:docker/containers","action":"next"}"#,
        r#"{"call":"answer_extension","yes":false}"#,
        r#"{"call":"extension_command","extension":"docker","command":"up","checkout":100,"pane":null}"#,
        r#"{"call":"answer_dialog","dialog":3,"answer":{"picked":"web"}}"#,
        r#"{"call":"extension_mode","panel":"extension:docker/containers","page":true}"#,
        r#"{"call":"toggle_extensions"}"#,
        r#"{"call":"reconnect"}"#,
        r#"{"call":"set_host","host":"devbox"}"#,
        r#"{"call":"minimize"}"#,
        r#"{"call":"toggle_maximize"}"#,
        r#"{"call":"close_window"}"#,
        r#"{"call":"drag_window"}"#,
        r#"{"call":"snap_layouts"}"#,
        r#"{"call":"open_view","pane":1,"id":"ci"}"#,
        r#"{"call":"pop_out_view","pane":1,"id":"ci"}"#,
        r#"{"call":"open_ref","pane":1,"reference":{"kind":"path","path":"src/lib.rs","line":3}}"#,
        r#"{"call":"hide_views"}"#,
        r#"{"call":"toggle_views"}"#,
        r#"{"call":"rail_width","width":320}"#,
        r#"{"call":"outline_width","width":480}"#,
        r#"{"call":"brief_height","height":160}"#,
        r#"{"call":"refresh_view","pane":1,"id":"ci"}"#,
        r#"{"call":"press_view","pane":1,"press":{"view":"ci","action":"rerun","rows":[],"form":null,"answers":[]}}"#,
        r#"{"call":"close_view","pane":1,"id":"ci"}"#,
        r#"{"call":"open_saved_view","id":"plan","from":null}"#,
        r#"{"call":"open_saved_view","id":"plan","from":{"panel":"saved:plan","column":0}}"#,
        r#"{"call":"close_saved_column","panel":"saved:plan","column":1}"#,
        r#"{"call":"pop_out_saved_column","panel":"saved:plan","column":1}"#,
        r#"{"call":"float_saved_column","panel":"saved:plan","column":1}"#,
        r#"{"call":"refresh_saved_view","id":"plan"}"#,
        r#"{"call":"mark_saved_view_stale","id":"plan"}"#,
        r#"{"call":"delete_saved_view","id":"plan"}"#,
        r#"{"call":"press_saved_view","id":"plan","press":{"view":"plan","action":"rerun","rows":[],"form":null,"answers":[]}}"#,
        r#"{"call":"open_link","map":false}"#,
        r#"{"call":"link","gesture":{"on":"fields","url":"https://linear.app/a/issue/ENG-1/x","role":"the work"}}"#,
        r#"{"call":"open_reconcile"}"#,
        r#"{"call":"reconcile","gesture":{"on":"action","action":"tick"}}"#,
        r#"{"call":"open_url","url":"https://example.com"}"#,
        r#"{"call":"copy_text","text":"ab"}"#,
        r#"{"call":"rename_pane","pane":1,"title":"the pty deadlock"}"#,
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
        assert!(s.stages.now() == Stage::Checkouts);
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
            ended: Some("2026-10-14T13:52:00Z".into()),
            modified: None,
            turns: Some(48),
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
        s.receive(ServerMsg::Resumes { request_id: resumes, resumes: vec![argus_protocol::sessions::Resume { harness: "claude".into(), argv }] });
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

    fn sessions_page(s: &mut Session) -> Option<crate::sessions_dialog::SessionsPage> {
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
        assert!(s.recent.get().is_none(), "the dialog gave way to the pane");
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
        assert!(s.recent.get().is_some(), "nothing is predicted");
        s.receive(ServerMsg::Created { request_id: *request_id, created: Some(Created::Pane(PaneId(9))) });
        assert!(s.recent.get().is_some() && s.shown.pane().is_none(), "made, but not in a tree yet");
        let mut grown = tree();
        grown[0].repositories[0].checkouts[0].panes.push(pane(9, PaneKind::Agent, "claude", PaneStatus::Working));
        s.receive(with_tree(grown));
        assert_eq!(s.shown.pane(), Some(PaneId(9)));
        assert!(s.recent.get().is_none());
    }

    #[test]
    fn a_resumed_pane_says_what_it_was_resumed_from_once_the_tree_holds_it() {
        let mut s = listing_sessions();
        s.call(Call::SessionsAction { action: Action::Next, row: None });
        let sent = s.call(Call::SessionsAction { action: Action::Resume, row: None });
        let [ClientMsg::ResumeSession { request_id, .. }] = sent.as_slice() else {
            panic!("{sent:?}");
        };
        s.receive(ServerMsg::Created { request_id: *request_id, created: Some(Created::Pane(PaneId(9))) });
        let mut grown = tree();
        grown[0].repositories[0].checkouts[0].panes.push(pane(9, PaneKind::Agent, "claude", PaneStatus::Working));
        s.receive(with_tree(grown.clone()));
        let shown = s.rail(0).shown.expect("the new pane is on screen");
        let from = page::Resumed { at: Some("2026-10-14T13:52:00Z".into()), turns: Some(48) };
        assert_eq!((shown.pane, shown.resumed), (9, Some(from)));
        assert!(
            s.rail(0).project.unwrap().repositories[0].checkouts[0].panes.iter().any(|p| p.id == 9 && p.on),
            "under its checkout, selected"
        );

        // Another pane says nothing of it, and once the pane is gone so is
        // what it was resumed from.
        s.select(PaneId(1));
        assert_eq!(s.rail(0).shown.unwrap().resumed, None);
        grown[0].repositories[0].checkouts[0].panes.retain(|p| p.id != PaneId(9));
        s.receive(with_tree(grown));
        assert_eq!(s.sessions.resumed_from(PaneId(9)), None);
    }

    #[test]
    fn the_full_transcript_holds_the_keys_and_escape_gives_the_list_back() {
        let mut s = listing_sessions();
        let sent = s.call(Call::SessionsAction { action: Action::Next, row: None });
        let [ClientMsg::ReadSession { request_id, .. }] = sent.as_slice() else {
            panic!("{sent:?}");
        };
        let entries = vec![
            argus_protocol::transcript::Entry::User { text: "Fix the build".into(), at: None },
            argus_protocol::transcript::Entry::Agent { text: "Fixed.".into(), at: None },
        ];
        let transcript = argus_protocol::transcript::Transcript { entries, ..Default::default() };
        s.receive(ServerMsg::SessionTranscript {
            request_id: *request_id,
            session_id: "ended".into(),
            conversation: Box::new(argus_protocol::transcript::Conversation::Read(transcript)),
        });
        s.call(Call::SessionsAction { action: Action::Transcript, row: None });
        let reader = sessions_page(&mut s).and_then(|p| p.reader).expect("the reader is open");
        assert_eq!(reader.session.session_id, "ended");
        s.call(Call::SessionsAction { action: Action::Next, row: None });
        assert_eq!(sessions_page(&mut s).and_then(|p| p.reader).and_then(|r| r.target), Some(0), "j is the reader's");
        s.call(Call::FindInTranscript { find: "fixed".into() });
        assert_eq!(sessions_page(&mut s).and_then(|p| p.reader).map(|r| r.matches), Some(vec![1]));
        s.call(Call::SessionsAction { action: Action::CopyId, row: None });
        assert_eq!(s.take_copied().as_deref(), Some("ended"));
        assert_eq!(s.take_copied(), None, "once");
        s.call(Call::SessionsAction { action: Action::CloseLayer, row: None });
        let page = sessions_page(&mut s).expect("the list again");
        assert!(page.open && page.reader.is_none());
        s.call(Call::SessionsAction { action: Action::CloseLayer, row: None });
        assert!(s.recent.get().is_none(), "and a second Escape closes the dialog");
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
        assert!(s.recent.get().is_some() && s.shown.pane().is_none());
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

    // ---- Projects: switch, add and remove ----------------------------------

    fn add_page(s: &mut Session) -> Option<crate::add_project::AddProjectPage> {
        s.events(0).into_iter().find_map(|e| match e {
            Event::AddProject(page) => Some(*page),
            _ => None,
        })
    }

    /// A session with the add dialog open on `/home/work`, whose preview
    /// found `web`.
    fn adding_a_folder() -> Session {
        let mut s = session(Hello::this_build());
        let sent = s.call(Call::OpenAddProject);
        let Some(ClientMsg::ListDirectories { request_id, .. }) = sent.first() else {
            panic!("{sent:?}");
        };
        let sent = s.receive(ServerMsg::Directories(argus_protocol::DirListing {
            request_id: *request_id,
            path: "/home/work".into(),
            parent: Some("/home".into()),
            entries: vec![argus_protocol::DirEntry { name: "web".into(), is_repo: true }],
            error: None,
        }));
        let Some(ClientMsg::PreviewProject { request_id, .. }) = sent.first() else {
            panic!("{sent:?}");
        };
        s.receive(ServerMsg::ProjectPreview(argus_protocol::ProjectPreview {
            request_id: *request_id,
            path: "/home/work".into(),
            repositories: vec!["web".into()],
            depth: 8,
            always_skipped: vec![".git".into()],
            error: None,
        }));
        s
    }

    fn add(s: &mut Session) -> u64 {
        let sent = s.call(Call::AddProject {
            gesture: AddProjectGesture::Add { name: String::new(), skip: String::new(), folder: String::new() },
        });
        let [ClientMsg::AddProject { request_id, .. }] = sent.as_slice() else {
            panic!("{sent:?}");
        };
        *request_id
    }

    #[test]
    fn the_switcher_lists_the_daemons_workspaces_and_opens_one_through_it() {
        let mut s = session(Hello::this_build());
        let workspace = |id, name: &str, open| argus_protocol::WorkspaceInfo {
            id: WorkspaceId(id),
            name: name.into(),
            projects: 2,
            panes: 3,
            open,
        };
        s.receive(ServerMsg::Workspaces(vec![workspace(1, "personal", true), workspace(2, "work", false)]));
        let listed: Vec<_> = s.rail(0).workspaces.iter().map(|w| (w.name.clone(), w.open)).collect();
        assert_eq!(listed, [("personal".to_string(), true), ("work".to_string(), false)]);
        let sent = s.call(Call::OpenWorkspace { workspace: 2 });
        assert!(matches!(sent.as_slice(), [ClientMsg::OpenWorkspace { workspace: WorkspaceId(2) }]));
        assert!(s.rail(0).workspaces[0].open, "the daemon says when it has switched");
    }

    #[test]
    fn an_added_project_comes_on_the_rail_once_the_tree_holds_it() {
        let mut s = adding_a_folder();
        let page = add_page(&mut s).unwrap();
        assert_eq!((page.repositories.clone(), page.blocked.clone()), (vec!["web".to_string()], None));

        let request_id = add(&mut s);
        assert!(add_page(&mut s).is_some_and(|p| p.adding));
        s.receive(ServerMsg::Created { request_id, created: Some(Created::Project(ProjectId(3))) });
        assert!(s.adding.get().is_some(), "no tree holds it yet");

        let mut grown = tree();
        grown.push(project(3, "work", vec![repository(30, "web", None, vec![checkout(300, "main", true, vec![])])]));
        s.receive(with_tree(grown));
        assert!(add_page(&mut s).is_some_and(|p| !p.open), "the dialog closes");
        assert_eq!(s.rail(0).project.map(|p| p.name), Some("work".into()));
    }

    #[test]
    fn a_refused_add_keeps_the_dialog_and_says_why_there() {
        let mut s = adding_a_folder();
        let request_id = add(&mut s);
        s.receive(ServerMsg::Created { request_id, created: None });
        s.receive(ServerMsg::Error { message: "there is already a project called work".into() });
        let events = s.events(0);
        assert!(!events.iter().any(|e| matches!(e, Event::Notice { .. })), "said in the dialog, not under it");
        let page = events.into_iter().find_map(|e| match e {
            Event::AddProject(page) => Some(*page),
            _ => None,
        });
        let page = page.unwrap();
        assert!(page.open && !page.adding);
        assert_eq!(page.refusal.as_deref(), Some("there is already a project called work"));
        assert_eq!(s.rail(0).project.map(|p| p.id), Some(1), "the rail is as it was");
    }

    #[test]
    fn an_older_daemon_is_not_asked_to_add_a_project() {
        let mut s = session(without_focus());
        assert!(s.call(Call::OpenAddProject).is_empty());
        assert!(s.adding.get().is_none());
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text.contains("too old"))));
    }

    #[test]
    fn removing_a_project_asks_first_and_the_rail_waits_for_the_tree() {
        let mut s = session(Hello::this_build());
        assert!(s.call(Call::RemoveProject { project: 2 }).is_empty(), "nothing is sent before a yes");
        assert_eq!(s.rail(0).removing.map(|r| r.name), Some("work".into()));

        let sent = s.call(Call::AnswerRemoveProject { yes: true });
        assert!(matches!(sent.as_slice(), [ClientMsg::RemoveProject { project: ProjectId(2) }]));
        let rail = s.rail(0);
        assert!(rail.removing.is_none());
        assert_eq!(rail.projects.len(), 2, "still there until the daemon's tree drops it");

        s.receive(with_tree(tree().into_iter().take(1).collect()));
        assert_eq!(s.rail(0).projects.len(), 1);
    }

    // ---- The console -------------------------------------------------------

    fn consoles(ids: &[u64]) -> ServerMsg {
        ServerMsg::Consoles(ids.iter().map(|&id| pane(id, PaneKind::Shell, "pwsh", PaneStatus::Idle)).collect())
    }

    /// A session showing pane 1 on the Workspace, beside a daemon holding
    /// consoles 8 and 9.
    fn with_consoles() -> Session {
        let mut s = session(Hello::this_build());
        s.resize(Side::Workspace, 30, 100);
        s.select(PaneId(1));
        s.receive(consoles(&[8, 9]));
        s
    }

    fn console_page(s: &mut Session) -> crate::console::ConsolePage {
        s.console.reload();
        s.events(0)
            .into_iter()
            .find_map(|e| match e {
                Event::Console(page) => Some(*page),
                _ => None,
            })
            .expect("a forgotten console is sent again")
    }

    fn console(s: &mut Session, gesture: ConsoleGesture) -> Vec<String> {
        names(&s.call(Call::Console { gesture }))
    }

    #[test]
    fn the_desktop_reads_the_daemons_consoles_as_tabs() {
        let mut s = with_consoles();
        let page = console_page(&mut s);
        assert_eq!(page.place, ConsolePlace::Hidden, "hidden until asked for");
        assert_eq!(page.tabs.iter().map(|t| (t.pane, t.on)).collect::<Vec<_>>(), [(8, true), (9, false)]);
        assert_eq!(page.showing, None, "nothing on the console's screen");
        assert!(page.can_spawn);
        assert_eq!(s.rail(0).shown.map(|p| p.pane), Some(1), "a console is no project's pane");
    }

    #[test]
    fn showing_the_console_docks_it_beside_the_workspaces_pane_with_the_keys() {
        let mut s = with_consoles();
        let sent = leader_then(&mut s, "`");
        assert_eq!(names(&sent)[..1], ["subscribe 8"], "the Workspace's pane stays subscribed");
        let page = console_page(&mut s);
        assert_eq!((page.place, page.showing, page.typing), (ConsolePlace::Docked, Some(8), Some(8)));
        assert_eq!(s.stages.now(), Stage::Workspace, "docked under the stage it was on");
        assert_eq!(s.on_screen(), Some(PaneId(8)), "the keys are the console's");
        assert_eq!(s.shown.panes(), [PaneId(1), PaneId(8)], "the agent and the console both on screen");
        assert_eq!(s.rail(0).shown.map(|p| p.pane), Some(1), "the Workspace still shows its pane");

        assert_eq!(console(&mut s, ConsoleGesture::Select { pane: 9 })[..2], ["unsubscribe 8", "subscribe 9"]);
        assert_eq!(console(&mut s, ConsoleGesture::Hide)[..1], ["unsubscribe 9"]);
        assert_eq!(s.on_screen(), Some(PaneId(1)), "the keys go back to the Workspace's pane");
        let page = console_page(&mut s);
        assert_eq!((page.place, page.showing), (ConsolePlace::Hidden, None));
    }

    #[test]
    fn a_pane_chosen_on_the_rail_takes_the_keys_and_the_console_stays_docked_beside_it() {
        let mut s = with_consoles();
        console(&mut s, ConsoleGesture::Toggle);
        assert_eq!(names(&s.select(PaneId(2)))[..2], ["unsubscribe 1", "subscribe 2"]);
        let page = console_page(&mut s);
        assert_eq!((page.place, page.showing, page.typing), (ConsolePlace::Docked, Some(8), None));
        assert_eq!(typed(&mut s), ["input 2 [97]"]);

        console(&mut s, ConsoleGesture::Select { pane: 8 });
        assert_eq!(typed(&mut s), ["input 8 [97]"]);
        assert!(names(&s.select(PaneId(2))).iter().all(|m| m.starts_with("view")), "already shown: only the keys move");
        assert_eq!(s.on_screen(), Some(PaneId(2)));

        console(&mut s, ConsoleGesture::Select { pane: 8 });
        assert!(names(&leader_then(&mut s, "1")).iter().all(|m| m.starts_with("view")), "1 gives the Workspace's pane the keys");
        assert_eq!(s.on_screen(), Some(PaneId(2)));
    }

    #[test]
    fn four_opens_the_console_stage_and_another_stage_docks_or_hides_it() {
        let mut s = with_consoles();
        leader_then(&mut s, "4");
        assert_eq!(s.stages.now(), Stage::Console);
        let page = console_page(&mut s);
        assert_eq!((page.place, page.showing), (ConsolePlace::Tab, Some(8)));
        assert_eq!(s.rail(0).stage, Stage::Console, "the sidebar marks it");

        leader_then(&mut s, "2");
        assert_eq!(console_page(&mut s).place, ConsolePlace::Hidden, "never docked");
        assert_eq!(s.shown.pane(), Some(PaneId(1)), "a hidden console gives the keys back");

        console(&mut s, ConsoleGesture::Tab);
        console(&mut s, ConsoleGesture::Dock);
        assert_eq!(s.stages.now(), Stage::Workspace);
        assert_eq!(console_page(&mut s).place, ConsolePlace::Docked);
    }

    #[test]
    fn fullscreen_covers_the_stage_and_leaving_it_returns_to_where_it_was() {
        let mut s = with_consoles();
        leader_then(&mut s, "4");
        leader_then(&mut s, "z");
        assert_eq!(console_page(&mut s).place, ConsolePlace::Fullscreen);
        leader_then(&mut s, "z");
        assert_eq!(console_page(&mut s).place, ConsolePlace::Tab);

        leader_then(&mut s, "1");
        console(&mut s, ConsoleGesture::Fullscreen);
        let page = console_page(&mut s);
        assert_eq!((page.place, page.showing), (ConsolePlace::Fullscreen, Some(8)), "from hidden, the console's tab");
        console(&mut s, ConsoleGesture::Fullscreen);
        assert_eq!(console_page(&mut s).place, ConsolePlace::Docked, "out of fullscreen it is docked, not hidden again");

        console(&mut s, ConsoleGesture::Fullscreen);
        s.select(PaneId(2));
        assert_eq!(console_page(&mut s).place, ConsolePlace::Docked, "a checkout's pane is not the console's to fill");
        assert_eq!(s.on_screen(), Some(PaneId(2)));
    }

    /// The stage on the rail the page is sent next, if one is.
    fn drawn_stage(s: &mut Session) -> Option<Stage> {
        s.events(0).into_iter().find_map(|e| match e {
            Event::Rail(rail) => Some(rail.stage),
            _ => None,
        })
    }

    #[test]
    fn a_console_asked_for_over_the_composer_puts_it_away_and_redraws_the_rail() {
        // Select reaches reveal_console; Toggle reaches dock_console, which
        // leaves the composer before reveal_console could see it.
        for gesture in [ConsoleGesture::Select { pane: 8 }, ConsoleGesture::Toggle] {
            let mut s = with_consoles();
            s.show_composer(true);
            s.events(0);
            console(&mut s, gesture);
            assert_eq!(s.stages.now(), Stage::Workspace, "{gesture:?}");
            assert_eq!(drawn_stage(&mut s), Some(Stage::Workspace), "{gesture:?}");
        }
    }

    #[test]
    fn fullscreen_over_the_composer_puts_it_away() {
        let mut s = with_consoles();
        s.show_composer(true);
        s.events(0);
        console(&mut s, ConsoleGesture::Fullscreen);
        assert_eq!(s.stages.now(), Stage::Workspace);
        assert_eq!(drawn_stage(&mut s), Some(Stage::Workspace));
        assert_eq!(console_page(&mut s).place, ConsolePlace::Fullscreen);
    }

    #[test]
    fn a_checkout_pane_chosen_on_the_console_stage_leaves_it_for_the_workspace() {
        let mut s = with_consoles();
        leader_then(&mut s, "4");
        s.events(0);
        s.select(PaneId(2));
        assert_eq!(s.stages.now(), Stage::Workspace);
        assert_eq!(drawn_stage(&mut s), Some(Stage::Workspace));
        assert_eq!(console_page(&mut s).place, ConsolePlace::Hidden);
    }

    #[test]
    fn plus_asks_the_daemon_and_the_tab_appears_only_when_its_consoles_say_so() {
        let mut s = with_consoles();
        let sent = s.call(Call::Console { gesture: ConsoleGesture::Spawn });
        let [ClientMsg::SpawnConsole { request_id }] = sent.as_slice() else {
            panic!("{:?}", names(&sent));
        };
        let request_id = *request_id;
        s.receive(ServerMsg::Created { request_id, created: Some(Created::Pane(PaneId(10))) });
        assert_eq!(console_page(&mut s).tabs.len(), 2, "nothing is predicted");
        assert_eq!(s.shown.pane(), Some(PaneId(1)));

        let sent = s.receive(consoles(&[8, 9, 10]));
        assert_eq!(names(&sent)[..1], ["subscribe 10"]);
        assert_eq!(s.shown.panes(), [PaneId(1), PaneId(10)]);
        let page = console_page(&mut s);
        assert_eq!((page.place, page.showing), (ConsolePlace::Docked, Some(10)), "a new console shows itself");
    }

    #[test]
    fn showing_a_console_when_there_is_none_asks_for_one() {
        let mut s = session(Hello::this_build());
        let sent = s.call(Call::Console { gesture: ConsoleGesture::Tab });
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::SpawnConsole { .. })), "{:?}", names(&sent));
        assert!(console_page(&mut s).tabs.is_empty());
    }

    #[test]
    fn closing_a_tab_ends_that_console_and_its_keys_fall_back() {
        let mut s = with_consoles();
        console(&mut s, ConsoleGesture::Select { pane: 9 });
        assert_eq!(console(&mut s, ConsoleGesture::Close { pane: 9 }), ["Kill { pane: PaneId(9) }"]);
        assert_eq!(console(&mut s, ConsoleGesture::Close { pane: 1 }), Vec::<String>::new(), "a checkout's pane is not a tab");
        assert_eq!(s.shown.pane(), Some(PaneId(9)), "until the daemon says it ended");

        s.receive(with_tree(tree()));
        assert_eq!(s.shown.pane(), Some(PaneId(9)), "the tree never held it");
        assert_eq!(names(&s.receive(consoles(&[8])))[..1], ["subscribe 8"]);
        assert_eq!(s.shown.pane(), Some(PaneId(8)), "the tab before it, with the keys");
        assert!(names(&s.receive(consoles(&[]))).iter().all(|m| !m.starts_with("subscribe")));
        assert_eq!(s.shown.pane(), Some(PaneId(1)), "the last one gives the keys back");
    }

    fn docks(s: &mut Session, gesture: DockGesture) -> Vec<String> {
        names(&s.call(Call::Docks { gesture }))
    }

    fn docks_page(s: &mut Session) -> Option<crate::docks::DocksPage> {
        s.events(0).into_iter().find_map(|e| match e {
            Event::Docks(page) => Some(*page),
            _ => None,
        })
    }

    #[test]
    fn the_docked_console_and_its_height_are_saved() {
        let mut s = with_consoles();
        s.take_settings();
        console(&mut s, ConsoleGesture::Toggle);
        docks(&mut s, DockGesture::Size { dock: Dock::Bottom, size: 333 });
        let saved = s.take_settings().expect("saved");
        let bottom = saved.docks.as_ref().expect("kept").dock(Dock::Bottom);
        assert_eq!((bottom.open, bottom.active.clone(), bottom.size), (true, Some(Panel::Console), 333));
        console(&mut s, ConsoleGesture::Fullscreen);
        assert!(s.take_settings().is_none(), "fullscreen lasts the run");

        let mut again = Session::new(saved);
        again.connected(Some(Hello::this_build()), None, vec![with_tree(tree())]);
        assert_eq!(docks_page(&mut again).map(|d| d.bottom.size), Some(333));
        assert_eq!(console_page(&mut again).place, ConsolePlace::Docked);
    }

    #[test]
    fn the_console_moved_to_the_right_is_shown_there_with_the_keys_and_stays_there() {
        let mut s = with_consoles();
        s.events(0);
        s.take_settings();
        console(&mut s, ConsoleGesture::Tab);
        docks(&mut s, DockGesture::Move { panel: Panel::Console, dock: Dock::Right });
        let page = docks_page(&mut s).expect("the docks changed");
        assert_eq!(page.right.shown, Some(Panel::Console));
        assert_eq!(page.right.tabs.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Console"]);
        assert!(page.bottom.tabs.is_empty());
        assert_eq!(s.stages.now(), Stage::Workspace, "out of the Console stage, beside the Workspace");
        assert_eq!(console_page(&mut s).place, ConsolePlace::Docked);
        assert_eq!(sides(&mut s), (Some(8), None, Some(8)), "the console has the keys");

        // Hidden and shown again, it comes back on the right, not home.
        console(&mut s, ConsoleGesture::Hide);
        assert_eq!(console_page(&mut s).place, ConsolePlace::Hidden);
        console(&mut s, ConsoleGesture::Toggle);
        assert_eq!(s.docks.dock_of(&Panel::Console), Some(Dock::Right));
        assert_eq!(console_page(&mut s).place, ConsolePlace::Docked);

        let saved = s.take_settings().expect("saved");
        assert_eq!(Docks::new(saved.docks.unwrap()).dock_of(&Panel::Console), Some(Dock::Right), "across runs");

        // The palette moves it as the menu does, out of fullscreen too.
        console(&mut s, ConsoleGesture::Fullscreen);
        s.call(Call::Command { action: Action::DockLeft });
        assert_eq!(s.docks.dock_of(&Panel::Console), Some(Dock::Left));
        assert_eq!(console_page(&mut s).place, ConsolePlace::Docked);
    }

    #[test]
    fn a_dragged_edge_keeps_its_size_across_runs() {
        let mut s = session(without_focus());
        s.take_settings();
        s.events(0);
        s.call(Call::RailWidth { width: Some(360) });
        s.call(Call::OutlineWidth { width: Some(480) });
        s.call(Call::BriefHeight { height: Some(1) });
        let rail = s.events(0).into_iter().find_map(|e| match e {
            Event::Rail(rail) => Some(rail),
            _ => None,
        });
        assert_eq!(rail.expect("the rail is told").width, Some(360));
        let saved = s.take_settings().expect("saved");
        assert_eq!(saved.rail_width, Some(360));
        assert_eq!(saved.outline_width, Some(480));
        assert_eq!(saved.brief_height, Some(dragged_width::BRIEF.min), "a brief keeps a line or two");

        let mut again = Session::new(saved);
        again.connected(Some(without_focus()), None, vec![with_tree(tree())]);
        let rail = again.events(0).into_iter().find_map(|e| match e {
            Event::Rail(rail) => Some(rail),
            _ => None,
        });
        assert_eq!(rail.expect("drawn").width, Some(360));
    }

    // ---- The split ---------------------------------------------------------

    /// `with_consoles` with the console docked on 8 and split beside it on
    /// 9, each side's room reported.
    fn split() -> Session {
        let mut s = with_consoles();
        console(&mut s, ConsoleGesture::Toggle);
        s.resize(Side::First, 30, 50);
        s.resize(Side::Second, 30, 49);
        console(&mut s, ConsoleGesture::Split);
        s
    }

    fn sides(s: &mut Session) -> (Option<u64>, Option<u64>, Option<u64>) {
        let page = console_page(s);
        (page.showing, page.beside, page.typing)
    }

    fn typed(s: &mut Session) -> Vec<String> {
        names(&s.key(&press("a")))
    }

    #[test]
    fn a_split_shows_the_next_console_beside_the_chosen_one_with_the_keys() {
        let mut s = with_consoles();
        console(&mut s, ConsoleGesture::Toggle);
        s.resize(Side::Second, 30, 49);
        let sent = console(&mut s, ConsoleGesture::Split);
        assert_eq!(sent[..2], ["subscribe 9", "resize 9 30x49"], "8 stays subscribed on the first side");
        assert_eq!(sides(&mut s), (Some(8), Some(9), Some(9)));
        let page = console_page(&mut s);
        assert_eq!(page.tabs.iter().map(|t| (t.pane, t.on)).collect::<Vec<_>>(), [(8, true), (9, true)]);
        assert_eq!(typed(&mut s), ["input 9 [97]"]);
    }

    #[test]
    fn a_split_from_the_workspace_or_with_one_console_brings_the_console_and_asks_for_another() {
        let mut s = session(Hello::this_build());
        s.select(PaneId(1));
        s.receive(consoles(&[8]));
        let sent = s.call(Call::Command { action: Action::SplitConsole });
        assert_eq!(names(&sent)[..1], ["subscribe 8"], "the console comes up beside the Workspace's pane first");
        let [.., ClientMsg::SpawnConsole { request_id }, _] = sent.as_slice() else {
            panic!("{:?}", names(&sent));
        };
        let request_id = *request_id;
        assert_eq!(sides(&mut s), (Some(8), None, Some(8)), "nothing predicted");
        s.receive(ServerMsg::Created { request_id, created: Some(Created::Pane(PaneId(10))) });
        assert_eq!(names(&s.receive(consoles(&[8, 10])))[..1], ["subscribe 10"]);
        assert_eq!(sides(&mut s), (Some(8), Some(10), Some(10)), "the new one beside, with the keys");
        assert_eq!(console_page(&mut s).place, ConsolePlace::Docked);
    }

    #[test]
    fn the_keys_follow_the_side_clicked_and_a_tab_replaces_the_side_with_them() {
        let mut s = split();
        assert!(console(&mut s, ConsoleGesture::Select { pane: 8 }).iter().all(|m| m.starts_with("view")), "nothing subscribed");
        assert_eq!(sides(&mut s), (Some(8), Some(9), Some(8)));
        assert_eq!(typed(&mut s), ["input 8 [97]"]);
        leader_then(&mut s, "o");
        assert_eq!(sides(&mut s), (Some(8), Some(9), Some(9)), "the leader and o: the next pane on screen");
        leader_then(&mut s, "o");
        assert_eq!((sides(&mut s).2, s.on_screen()), (None, Some(PaneId(1))), "round to the Workspace's");
        leader_then(&mut s, "o");
        assert_eq!(sides(&mut s).2, Some(8));
        leader_then(&mut s, "o");

        s.receive(consoles(&[8, 9, 10]));
        let sent = console(&mut s, ConsoleGesture::Select { pane: 10 });
        assert_eq!(sent[..3], ["unsubscribe 9", "subscribe 10", "resize 10 30x49"]);
        assert_eq!(sides(&mut s), (Some(8), Some(10), Some(10)));
    }

    #[test]
    fn closing_a_side_leaves_the_other_filling_the_console_and_both_stay_tabs() {
        let mut s = split();
        let sent = console(&mut s, ConsoleGesture::CloseSide { pane: 8 });
        assert_eq!(sent[..2], ["unsubscribe 8", "resize 9 30x50"], "9 moves over at the first side's size");
        assert_eq!(sides(&mut s), (Some(9), None, Some(9)));
        assert_eq!(console_page(&mut s).tabs.len(), 2, "the daemon keeps both");

        console(&mut s, ConsoleGesture::Split);
        assert_eq!(sides(&mut s), (Some(9), Some(8), Some(8)));
        console(&mut s, ConsoleGesture::Select { pane: 9 });
        assert_eq!(console(&mut s, ConsoleGesture::Split)[..1], ["unsubscribe 8"], "the split's button keeps the side with the keys");
        assert_eq!(sides(&mut s), (Some(9), None, Some(9)));
    }

    #[test]
    fn a_split_sides_console_ending_closes_the_split() {
        let mut s = split();
        assert!(names(&s.receive(consoles(&[8]))).iter().all(|m| !m.starts_with("unsubscribe")), "the daemon let 9 go");
        assert_eq!(sides(&mut s), (Some(8), None, Some(8)));
        assert_eq!(typed(&mut s), ["input 8 [97]"]);

        let mut s = split();
        let sent = s.receive(consoles(&[9]));
        assert_eq!(names(&sent)[..1], ["resize 9 30x50"], "the second side's moves over");
        assert_eq!(sides(&mut s), (Some(9), None, Some(9)));

        let mut s = split();
        s.select(PaneId(1));
        s.receive(consoles(&[8]));
        assert_eq!((sides(&mut s), s.on_screen()), ((Some(8), None, None), Some(PaneId(1))), "the Workspace keeps its keys");
    }

    #[test]
    fn the_view_reported_names_the_side_with_the_keys() {
        let mut s = split();
        assert_eq!(s.view().pane, Some(PaneId(9)));
        let sent = console(&mut s, ConsoleGesture::Select { pane: 8 });
        assert_eq!(sent, ["view Some(8) true"]);
        s.focus(false);
        assert_eq!(s.watched(), None, "an away window watches nothing");
        s.focus(true);
        assert_eq!(s.watched(), Some(PaneId(8)), "the side with the keys is the one looked at");
    }

    #[test]
    fn each_side_asks_its_own_size() {
        let mut s = split();
        assert_eq!(names(&s.resize(Side::Second, 40, 60)), ["resize 9 40x60"]);
        assert_eq!(names(&s.resize(Side::First, 40, 61)), ["resize 8 40x61"]);
        assert!(s.resize(Side::First, 40, 61).is_empty(), "unchanged");
    }

    #[test]
    fn the_wheel_scrolls_the_side_under_it_without_taking_the_keys() {
        let mut s = split();
        for pane in [8, 9] {
            s.receive(ServerMsg::PaneRows {
                pane: PaneId(pane),
                rows: 5,
                cols: 10,
                runs: Vec::new(),
                cursor: argus_protocol::Cursor::default(),
                mouse: argus_protocol::MouseTracking::default(),
                alternate_screen: false,
                keyboard: Default::default(),
            });
        }
        let sent = s.wheel(PaneId(8), -1, (0, 0));
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::Scrollback { pane: PaneId(8), .. })), "{:?}", names(&sent));
        assert_eq!(s.shown.pane(), Some(PaneId(9)));
        let frames: Vec<_> = s
            .take_frames()
            .into_iter()
            .filter_map(|e| match e {
                Event::Frame(frame) => Some((frame.pane, frame.side)),
                _ => None,
            })
            .collect();
        assert_eq!(frames, [(8, Side::First), (9, Side::Second)], "a frame per side, naming it");
    }

    #[test]
    fn the_split_stays_beside_a_checkouts_pane_and_waits_out_a_hidden_console() {
        let mut s = split();
        assert_eq!(names(&s.select(PaneId(2)))[..2], ["unsubscribe 1", "subscribe 2"]);
        assert_eq!(s.shown.panes(), [PaneId(2), PaneId(8), PaneId(9)], "the split stays beside it");
        assert_eq!(sides(&mut s), (Some(8), Some(9), None), "the keys are the Workspace's");
        let sent = console(&mut s, ConsoleGesture::Select { pane: 9 });
        assert!(sent.iter().all(|m| m.starts_with("view")), "nothing subscribed: {sent:?}");
        assert_eq!(sides(&mut s), (Some(8), Some(9), Some(9)), "the tab picked takes the keys");
        console(&mut s, ConsoleGesture::Hide);
        leader_then(&mut s, "`");
        assert_eq!(sides(&mut s), (Some(8), Some(9), Some(8)));
        let saved = s.take_settings().expect("saved");
        assert!(saved.docks.unwrap().bottom.open, "docked is kept, the split is not in it");
    }

    #[test]
    fn an_older_daemon_is_not_asked_for_a_console() {
        let mut s = session(without_focus());
        assert!(console(&mut s, ConsoleGesture::Spawn).is_empty());
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text.contains("too old"))));
        assert!(!console_page(&mut s).can_spawn);
    }

    // ---- Extensions ------------------------------------------------------

    fn containers(rows: &[&str]) -> ServerMsg {
        let rows: Vec<String> =
            rows.iter().map(|key| format!(r#"{{"key": "{key}", "cells": [{{"kind": "text", "text": "{key}"}}]}}"#)).collect();
        let json = format!(
            r#"[{{"extension": "docker", "name": "containers", "title": "Containers", "columns": [{{"title": "Name"}}],
                 "rows": [{}], "actions": [{{"id": "restart", "label": "Restart", "key": "r"}}]}}]"#,
            rows.join(",")
        );
        ServerMsg::ExtensionViews(serde_json::from_str(&json).unwrap())
    }

    fn extensions_page(s: &mut Session) -> Option<page::ExtensionsPage> {
        s.events(0).into_iter().rev().find_map(|e| match e {
            Event::Extensions(page) => Some(*page),
            _ => None,
        })
    }

    fn panel(view: &str) -> Panel {
        Panel::extension("docker", view)
    }

    fn agents_view(s: &mut Session, pane: u64) {
        let view = argus_protocol::AgentView { id: "ci".into(), title: "CI".into(), ..Default::default() };
        s.receive(ServerMsg::ViewShown(Box::new(argus_protocol::ShownView { pane: PaneId(pane), at: 5, subject: None, view })));
    }

    /// The extension views on screen, by name.
    fn tables(s: &mut Session) -> Vec<String> {
        s.extensions_page().tables.into_iter().map(|t| t.view).collect()
    }

    #[test]
    fn a_published_view_is_listed_and_its_action_goes_to_the_daemon_as_it_stands() {
        let mut s = session(Hello::this_build());
        s.events(0);
        s.receive(containers(&["web", "db"]));
        let page = extensions_page(&mut s).unwrap();
        assert_eq!(page.views[0].title, "Containers");
        assert!(page.tables.is_empty(), "listed, not opened for the person");

        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() });
        s.call(Call::ExtensionKey { panel: panel("containers"), action: Action::Next });
        let sent = s.call(Call::ExtensionAction { panel: panel("containers"), action: "restart".into() });
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::ExtensionAction { row: Some(row), action, .. }] if row == "db" && action == "restart"
        ), "{sent:?}");
        let open = extensions_page(&mut s).unwrap().tables.remove(0);
        assert_eq!(open.rows.len(), 2, "the view is as the extension last said");
        assert_eq!(s.docks.dock_of(&panel("containers")), Some(Dock::Right), "where an older extension's opened");
    }

    #[test]
    fn a_double_click_or_enter_takes_the_views_default_action_on_its_row() {
        let mut s = session(Hello::this_build());
        let json = r#"[{"extension": "docker", "name": "containers", "title": "Containers", "columns": [{"title": "Name"}],
            "rows": [{"key": "web", "cells": [{"kind": "text", "text": "web"}]}, {"key": "db", "cells": [{"kind": "text", "text": "db"}]}],
            "actions": [{"id": "start", "label": "Start"}], "default": "start"}]"#;
        s.receive(ServerMsg::ExtensionViews(serde_json::from_str(json).unwrap()));
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() });
        let sent = s.call(Call::ExtensionDefault { panel: panel("containers"), row: "db".into() });
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::ExtensionAction { row: Some(row), action, .. }] if row == "db" && action == "start"
        ), "{sent:?}");
        let sent = s.call(Call::ExtensionKey { panel: panel("containers"), action: Action::Choose });
        assert!(matches!(sent.as_slice(), [ClientMsg::ExtensionAction { row: Some(row), .. }] if row == "db"), "{sent:?}");

        s.receive(containers(&["web"]));
        assert!(s.call(Call::ExtensionKey { panel: panel("containers"), action: Action::Choose }).is_empty(), "no default, nothing sent");
    }

    #[test]
    fn an_extensions_view_and_the_agents_views_are_tabs_of_one_dock_until_one_moves() {
        let mut s = session(Hello::this_build());
        s.select(PaneId(1));
        s.receive(containers(&["web"]));
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() });
        agents_view(&mut s, 1);
        assert_eq!(tables(&mut s), ["containers"], "an agent's view arriving does not take the dock");
        assert!(!s.docks.shows(&Panel::Views));
        let tabs = docks_page(&mut s).unwrap().right.tabs.into_iter().map(|t| t.title).collect::<Vec<_>>();
        assert_eq!(tabs, ["Views", "Containers"]);

        s.call(Call::OpenView { pane: 1, id: "ci".into() });
        assert!(s.docks.shows(&Panel::Views));
        assert!(tables(&mut s).is_empty(), "the other tab");
        docks(&mut s, DockGesture::Show { panel: panel("containers") });
        assert_eq!(tables(&mut s), ["containers"]);

        docks(&mut s, DockGesture::Move { panel: panel("containers"), dock: Dock::Bottom });
        docks(&mut s, DockGesture::Show { panel: Panel::Views });
        assert_eq!(tables(&mut s), ["containers"], "both on screen once in two docks");
        assert!(s.docks.shows(&Panel::Views));
        docks(&mut s, DockGesture::Hide { panel: panel("containers") });
        assert!(tables(&mut s).is_empty());
        assert!(s.docks.shows(&Panel::Views), "a dock's close is its own");
    }

    #[test]
    fn a_view_its_extension_takes_away_is_put_away() {
        let mut s = session(Hello::this_build());
        s.receive(containers(&["web"]));
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() });
        assert_eq!(extensions_page(&mut s).expect("opened").tables.len(), 1);
        s.receive(ServerMsg::ExtensionViews(Vec::new()));
        assert_eq!(extensions_page(&mut s), Some(page::ExtensionsPage::default()), "the page hears it went");
        assert!(extensions_page(&mut s).is_none(), "once");
        s.receive(containers(&["web"]));
        assert_eq!(tables(&mut s), ["containers"], "and comes back where it was, when it comes again");
    }

    #[test]
    fn a_command_goes_with_where_the_person_is_and_after_the_leader_from_a_pane() {
        let mut s = session(Hello::this_build());
        let commands = r#"[{"extension": "docker", "name": "up", "title": "Compose up", "key": {"key": "u", "ctrl": true}}]"#;
        s.receive(ServerMsg::ExtensionCommands(serde_json::from_str(commands).unwrap()));
        s.select(PaneId(1));
        let sent = s.call(Call::ExtensionCommand { extension: "docker".into(), command: "up".into(), checkout: None, pane: None });
        assert!(matches!(
            sent.as_slice(),
            [ClientMsg::ExtensionCommand { command, context, .. }, ..]
                if command == "up" && context.checkout == Some(CheckoutId(100)) && context.pane == Some(PaneId(1))
        ), "{sent:?}");
        s.key(&PageKey { ctrl: true, key: " ".into(), ..PageKey::default() });
        let sent = s.key(&PageKey { ctrl: true, key: "u".into(), ..PageKey::default() });
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::ExtensionCommand { .. })), "{sent:?}");
        let sent = s.call(Call::ExtensionCommand { extension: "docker".into(), command: "down".into(), checkout: None, pane: None });
        assert!(sent.iter().all(|m| !matches!(m, ClientMsg::ExtensionCommand { .. })), "no such command");
    }

    #[test]
    fn a_dialog_put_here_is_drawn_and_its_answer_goes_back() {
        let mut s = session(Hello::this_build());
        let dialog = argus_protocol::ExtDialog {
            title: "Restart".into(),
            kind: argus_protocol::ExtDialogKind::Confirm { message: "sure?".into(), yes: None },
            answers: Some(4),
        };
        s.receive(ServerMsg::ExtensionDialog { extension: "docker".into(), id: 9, dialog, prompted: true });
        let page = s.events(0).into_iter().find_map(|e| match e {
            Event::ExtensionDialog(page) => Some(*page),
            _ => None,
        });
        assert!(page.is_some_and(|p| p.open && p.prompted && p.id == 9));
        let sent = s.call(Call::AnswerDialog { dialog: 9, answer: Some(argus_protocol::ExtAnswer::Yes) });
        assert!(matches!(sent.as_slice(), [ClientMsg::ExtensionDialogAnswer { dialog: 9, answer: Some(_) }, ..]), "{sent:?}");
        s.receive(ServerMsg::ExtensionDialog {
            extension: "docker".into(),
            id: 10,
            dialog: argus_protocol::ExtDialog {
                title: "x".into(),
                kind: argus_protocol::ExtDialogKind::Prompt { message: "?".into(), initial: String::new() },
                answers: None,
            },
            prompted: false,
        });
        s.receive(ServerMsg::ExtensionDialogWithdrawn { id: 10 });
        assert!(s.call(Call::AnswerDialog { dialog: 10, answer: None }).iter().all(|m| !matches!(m, ClientMsg::ExtensionDialogAnswer { .. })));
    }

    #[test]
    fn an_extensions_notice_is_said_with_its_name() {
        let mut s = session(Hello::this_build());
        s.receive(ServerMsg::ExtensionNotice { extension: "docker".into(), text: "web restarted".into() });
        assert!(s.events(0).iter().any(|e| matches!(e, Event::Notice { text } if text == "docker: web restarted")));
    }

    /// One view per scope: everywhere, in repository 10 (argus), and in
    /// repository 20 (api); the first asks for a page.
    fn scoped() -> ServerMsg {
        let json = r#"[
            {"extension": "docker", "name": "containers", "title": "Containers", "place": "stage", "icon": "box",
             "columns": [{"title": "Name"}], "rows": [{"key": "web", "cells": [{"kind": "text", "text": "web"}]}]},
            {"extension": "docker", "name": "argus", "title": "Argus images", "scope": {"repository": 10},
             "columns": [{"title": "Tag"}]},
            {"extension": "docker", "name": "api", "title": "API images", "scope": {"repository": 20},
             "columns": [{"title": "Tag"}]}
        ]"#;
        ServerMsg::ExtensionViews(serde_json::from_str(json).unwrap())
    }

    fn listed(s: &mut Session) -> Vec<String> {
        s.extensions_page().views.into_iter().map(|t| t.view).collect()
    }

    #[test]
    fn a_path_in_a_view_opens_in_the_editor_of_the_pane_checkout_and_one_that_escapes_does_nothing() {
        let mut s = session(Hello::this_build());
        let path = |path: &str| Call::OpenRef { pane: Some(1), reference: Reference::Path { path: path.into(), line: Some(3) } };
        let sent = s.call(path("src/lib.rs"));
        assert!(
            matches!(&sent[..], [ClientMsg::OpenInEditor { checkout: CheckoutId(100), path, line: Some(3), .. }] if path == "src/lib.rs"),
            "{sent:?}"
        );
        for bad in ["../secrets", "/etc/passwd"] {
            assert!(s.call(path(bad)).is_empty(), "{bad}");
        }
        assert!(notices(&mut s).iter().any(|t| t.contains("not a path inside the checkout")));
    }

    #[test]
    fn a_reference_to_a_pane_that_is_gone_says_so_and_one_that_is_there_is_shown() {
        let mut s = session(Hello::this_build());
        s.call(Call::OpenRef { pane: None, reference: Reference::Pane { pane: 99 } });
        assert!(notices(&mut s).iter().any(|t| t.contains("gone")));
        s.call(Call::OpenRef { pane: None, reference: Reference::Pane { pane: 2 } });
        assert_eq!(s.shown.pane(), Some(PaneId(2)));
    }

    #[test]
    fn a_commit_in_a_view_opens_the_history_of_the_pane_checkout() {
        let mut s = session(Hello::this_build());
        let sent = s.call(Call::OpenRef { pane: Some(3), reference: Reference::Commit { sha: "abc1234".into() } });
        assert!(matches!(&sent[..], [ClientMsg::ListCommits { checkout: CheckoutId(200), .. }]), "{sent:?}");
    }

    #[test]
    fn a_feature_in_a_view_is_shown_once_the_board_of_the_panes_repository_says_which_it_is() {
        let mut s = session(without_focus());
        s.call(Call::OpenRef { pane: Some(1), reference: Reference::Feature { number: 7 } });
        assert_eq!(s.stages.now(), Stage::Feature);
        s.receive(ServerMsg::Decisions(Box::new(crate::fixtures::board())));
        assert_eq!(s.features.peeked(), Some("trim"));
        s.call(Call::OpenRef { pane: Some(1), reference: Reference::Feature { number: 99 } });
        assert!(notices(&mut s).iter().any(|t| t.contains("no feature #99")));
    }

    #[test]
    fn a_view_popped_out_is_a_page_over_the_stage_and_goes_back_when_docked_or_closed() {
        let mut s = session(Hello::this_build());
        agents_view(&mut s, 1);
        s.call(Call::PopOutView { pane: 1, id: "ci".into() });
        assert_eq!(s.stages.now(), Stage::View);
        assert!(!s.docks.shows(&Panel::Views), "it is in the page, not also in a dock");
        assert!(s.views.take(false, true, |_| None).unwrap().popped.is_some());

        s.call(Call::OpenView { pane: 1, id: "ci".into() });
        assert_eq!(s.stages.now(), Stage::Workspace, "docking it goes back to the stage it opened over");
        assert!(s.docks.shows(&Panel::Views));

        s.call(Call::PopOutView { pane: 1, id: "ci".into() });
        s.receive(ServerMsg::ViewClosed { pane: PaneId(1), id: "ci".into() });
        assert_eq!(s.stages.now(), Stage::Workspace, "a view that closes is no page");
        s.call(Call::PopOutView { pane: 1, id: "gone".into() });
        assert_eq!(s.stages.now(), Stage::Workspace, "a view the window does not hold opens nothing");
    }

    #[test]
    fn the_notice_for_a_closed_popped_view_names_its_title_not_its_id() {
        let mut s = session(Hello::this_build());
        agents_view(&mut s, 1);
        s.call(Call::PopOutView { pane: 1, id: "ci".into() });
        s.receive(ServerMsg::ViewClosed { pane: PaneId(1), id: "ci".into() });
        assert!(notices(&mut s).iter().any(|t| t == "CI was closed"));
    }

    #[test]
    fn a_press_the_daemon_refused_is_said_and_gives_the_button_back() {
        let mut s = session(Hello::this_build());
        agents_view(&mut s, 1);
        let press = argus_protocol::ViewPress { view: "ci".into(), action: "retry".into(), rows: vec![], form: None, answers: vec![] };
        s.call(Call::PressView { pane: 1, press });
        let pending = |s: &mut Session| s.views.take(false, true, |_| None).map(|p| p.views[0].pressed.clone());
        assert_eq!(pending(&mut s), Some(vec!["retry".to_string()]));
        s.receive(ServerMsg::ViewPressFailed { pane: PaneId(1), view: "ci".into(), action: "retry".into(), reason: "the agent has gone".into() });
        assert!(notices(&mut s).iter().any(|t| t == "CI: the agent has gone"));
        assert_eq!(pending(&mut s), Some(Vec::new()));
    }

    #[test]
    fn the_page_is_told_a_daemon_that_cannot_take_presses_cannot() {
        let mut s = session(Hello { capabilities: vec![AGENT_VIEWS.to_string()], ..Hello::this_build() });
        agents_view(&mut s, 1);
        assert!(!s.views.take(false, s.status.daemon_can(VIEW_ACTIONS), |_| None).unwrap().can_press);
        let mut s = session(Hello::this_build());
        agents_view(&mut s, 1);
        let page = s.events(0).into_iter().find_map(|e| match e {
            Event::Views(page) => Some(page),
            _ => None,
        });
        assert!(page.unwrap().can_press);
    }

    fn notices(s: &mut Session) -> Vec<String> {
        s.events(0)
            .into_iter()
            .filter_map(|e| match e {
                Event::Notice { text } => Some(text),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_sidebars_extensions_list_the_views_whose_scope_is_where_the_person_is() {
        let mut s = session(Hello::this_build());
        s.receive(scoped());
        assert_eq!(listed(&mut s), ["containers", "argus"], "the chosen repository's");
        s.choose_repository(RepositoryId(20));
        s.select_project(ProjectId(2));
        assert_eq!(listed(&mut s), ["containers", "api"], "and the next's once it is chosen");
        s.receive(ServerMsg::ExtensionViews(Vec::new()));
        assert!(listed(&mut s).is_empty(), "gone with the extension");
    }

    #[test]
    fn a_view_opens_as_its_extension_says_and_where_the_person_moved_it_after() {
        let mut s = session(Hello::this_build());
        s.call(Call::ShowCheckouts { open: true });
        s.receive(scoped());
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() });
        assert_eq!(s.stages.now(), Stage::Extension, "a page, as it asked");
        assert!(s.extensions_page().tables.first().is_some_and(|t| t.page));

        s.call(Call::ExtensionMode { panel: panel("containers"), page: false });
        assert_eq!(s.stages.now(), Stage::Checkouts, "docking goes back to the stage it opened over");
        assert!(s.extensions_page().tables.first().is_some_and(|t| !t.page));
        let saved = s.take_settings().expect("saved").docks.unwrap();
        assert!(saved.right.tabs.contains(&panel("containers")) && saved.staged.is_empty());

        s.receive(scoped());
        docks(&mut s, DockGesture::Hide { panel: panel("containers") });
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() });
        assert_eq!(s.stages.now(), Stage::Checkouts, "docked again after its extension republished");
        assert_eq!(tables(&mut s), ["containers"]);
    }

    #[test]
    fn a_page_whose_view_goes_falls_back_saying_so_and_a_docked_one_just_closes() {
        let mut s = session(Hello::this_build());
        s.receive(scoped());
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() });
        s.events(0);
        s.receive(ServerMsg::ExtensionViews(Vec::new()));
        assert_eq!(s.stages.now(), Stage::Workspace);
        assert!(notices(&mut s).iter().any(|n| n.contains("Containers")), "said");

        s.receive(scoped());
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "argus".into() });
        assert!(s.extensions_page().tables.first().is_some_and(|t| !t.page));
        s.events(0);
        s.receive(ServerMsg::ExtensionViews(Vec::new()));
        assert!(tables(&mut s).is_empty());
        assert_eq!(s.stages.now(), Stage::Workspace);
    }

    /// The `docker/dev` view, a console, published running in `pane` or
    /// still waiting for one, and ended when `ended`.
    fn console_view(pane: Option<u64>, ended: bool) -> ServerMsg {
        let pane = pane.map_or("null".to_string(), |p| p.to_string());
        let ended = if ended { r#"{"code": 3}"# } else { "null" };
        let json = format!(
            r#"[{{"extension": "docker", "name": "dev", "title": "Dev", "place": "bottom",
                "console": {{"command": ["sh"], "pane": {pane}, "ended": {ended}}},
                "actions": [{{"id": "restart", "label": "Restart", "view_wide": true}}]}}]"#
        );
        ServerMsg::ExtensionViews(serde_json::from_str(&json).unwrap())
    }

    fn dev_console(s: &mut Session) -> page::ExtTableConsole {
        s.extensions_page().tables.into_iter().find(|t| t.view == "dev").and_then(|t| t.console).expect("on screen")
    }

    #[test]
    fn a_console_view_on_screen_is_a_screen_of_its_own_that_follows_its_pane() {
        let mut s = session(Hello::this_build());

        s.select(PaneId(1));
        s.receive(console_view(None, false));
        assert!(s.shown.view_side("docker/dev").is_none(), "off screen: nothing subscribed");
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "dev".into() });
        let console = dev_console(&mut s);
        assert_eq!((console.pane, console.keyed), (None, false), "starting");
        let side = console.side.expect("a screen waits for its pane");
        assert!(matches!(side, Side::View(_)));
        assert!(names(&s.resize(side, 12, 70)).is_empty(), "nothing to size yet");

        assert_eq!(names(&s.receive(console_view(Some(30), false))), ["subscribe 30", "resize 30 12x70"]);
        let sent = s.receive(console_view(Some(31), false));
        assert_eq!(names(&sent), ["unsubscribe 30", "subscribe 31", "resize 31 12x70"], "ended and run again: a new pane");
        assert_eq!(s.shown.on(side), Some(PaneId(31)));
        assert!(s.ended().is_empty(), "held by its view, not the tree");

        let ended = {
            s.receive(console_view(Some(31), true));
            dev_console(&mut s).ended
        };
        assert_eq!(ended.map(|e| e.code), Some(Some(3)), "the last screen stays with how it ended");
        assert_eq!(s.shown.on(side), Some(PaneId(31)));
    }

    #[test]
    fn a_console_view_takes_the_keys_when_pressed_and_types_into_its_pane() {
        let mut s = session(Hello::this_build());
        s.select(PaneId(1));
        s.receive(console_view(Some(30), false));
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "dev".into() });
        assert_eq!(s.shown.pane(), Some(PaneId(1)), "showing it does not take the keys");
        s.call(Call::FocusView { pane: 30 });
        assert!(dev_console(&mut s).keyed);
        assert!(!s.console_typing(), "the console's own choices stay the console's");
        let key = r#"{"call":"key","key":{"key":"a","code":"KeyA","ctrl":false,"alt":false,"shift":false,"meta":false}}"#;
        let sent = s.call(serde_json::from_str(key).unwrap());
        assert!(names(&sent).iter().any(|m| m.starts_with("input 30")), "{:?}", names(&sent));
        let killed = s.command(Action::Close);
        assert!(!killed.iter().any(|m| matches!(m, ClientMsg::Kill { .. })), "the extension owns its lifetime");
    }

    #[test]
    fn a_console_view_hidden_or_gone_lets_its_pane_go_and_hands_back_the_keys() {
        let mut s = session(Hello::this_build());
        s.select(PaneId(1));
        s.receive(console_view(Some(30), false));
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "dev".into() });
        s.call(Call::FocusView { pane: 30 });
        let sent = docks(&mut s, DockGesture::Hide { panel: panel("dev") });
        assert_eq!(sent[..1], ["unsubscribe 30"]);
        assert_eq!(s.shown.pane(), Some(PaneId(1)));

        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "dev".into() });
        assert_eq!(names(&s.receive(ServerMsg::ExtensionViews(Vec::new())))[..1], ["unsubscribe 30"]);
    }

    #[test]
    fn a_console_view_on_the_stage_is_a_page_and_the_table_has_no_rows() {
        let mut s = session(Hello::this_build());
        s.receive(console_view(Some(30), false));
        s.call(Call::ExtensionMode { panel: panel("dev"), page: true });
        assert_eq!(s.stages.now(), Stage::Extension);
        let table = s.extensions_page().tables.remove(0);
        assert!(table.page && table.rows.is_empty() && table.columns.is_empty() && table.selected.is_none());
        assert_eq!(s.shown.pane(), Some(PaneId(30)), "a page has the keys the moment it is up");
        assert!(table.console.as_ref().unwrap().keyed);
        assert_eq!(table.actions.iter().map(|a| a.view_wide).collect::<Vec<_>>(), [true], "Restart rides along");
        assert_eq!(s.shown.on(table.console.unwrap().side.unwrap()), Some(PaneId(30)));
        assert!(s.call(Call::ExtensionKey { panel: panel("dev"), action: Action::Next }).is_empty());
    }

    #[test]
    fn a_reconnect_drops_a_console_views_screen_until_the_new_daemon_names_its_pane() {
        let mut s = session(Hello::this_build());
        s.receive(console_view(Some(30), false));
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "dev".into() });
        let again = s.connected(Some(Hello::this_build()), None, vec![with_tree(tree())]);
        assert!(!names(&again).contains(&"subscribe 30".to_string()), "the old daemon's pane: {:?}", names(&again));
        assert_eq!(names(&s.receive(console_view(Some(5), false)))[..1], ["subscribe 5"]);
    }

    #[test]
    fn a_page_and_views_docked_elsewhere_are_on_screen_together_each_with_its_own_row() {
        let mut s = session(Hello::this_build());
        s.receive(scoped());
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "argus".into() });
        docks(&mut s, DockGesture::Move { panel: panel("argus"), dock: Dock::Left });
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() });
        assert_eq!(s.stages.now(), Stage::Extension);
        assert_eq!(tables(&mut s), ["containers", "argus"]);
        let page = s.extensions_page();
        assert_eq!(page.tables.iter().map(|t| t.page).collect::<Vec<_>>(), [true, false]);
        assert!(page.views.iter().all(|t| t.on || t.view == "api"));

        // A page moved into a dock leaves the stage for it.
        docks(&mut s, DockGesture::Move { panel: panel("containers"), dock: Dock::Bottom });
        assert_eq!(s.stages.now(), Stage::Workspace);
        assert_eq!(tables(&mut s), ["containers", "argus"]);
        assert!(s.extensions_page().tables.iter().all(|t| !t.page));
        // The palette moves the view last used.
        s.call(Call::ExtensionKey { panel: panel("argus"), action: Action::Next });
        s.call(Call::Command { action: Action::ExtensionPage });
        assert_eq!((s.stages.now(), s.docks.place_of(&panel("argus"))), (Stage::Extension, Some(Place::Stage)));
    }

    #[test]
    fn leaving_a_page_for_another_stage_closes_it_and_the_fold_is_remembered() {
        let mut s = session(Hello::this_build());
        s.receive(scoped());
        s.call(Call::OpenExtensionView { extension: "docker".into(), view: "containers".into() });
        s.call(Call::ShowFeatures { open: true });
        assert!(tables(&mut s).is_empty());
        assert!(!s.extensions_page().listed);
        s.call(Call::ToggleExtensions);
        assert!(s.extensions_page().listed);
        assert!(s.take_settings().is_some_and(|saved| saved.extensions.open));
    }

    // ---- Saved views --------------------------------------------------------

    fn saved_view(id: &str) -> argus_protocol::SavedView {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "scope": {"kind": "repository", "repository": 10},
            "built_at": 100,
            "view": {"id": id, "title": format!("Title of {id}"), "blocks": []},
        }))
        .unwrap()
    }

    fn with_saved(ids: &[&str]) -> Session {
        let mut s = session(Hello::this_build());
        s.receive(ServerMsg::SavedViews(ids.iter().map(|id| saved_view(id)).collect()));
        s
    }

    fn saved_page(s: &mut Session) -> Option<crate::saved_views::SavedViewsPage> {
        // Asked again for what it holds, not only for a change.
        s.saved.forget();
        s.events(0).into_iter().find_map(|e| match e {
            Event::SavedViews(page) => Some(*page),
            _ => None,
        })
    }

    fn saved_panel(id: &str) -> Panel {
        Panel::Saved(id.into())
    }

    fn open_saved(s: &mut Session, id: &str) {
        s.call(Call::OpenSavedView { id: id.into(), from: None });
    }

    fn follow(s: &mut Session, owner: &str, column: usize, id: &str) {
        s.call(Call::OpenSavedView { id: id.into(), from: Some(SavedColumn { panel: saved_panel(owner), column }) });
    }

    fn columns(s: &mut Session, owner: &str) -> Vec<String> {
        saved_page(s).and_then(|p| p.panels.into_iter().find(|p| p.panel == saved_panel(owner))).map(|p| p.columns).unwrap_or_default()
    }

    #[test]
    fn saved_views_are_watched_only_from_a_daemon_that_lists_them() {
        let mut s = Session::default();
        let sent = s.connected(Some(Hello::this_build()), None, vec![with_tree(tree())]);
        assert!(sent.iter().any(|m| matches!(m, ClientMsg::WatchSavedViews)));
        let mut old = Session::default();
        let sent = old.connected(Some(Hello { capabilities: vec![AGENT_VIEWS.to_string()], ..Hello::this_build() }), None, vec![with_tree(tree())]);
        assert!(!sent.iter().any(|m| matches!(m, ClientMsg::WatchSavedViews)));
        let sent = old.call(Call::RefreshSavedView { id: "plan".into() });
        assert!(sent.is_empty(), "nothing to refresh on a daemon that keeps none");
    }

    #[test]
    fn opening_a_saved_view_docks_it_on_the_right_and_the_file_keeps_the_place() {
        let mut s = with_saved(&["plan"]);
        assert!(!s.docks.shows(&saved_panel("plan")), "listed, not opened");
        open_saved(&mut s, "plan");
        assert_eq!(s.docks.place_of(&saved_panel("plan")), Some(Place::Dock(Dock::Right)));
        assert!(s.docks.shows(&saved_panel("plan")));
        let page = docks_page(&mut s).unwrap();
        assert!(page.right.tabs.iter().any(|t| t.panel == saved_panel("plan") && t.title == "Title of plan"));
        let saved = s.take_settings().expect("saved").docks.unwrap();
        assert!(saved.right.tabs.contains(&saved_panel("plan")));
        assert!(Docks::new(saved).dock_of(&saved_panel("plan")).is_some(), "across runs");
    }

    #[test]
    fn a_saved_view_the_daemon_does_not_hold_is_not_opened() {
        let mut s = with_saved(&["plan"]);
        open_saved(&mut s, "ghost");
        assert_eq!(s.docks.place_of(&saved_panel("ghost")), None);
        assert_eq!(notices(&mut s), ["no saved view ghost"]);
    }

    #[test]
    fn following_a_link_in_a_column_puts_the_target_after_it_and_drops_what_followed() {
        let mut s = with_saved(&["a", "b", "c", "d"]);
        s.set(SettingChange::SavedLinks { opens: SavedLinks::Column });
        open_saved(&mut s, "a");
        follow(&mut s, "a", 0, "b");
        follow(&mut s, "a", 1, "c");
        assert_eq!(columns(&mut s, "a"), ["a", "b", "c"]);
        follow(&mut s, "a", 0, "d");
        assert_eq!(columns(&mut s, "a"), ["a", "d"]);
        assert_eq!(s.docks.place_of(&saved_panel("b")), None, "a column is not a panel");
    }

    #[test]
    fn a_link_from_an_agents_view_or_an_extension_cell_opens_a_panel_or_brings_out_the_one_showing_it() {
        let mut s = with_saved(&["a", "b"]);
        s.set(SettingChange::SavedLinks { opens: SavedLinks::Column });
        // The page sends the same call from both, with no column.
        let call: Call = serde_json::from_str(r#"{"call": "open_saved_view", "id": "b", "from": null}"#).unwrap();
        s.call(call);
        assert_eq!(columns(&mut s, "b"), ["b"], "its own first column");
        open_saved(&mut s, "a");
        follow(&mut s, "a", 0, "b");
        docks(&mut s, DockGesture::Hide { panel: saved_panel("a") });
        docks(&mut s, DockGesture::Hide { panel: saved_panel("b") });
        open_saved(&mut s, "b");
        assert!(s.docks.shows(&saved_panel("b")), "the panel named for it comes first");
        assert_eq!(columns(&mut s, "a"), ["a", "b"], "and the other is left as it was");
        let followed: Call = serde_json::from_str(r#"{"call": "open_saved_view", "id": "b", "from": {"panel": "saved:a", "column": 0}}"#).unwrap();
        assert!(matches!(followed, Call::OpenSavedView { from: Some(SavedColumn { column: 0, .. }), .. }));
    }

    #[test]
    fn popping_a_column_out_makes_it_a_panel_of_its_own_and_leaves_the_rest() {
        let mut s = with_saved(&["a", "b", "c"]);
        s.set(SettingChange::SavedLinks { opens: SavedLinks::Column });
        open_saved(&mut s, "a");
        follow(&mut s, "a", 0, "b");
        follow(&mut s, "a", 1, "c");
        s.call(Call::PopOutSavedColumn { panel: saved_panel("a"), column: 1 });
        assert_eq!(columns(&mut s, "a"), ["a", "c"]);
        assert_eq!(columns(&mut s, "b"), ["b"]);
        assert!(s.docks.shows(&saved_panel("b")));
        s.call(Call::PopOutSavedColumn { panel: saved_panel("a"), column: 0 });
        assert_eq!(columns(&mut s, "a"), ["a", "c"], "the first column is the panel already");
    }

    #[test]
    fn a_link_in_a_saved_view_opens_a_dialog_unless_links_open_columns() {
        let mut s = with_saved(&["a", "b", "c"]);
        open_saved(&mut s, "a");
        follow(&mut s, "a", 0, "b");
        assert_eq!(columns(&mut s, "a"), ["a"], "the panel it came from keeps its columns");
        assert_eq!(s.docks.place_of(&saved_panel("b")), Some(Place::Float));
        assert!(s.docks.shows(&saved_panel("a")), "and its dock");
        follow(&mut s, "b", 0, "a");
        assert_eq!(s.docks.place_of(&saved_panel("a")), Some(Place::Dock(Dock::Right)), "a view open already is brought out where it is");
        s.docks.reload();
        let page = docks_page(&mut s).unwrap();
        assert_eq!(page.floating.iter().map(|f| f.title.as_str()).collect::<Vec<_>>(), ["Title of b"]);
        assert!(saved_page(&mut s).unwrap().panels.iter().any(|p| p.panel == saved_panel("b")), "its columns are drawn");
        s.set(SettingChange::SavedLinks { opens: SavedLinks::Column });
        follow(&mut s, "a", 0, "c");
        assert_eq!(columns(&mut s, "a"), ["a", "c"]);
        assert_eq!(s.docks.place_of(&saved_panel("c")), None);
    }

    #[test]
    fn a_column_or_a_whole_panel_floats_and_a_dialog_docks_again_by_its_menu() {
        let mut s = with_saved(&["a", "b"]);
        s.set(SettingChange::SavedLinks { opens: SavedLinks::Column });
        open_saved(&mut s, "a");
        follow(&mut s, "a", 0, "b");
        s.call(Call::FloatSavedColumn { panel: saved_panel("a"), column: 1 });
        assert_eq!(columns(&mut s, "a"), ["a"]);
        assert_eq!(s.docks.place_of(&saved_panel("b")), Some(Place::Float));
        s.call(Call::FloatSavedColumn { panel: saved_panel("a"), column: 0 });
        assert_eq!(s.docks.place_of(&saved_panel("a")), Some(Place::Float));
        docks(&mut s, DockGesture::Bounds { panel: saved_panel("a"), bounds: FloatBounds { x: 10, y: 20, width: 600, height: 400 } });
        let saved = s.take_settings().expect("saved").docks.unwrap();
        assert_eq!(saved.floating.last().map(|f| (f.panel.clone(), f.bounds.x)), Some((saved_panel("a"), 10)), "the file keeps it");
        docks(&mut s, DockGesture::Raise { panel: saved_panel("b") });
        s.docks.reload();
        assert_eq!(docks_page(&mut s).unwrap().floating.last().map(|f| f.panel.clone()), Some(saved_panel("b")));
        docks(&mut s, DockGesture::Move { panel: saved_panel("b"), dock: Dock::Left });
        assert_eq!(s.docks.place_of(&saved_panel("b")), Some(Place::Dock(Dock::Left)));
        s.call(Call::CloseSavedColumn { panel: saved_panel("a"), column: 0 });
        assert_eq!(s.docks.place_of(&saved_panel("a")), None, "a dialog's close takes it");
    }

    #[test]
    fn closing_a_column_leaves_the_panel_and_closing_the_first_takes_it_from_the_docks() {
        let mut s = with_saved(&["a", "b"]);
        s.set(SettingChange::SavedLinks { opens: SavedLinks::Column });
        open_saved(&mut s, "a");
        follow(&mut s, "a", 0, "b");
        s.call(Call::CloseSavedColumn { panel: saved_panel("a"), column: 1 });
        assert_eq!(columns(&mut s, "a"), ["a"]);
        assert!(s.docks.shows(&saved_panel("a")));
        s.call(Call::CloseSavedColumn { panel: saved_panel("a"), column: 0 });
        assert_eq!(s.docks.place_of(&saved_panel("a")), None);
        assert!(!s.take_settings().unwrap().docks.unwrap().right.tabs.contains(&saved_panel("a")));
    }

    #[test]
    fn a_deleted_view_takes_its_panel_with_it_and_a_refusal_is_said() {
        let mut s = with_saved(&["a", "b"]);
        s.set(SettingChange::SavedLinks { opens: SavedLinks::Column });
        open_saved(&mut s, "a");
        follow(&mut s, "a", 0, "b");
        s.receive(ServerMsg::SavedViewRemoved { id: "b".into() });
        assert_eq!(columns(&mut s, "a"), ["a"]);
        s.receive(ServerMsg::SavedViewRemoved { id: "a".into() });
        assert_eq!(s.docks.place_of(&saved_panel("a")), None);
        assert!(saved_page(&mut s).unwrap().views.is_empty());
        let mut s = with_saved(&["a"]);
        let sent = s.call(Call::RefreshSavedView { id: "a".into() });
        assert!(matches!(&sent[..], [ClientMsg::RefreshSavedView { id }] if id == "a"));
        assert!(saved_page(&mut s).unwrap().views[0].refreshing);
        s.receive(ServerMsg::SavedViewFailed { id: "a".into(), reason: "no agent runs it".into() });
        assert_eq!(notices(&mut s), ["Title of a: no agent runs it"]);
        assert!(!saved_page(&mut s).unwrap().views[0].refreshing);
    }

    #[test]
    fn the_header_actions_go_to_the_daemon_by_id() {
        let mut s = with_saved(&["a"]);
        assert!(matches!(&s.call(Call::MarkSavedViewStale { id: "a".into() })[..], [ClientMsg::MarkSavedViewStale { id }] if id == "a"));
        assert!(matches!(&s.call(Call::DeleteSavedView { id: "a".into() })[..], [ClientMsg::DeleteSavedView { id }] if id == "a"));
        assert!(s.call(Call::DeleteSavedView { id: "nope".into() }).is_empty());
    }
}
