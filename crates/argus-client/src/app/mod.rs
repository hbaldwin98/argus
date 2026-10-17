//! The client's model: everything on screen is a function of this type.
//!
//! `App` holds the tree the daemon sent, where the selection is, which
//! modal is up, and the streamed screen of every pane being drawn. It
//! never predicts the result of a request — a row moves when the tree
//! comes back saying it moved — so a refused action leaves the panel
//! showing what is actually true.
//!
//! The `impl` is split by what the operator is doing: `tree` for the
//! selection and moving it, `gestures` for keys, the pointer and scrolling,
//! `daemon` for what is asked of the daemon and what arrives back,
//! `layers` for the modal layers, `agent_views` for the views agents
//! showed, `saved_views` for the ones kept and the links followed between
//! them, and `consoles` for the shells that belong to no checkout.

use argus_protocol::{
    CheckoutId, CheckoutInfo, ClientMsg, HeldTree, PaneId, PaneInfo, PaneState,
    PaneStatus, ProjectId, ProjectInfo, RepositoryId, RepositoryInfo, ReviewAnchor, ServerMsg,
    WorkspaceId, WorkspaceInfo,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use layers::brief::{BriefMode, BriefTarget, BriefView};
use layers::dirpicker::{DirAction, DirPicker, DirTarget};
use argus_protocol::fuzzy::Fuzzy;
use argus_protocol::panes::view::PaneView;
use argus_protocol::review::history_view::{Drill, HistoryView};
use crate::input::pty::{encode_key, encode_mouse, is_leader};
use argus_protocol::review::agent_comment::{self, Comment};
use argus_protocol::review::session::ReviewSession;
use argus_protocol::review::view::ReviewView;
use crate::input::selection::TerminalSelection;
use crate::ui::theme::Theme;

use argus_protocol::ReviewBase;
use gestures::layout::in_rect;
pub use gestures::layout::{Focus, Layout, Panel};
pub use layers::modal::{Help, Overlay, Picker, PickerKind, Prompt, RemoveTarget, Setting};
pub use gestures::mode::{Mode, OverlayMode};
pub use tree::rail::RailRow;
pub use tree::rows::{CheckoutAnchor, CheckoutRow, PaneLocation};
pub use board_writes::LineInput;
pub use feature::{FeatureHit, FeatureRow, Lens, Standing, GROUPS};
pub use agent_views::AgentViews;
pub use saved_views::{Links, SavedViews};
pub use views::View;

mod agent_views;
mod board_writes;
mod consoles;
mod daemon;
mod feature;
mod gestures;
pub mod layers;
use daemon::awaited::Then;
mod saved_views;
mod tree;
mod views;

fn state_word(status: PaneStatus) -> &'static str {
    match status {
        PaneStatus::Idle => "idle",
        PaneStatus::Working => "working",
        PaneStatus::Waiting => "needs attention",
        PaneStatus::NeedsReview => "needs review",
        PaneStatus::Done => "done",
        PaneStatus::Failed => "failed",
        PaneStatus::Exited { code: Some(0) } => "exited",
        PaneStatus::Exited { .. } => "exited unsuccessfully",
    }
}

fn effective_label(pane: &PaneInfo, state: PaneState) -> String {
    match state.child {
        Some(child) => format!("{} / {child}", pane.title),
        None => pane.title.clone(),
    }
}

/// Includes nested agents, because their parent row is the only
/// selectable destination the client can flash or open.
fn attention_of(pane: &PaneInfo) -> Option<(String, Option<String>)> {
    let state = pane.loudest_state();
    state
        .status
        .needs_you()
        .then(|| (effective_label(pane, state), state.note.map(str::to_string)))
}

/// A monotonically increasing, never-zero request id for one kind of
/// request, plus which (key, id) pair is still awaited. A reply carrying
/// any other id is for a request this app has since moved on from — a
/// slower diff overtaken by a faster one, say — and is dropped rather than
/// applied.
struct Outstanding<K> {
    wanted: Option<(K, u64)>,
    next: u64,
}

impl<K: PartialEq + Copy> Outstanding<K> {
    fn new() -> Self {
        Self { wanted: None, next: 1 }
    }

    /// Hands out the next id and remembers it as the one awaited for `key`.
    fn request(&mut self, key: K) -> u64 {
        let id = self.next;
        self.next = self.next.wrapping_add(1).max(1);
        self.wanted = Some((key, id));
        id
    }

    fn is_wanted(&self, key: K, id: u64) -> bool {
        self.wanted == Some((key, id))
    }

    fn clear(&mut self) {
        self.wanted = None;
    }

    /// Clears and reports whether `(key, id)` was in fact the one awaited.
    fn take_if_wanted(&mut self, key: K, id: u64) -> bool {
        let wanted = self.is_wanted(key, id);
        if wanted {
            self.wanted = None;
        }
        wanted
    }
}

/// A host this client is attached to, as the host picker shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedHost {
    /// `None` for this machine.
    pub host: Option<String>,
    /// False while its daemon is gone and the client is trying again.
    pub connected: bool,
}

/// A change of host the picker asked for. Hosts belong to the run loop,
/// not to any one app, so the app records the ask and the loop carries it
/// out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRequest {
    /// Put an attached host on screen; `None` is this machine.
    Show(Option<String>),
    /// Connect to a host over ssh, and put it on screen once connected.
    Connect(String),
}

pub struct App {
    /// The machine this app's daemon runs on, as ssh names it; `None` for
    /// this one.
    pub host: Option<String>,
    /// Whether this app's host is the one on screen. One that is not holds
    /// no pane subscriptions: nothing draws its grids, and a remote host
    /// would be streaming them over ssh for nobody.
    pub on_screen: bool,
    /// Every host this client is attached to, so the host picker can say
    /// which are connected.
    pub attached_hosts: Vec<AttachedHost>,
    /// What the host picker asked for, until the run loop takes it.
    pub host_request: Option<HostRequest>,
    pub tree: Vec<ProjectInfo>,
    /// The shells that belong to no checkout, as the daemon last listed
    /// them. Beside the tree because they are not in it: see [`consoles`].
    pub consoles: Vec<PaneInfo>,
    /// Whether the daemon keeps consoles (`CONSOLES`). False until it
    /// greets, and for a daemon from before them: the rail draws no section
    /// and `c` says why.
    pub consoles_offered: bool,
    /// The console picked on the rail, by id. Read through
    /// [`App::current_console`], which says when it is the one on the stage.
    console: Option<PaneId>,
    pub templates: Vec<String>,
    /// Every workspace, not just the open one — the picker lists them all,
    /// and their pane counts are how an agent working in a workspace you
    /// are not looking at stays visible.
    pub workspaces: Vec<WorkspaceInfo>,
    /// Name of the open workspace, shown above the project list so the
    /// scope of that column is never a guess.
    pub open_workspace: String,
    pub focus: Focus,
    /// Which top-level surface is open. Client-only state: see
    /// [`views`].
    pub view: View,
    /// The views agents showed, shown by the Views stage. See `agent_views`.
    pub agent_views: AgentViews,
    /// The saved views the daemon holds, shown by the same stage. See `saved_views`.
    pub saved_views: SavedViews,
    /// Where focus was in the workspace when another view took the stage,
    /// so coming back lands where you left rather than at the root.
    workspace_focus: Focus,
    pub sel_project: usize,
    pub sel_repository: usize,
    pub sel_checkout: usize,
    pub sel_pane: usize,
    /// Every pane being streamed, and the screen of each. More than one,
    /// because a floating editor must not cost you sight of the agent
    /// running behind it.
    pub grids: std::collections::HashMap<PaneId, PaneView>,
    pub leader_pending: bool,
    /// Whether the selected pane temporarily owns the main content area.
    /// This is view state only; the renderer's existing live-pane sizing
    /// turns the larger area into a PTY resize.
    pub pane_fullscreen: bool,
    /// How the clipboard is read. A field so a test can hand the app a
    /// clipboard without there being a desktop session to hold one.
    pub clipboard: fn() -> Option<String>,
    /// How the clipboard's image is read, as a PNG; injectable the same way.
    pub clipboard_image: fn() -> Option<Vec<u8>>,
    /// How selected terminal text is written, injectable for tests without
    /// a desktop session.
    pub clipboard_write: fn(&str) -> bool,
    /// How a ticket's URL is opened, injectable so a test never starts the
    /// desk's browser.
    pub open_url: fn(&str) -> crate::system::browser::Opened,
    /// Whether the daemon takes a file off this machine (`PASTE_FILE`).
    /// False until it greets, and for a daemon from before the capability,
    /// which is told about rather than sent a message it cannot read.
    pub pastes_files: bool,
    /// Whether the daemon takes this client's view (`FOCUS`), and the
    /// view it was last sent. See `daemon::showing`.
    reports_view: bool,
    view_sent: Option<argus_protocol::View>,
    /// Whether the terminal holds the person's focus. True until it says
    /// otherwise, which a terminal that reports no focus never does.
    terminal_focused: bool,
    pub selection: Option<TerminalSelection>,
    /// True between a press on the rail's right border and its release.
    pub rail_drag: bool,
    pub should_quit: bool,
    /// The last thing worth saying on the status bar, and whether it is
    /// something the user *must* read. The rank rides along rather than
    /// being guessed from the words, because it decides both the color and
    /// whether the message outranks the keymap for space. Set through
    /// [`App::report`] and [`App::alert`].
    pub status: String,
    pub status_alert: bool,
    pub layout: Layout,
    /// True while the checkouts column also lists the branches nothing is
    /// sitting on. Off by default — the column is for what is running, and
    /// the main branch is pinned to the top of it either way.
    pub show_branches: bool,
    /// Live filter on checkout branch names, started with `/` on a checkout
    /// surface. Empty means every drawn row is shown.
    pub checkout_filter: String,
    /// While true, printable keys edit [`checkout_filter`] rather than
    /// firing navigation bindings.
    pub checkout_filtering: bool,
    /// The repositories whose checkouts and panes the rail shows, as the
    /// operator opened them. A selection alone does not open a repository:
    /// the rail stays useful as an index until asked for more. Read through
    /// [`App::is_repository_open`].
    opened_repositories: std::collections::HashSet<RepositoryId>,
    /// The checkouts the operator folded, their panes off the rail. Open is
    /// the default, the other way from repositories: a checkout is listed
    /// because something runs in it. Read through [`App::is_checkout_open`].
    folded_checkouts: std::collections::HashSet<CheckoutId>,
    pub picker: Option<Picker>,
    /// The directory browser, up in place of a prompt when a project or a
    /// repository is being added.
    pub dir_picker: Option<DirPicker>,
    pub overlay: Option<Overlay>,
    /// The keymap window. Its own field rather than an [`Overlay`] variant
    /// because `?` has to work *over* whatever is already open — a review
    /// you asked the keys about is still there when you close them.
    pub help: Option<Help>,
    pub settings: crate::settings::Settings,
    /// False for an app that must not write to the user's config — every
    /// test, and anything constructed with [`App::new`].
    persist_settings: bool,
    /// The review asked for or on screen; what it holds and asks is the
    /// session's, shared with the desktop window.
    pub review: Option<ReviewSession>,
    pub history: Option<HistoryView>,
    /// The feature or task brief being read or written, if one is open.
    pub brief: Option<BriefView>,
    /// The board of the repository the Feature view reads, and the task
    /// and diagram lists of the feature on screen: when each is asked for
    /// and which pushes are kept, by the rule the desktop window follows
    /// too. Held on the app rather than created with the view, because the
    /// daemon pushes a changed board whether or not anyone is looking at
    /// it — a tree is meant to be watched being built.
    pub board_follower: argus_protocol::BoardFollower,
    /// The decisions narrowed to the feature on screen. Kept rather than
    /// derived per frame because the rows a renderer walks borrow from it.
    pub board_scoped: Option<argus_protocol::DecisionBoard>,
    /// Which feature the whole view is on, as a place in `feature_rows`.
    ///
    /// One selection, shared by the brief, the tasks and the decisions.
    /// They were three views with a selection each, and a selection each
    /// meant they could disagree about which feature you were reading —
    /// which is exactly what they did.
    pub feature_sel: usize,
    /// Whether the held and archived features are listed. Neither is work
    /// to pick up, so the index folds them away until asked.
    pub show_folded_features: bool,
    /// Whether the stage is inside the selected feature rather than on the
    /// index of them.
    pub feature_open: bool,
    /// How the open feature is being read.
    pub lens: Lens,
    /// Whether the keys are in the detail column beside the list, and
    /// where its cursor is among a task's criteria or a question's options.
    pub in_detail: bool,
    pub detail_sel: usize,
    /// The cursor in the work lens's outline, the record's decision tree,
    /// and the selected flow's steps.
    pub work_sel: usize,
    pub decision_sel: usize,
    pub step_sel: usize,
    /// The tasks the outline has folded shut, by id.
    pub folded: std::collections::HashSet<i64>,
    /// The note a feature is being written as, when one is.
    pub capture: Option<BriefView>,
    /// The selected flow as last drawn, kept so a frame does not render
    /// Mermaid again to draw what has not changed.
    pub flow_drawn: Option<(i64, layers::diagram::DiagramView)>,
    pub diagram_sel: usize,
    /// The sequence diagram open in the overlay, if any.
    pub diagram: Option<layers::diagram::DiagramView>,
    /// The line being typed in the feature view, when one is. `Some` is
    /// what makes the view swallow keys — a view whose `x` deletes a row
    /// while you are typing a title with an x in it is not usable.
    pub line: Option<LineInput>,
    /// The id the next review request goes out with; the session matches
    /// its answers.
    next_review_request: u64,
    /// What the outstanding request was for; a list for anything else is
    /// stale and dropped.
    history_request: Outstanding<CheckoutId>,
    /// Same, for a branch or file list.
    list_wanted: Option<CheckoutId>,
    next_browse_request: u64,
    /// Sticky across reopens, so `b` is a setting rather than a per-visit
    /// choice.
    pub review_base: ReviewBase,
    /// Where closing a review or history puts the keys: back on the
    /// Checkouts stage when it was opened from there, else the rail.
    review_return: Focus,
    /// Whether review pairs the two sides of a change rather than stacking
    /// them. Held here for whichever view opens next; the open view carries
    /// its own copy, because its rows are built from it.
    pub review_split: bool,
    pub prompt: Option<Prompt>,
    /// Active color theme. Every color the UI draws comes from here, so a
    /// preset swap is one assignment rather than a sweep of call sites.
    pub theme: Theme,
    /// Requests waiting on something the daemon is making, matched by the
    /// id it answers with.
    awaited: daemon::awaited::Awaited,
    /// The instant the frame being drawn is for, and the epoch every
    /// repeating animation is phased off.
    ///
    /// A frame reads one clock rather than each widget calling
    /// `Instant::now` as it draws — otherwise two spinners in the same
    /// frame can land on different glyphs, and a render test has nothing
    /// to hold still. `main` sets it before each draw; the tests set it
    /// to pin an animation mid-flight.
    frame_now: std::time::Instant,
    epoch: std::time::Instant,
    /// The focus the last frame was drawn for, and the card focus is
    /// travelling away from while it moves.
    ///
    /// Kept here rather than set wherever `focus` is assigned: focus moves
    /// from nav, mouse, actions and input, and a fade that has to be
    /// remembered at 38 call sites is one somebody forgets. Comparing
    /// against what was last drawn cannot be forgotten.
    focus_shown: Focus,
    focus_from: Option<(Focus, crate::ui::motion::Animation)>,
    bell_pending: bool,
    out: UnboundedSender<ClientMsg>,
}

impl App {
    /// The diff on screen, once its side has arrived.
    pub fn review_view(&self) -> Option<&ReviewView> {
        self.review.as_ref().and_then(ReviewSession::view)
    }

    pub fn review_view_mut(&mut self) -> Option<&mut ReviewView> {
        self.review.as_mut().and_then(ReviewSession::view_mut)
    }

    /// Defaults, and nothing written to disk — an app that cannot touch
    /// the user's real preferences. `main` uses [`App::with_settings`].
    #[cfg(test)]
    pub fn new(out: UnboundedSender<ClientMsg>) -> Self {
        App::build(out, crate::settings::Settings::default(), false)
    }

    /// The real thing: preferences loaded from disk, and changes saved back.
    pub fn with_settings(
        out: UnboundedSender<ClientMsg>,
        settings: crate::settings::Settings,
    ) -> Self {
        App::build(out, settings, true)
    }

    fn build(
        out: UnboundedSender<ClientMsg>,
        settings: crate::settings::Settings,
        persist: bool,
    ) -> Self {
        // The environment still wins, so a one-off `ARGUS_THEME=latte argus`
        // works without editing anything.
        let theme = match std::env::var("ARGUS_THEME") {
            Ok(_) => Theme::from_env(),
            Err(_) => Theme::by_name(&settings.theme),
        };
        let review_split = settings.review_split;
        let started = std::time::Instant::now();
        // Hoisted so the first frame is drawn already settled on it,
        // rather than fading in from wherever the default sat.
        let focus = Focus::Projects;
        App {
            host: None,
            on_screen: true,
            attached_hosts: vec![AttachedHost {
                host: None,
                connected: true,
            }],
            host_request: None,
            tree: Vec::new(),
            consoles: Vec::new(),
            consoles_offered: false,
            console: None,
            templates: Vec::new(),
            workspaces: Vec::new(),
            open_workspace: String::new(),
            // A remembered folded-away tab is not a focus target, so a
            // restart from that state lands a column further in.
            focus,
            view: View::default(),
            agent_views: AgentViews::default(),
            saved_views: SavedViews::default(),
            workspace_focus: Focus::Panes,
            sel_project: 0,
            sel_repository: 0,
            sel_checkout: 0,
            sel_pane: 0,
            grids: std::collections::HashMap::new(),
            leader_pending: false,
            pane_fullscreen: false,
            clipboard: crate::system::clipboard::read,
            clipboard_image: crate::system::clipboard::read_image,
            clipboard_write: crate::system::clipboard::write,
            open_url: crate::system::browser::open,
            pastes_files: false,
            reports_view: false,
            view_sent: None,
            terminal_focused: true,
            selection: None,
            rail_drag: false,
            should_quit: false,
            // Empty, not a keymap: the bar's left half is the breadcrumb's
            // until something has actually happened to report.
            status: String::new(),
            status_alert: false,
            layout: Layout::default(),
            show_branches: false,
            checkout_filter: String::new(),
            checkout_filtering: false,
            opened_repositories: std::collections::HashSet::new(),
            folded_checkouts: std::collections::HashSet::new(),
            picker: None,
            dir_picker: None,
            overlay: None,
            help: None,
            settings,
            persist_settings: persist,
            review: None,
            history: None,
            brief: None,
            board_follower: argus_protocol::BoardFollower::new(true),
            board_scoped: None,
            feature_sel: 0,
            show_folded_features: false,
            feature_open: false,
            lens: Lens::default(),
            in_detail: false,
            detail_sel: 0,
            work_sel: 0,
            decision_sel: 0,
            step_sel: 0,
            folded: Default::default(),
            capture: None,
            flow_drawn: None,
            diagram_sel: 0,
            diagram: None,
            line: None,
            next_review_request: 1,
            history_request: Outstanding::new(),
            list_wanted: None,
            next_browse_request: 1,
            review_base: ReviewBase::Unstaged,
            review_return: Focus::Checkouts,
            review_split,
            prompt: None,
            theme,
            awaited: daemon::awaited::Awaited::default(),
            frame_now: started,
            epoch: started,
            focus_shown: focus,
            focus_from: None,
            bell_pending: false,
            out,
        }
    }
}

#[cfg(test)]
mod tests;
