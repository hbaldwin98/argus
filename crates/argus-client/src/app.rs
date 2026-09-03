//! The client's model: everything on screen is a function of this type.
//!
//! `App` holds the tree the daemon sent, where the selection is, which
//! modal is up, and the streamed screen of every pane being drawn. It
//! never predicts the result of a request — a row moves when the tree
//! comes back saying it moved — so a refused action leaves the panel
//! showing what is actually true.
//!
//! The `impl` is split by what the operator is doing: `nav` for moving
//! between columns, `input` for keys, `mouse` for the pointer, `scroll`
//! for a pane parked in its history, `actions` for what is asked of the
//! daemon, `pickers` for the modal layers, and `server` for what arrives
//! back.

use argus_protocol::{
    CheckoutId, CheckoutInfo, ClientMsg, NoteTarget, PaneId, PaneInfo, PaneKind, PaneStatus,
    ProjectId, ProjectInfo, RepositoryId, RepositoryInfo, ReviewAnchor, ServerMsg, WorkspaceId,
    WorkspaceInfo,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use crate::dirpicker::{DirAction, DirPicker, DirTarget};
use crate::fuzzy::Fuzzy;
use crate::grid::Grid;
use crate::history::{Drill, HistoryView};
use crate::pty_input::{encode_key, encode_mouse, is_leader};
use crate::notes::{NoteMode, NoteView};
use crate::review::ReviewView;
use crate::theme::Theme;

pub use layout::{Focus, Fold, Layout, Panel};
pub use modal::{Help, Overlay, Picker, PickerKind, Prompt, RemoveTarget, Setting};
pub use rows::{CheckoutAnchor, CheckoutRow, PaneLocation};
pub use views::View;
use layout::{in_rect, row_in};
use argus_protocol::ReviewBase;

mod actions;
mod input;
mod layout;
mod modal;
mod mouse;
mod nav;
mod pickers;
mod rows;
mod scroll;
mod server;
mod views;

const STATE_FLASH: std::time::Duration = std::time::Duration::from_millis(900);

/// The branch a remote-tracking name would become locally:
/// `origin/feature/x` → `feature/x`. Remote names have no slash in them,
/// so the first one is the split.
fn local_name(remote_branch: &str) -> Option<&str> {
    remote_branch.split_once('/').map(|(_, rest)| rest)
}

/// Whether this checkout is the one sitting on `branch`. Its git status is
/// the truth; the row's name stands in only until the first poll has been
/// round.
fn on_branch(c: &CheckoutInfo, branch: &str) -> bool {
    c.git
        .as_ref()
        .and_then(|g| g.branch.as_deref())
        .unwrap_or(&c.name)
        == branch
}

/// The tree flattened to its checkouts, and to its panes. Written once
/// because half a dozen questions are one of these plus a `find`.
fn checkouts_in(tree: &[ProjectInfo]) -> impl Iterator<Item = &CheckoutInfo> {
    tree.iter()
        .flat_map(|project| project.repositories.iter())
        .flat_map(|repository| repository.checkouts.iter())
}

fn panes_in(tree: &[ProjectInfo]) -> impl Iterator<Item = &PaneInfo> {
    checkouts_in(tree).flat_map(|checkout| checkout.panes.iter())
}

/// An agent pane that can still be spoken to: a shell has nothing to hear
/// a review comment, and an exited agent no longer has anywhere to put it.
fn is_live_agent_pane(pane: &PaneInfo) -> bool {
    pane.kind == PaneKind::Agent && !matches!(pane.status, PaneStatus::Exited { .. })
}

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

/// The pane's effective state includes nested agents, because their parent
/// row is the only selectable destination the client can flash or open.
fn effective_state(pane: &PaneInfo) -> (PaneStatus, &str, Option<&str>) {
    let mut effective = (pane.status, pane.title.as_str(), pane.note.as_deref());
    for child in &pane.children {
        if child.status.urgency() > effective.0.urgency() {
            effective = (child.status, child.label.as_str(), child.note.as_deref());
        }
    }
    effective
}

fn effective_label(pane: &PaneInfo, label: &str) -> String {
    if label == pane.title {
        label.to_string()
    } else {
        format!("{} / {label}", pane.title)
    }
}

fn attention_of(pane: &PaneInfo) -> Option<(String, Option<String>)> {
    let (status, label, note) = effective_state(pane);
    status
        .needs_you()
        .then(|| (effective_label(pane, label), note.map(str::to_string)))
}

pub struct App {
    pub tree: Vec<ProjectInfo>,
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
    /// Where focus was on the spine when another view took the screen, so
    /// coming back lands on the column you left rather than at the root.
    spine_focus: Focus,
    pub sel_project: usize,
    pub sel_repository: usize,
    pub sel_checkout: usize,
    pub sel_pane: usize,
    /// Every pane being streamed, and the screen of each. More than one,
    /// because a floating editor must not cost you sight of the agent
    /// running behind it.
    pub grids: std::collections::HashMap<PaneId, Grid>,
    pub leader_pending: bool,
    /// Whether the selected pane temporarily owns the main content area.
    /// This is view state only; the renderer's existing live-pane sizing
    /// turns the larger area into a PTY resize.
    pub pane_fullscreen: bool,
    /// How the clipboard is read. A field so a test can hand the app a
    /// clipboard without there being a desktop session to hold one.
    pub clipboard: fn() -> Option<String>,
    pub should_quit: bool,
    /// The last thing worth saying on the status bar, and whether it is
    /// something the user *must* read. The rank rides along rather than
    /// being guessed from the words, because it decides both the color and
    /// whether the message outranks the keymap for space. Set through
    /// [`App::report`] and [`App::alert`].
    pub status: String,
    pub status_alert: bool,
    pub layout: Layout,
    /// Preferred outer widths for the five main columns. `None` uses the
    /// initial proportional layout; dragging a gutter captures concrete
    /// widths so the adjustment survives subsequent frames.
    pub column_widths: Option<Vec<u16>>,
    /// True when the projects column is folded away to a left-edge tab.
    /// Stored both here (for the renderer) and on `settings` (so it persists).
    pub fold: Fold,
    /// True while the checkouts column also lists the branches nothing is
    /// sitting on. Off by default — the column is for what is running, and
    /// the main branch is pinned to the top of it either way.
    pub show_branches: bool,
    resizing_gutter: Option<usize>,
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
    /// The next pane the daemon tells us about should open in an overlay.
    /// Set when an editor is spawned for one.
    pending_overlay_new: bool,
    pub review: Option<ReviewView>,
    pub history: Option<HistoryView>,
    /// The note being read or written, if one is open.
    pub notes: Option<NoteView>,
    /// What the outstanding request was for; a diff for anything else is
    /// stale and dropped.
    review_wanted: Option<(CheckoutId, u64)>,
    next_review_request: u64,
    history_wanted: Option<(CheckoutId, u64)>,
    next_history_request: u64,
    /// Jump here once a commit review lands, when Enter was on a file row.
    pending_history_file: Option<String>,
    /// Same, for a branch or file list.
    list_wanted: Option<CheckoutId>,
    next_browse_request: u64,
    /// Sticky across reopens, so `b` is a setting rather than a per-visit
    /// choice.
    pub review_base: ReviewBase,
    /// Whether review pairs the two sides of a change rather than stacking
    /// them. Held here for whichever view opens next; the open view carries
    /// its own copy, because its rows are built from it.
    pub review_split: bool,
    pub prompt: Option<Prompt>,
    /// Active color theme. Every color the UI draws comes from here, so a
    /// preset swap is one assignment rather than a sweep of call sites.
    pub theme: Theme,
    pending_focus_new: bool,
    pending_focus_new_checkout: Option<RepositoryId>,
    pending_focus_new_project: bool,
    /// The project a just-added repository belongs to, so the new row is
    /// the selected one when the tree carrying it arrives.
    pending_focus_new_repository: Option<ProjectId>,
    /// A short shape-preserving highlight after an effective parent or child
    /// state changes. The client derives this from consecutive snapshots;
    /// the first snapshot on attach is only a baseline.
    state_flashes: std::collections::HashMap<PaneId, std::time::Instant>,
    bell_pending: bool,
    out: UnboundedSender<ClientMsg>,
}

impl App {
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
        let column_widths = settings
            .column_widths
            .clone()
            .filter(|widths| widths.len() == 5);
        let review_split = settings.review_split;
        App {
            tree: Vec::new(),
            templates: Vec::new(),
            workspaces: Vec::new(),
            open_workspace: String::new(),
            // A remembered folded-away tab is not a focus target, so a
            // restart from that state lands a column further in.
            focus: if settings.fold().hides(Focus::Projects) {
                Focus::Repositories
            } else {
                Focus::Projects
            },
            view: View::default(),
            spine_focus: Focus::Panes,
            sel_project: 0,
            sel_repository: 0,
            sel_checkout: 0,
            sel_pane: 0,
            grids: std::collections::HashMap::new(),
            leader_pending: false,
            pane_fullscreen: false,
            clipboard: crate::clipboard::read,
            should_quit: false,
            // Empty, not a keymap: the bar's left half is the breadcrumb's
            // until something has actually happened to report.
            status: String::new(),
            status_alert: false,
            layout: Layout::default(),
            column_widths,
            fold: settings.fold(),
            show_branches: false,
            resizing_gutter: None,
            picker: None,
            dir_picker: None,
            overlay: None,
            help: None,
            settings,
            persist_settings: persist,
            pending_overlay_new: false,
            review: None,
            history: None,
            notes: None,
            review_wanted: None,
            next_review_request: 1,
            history_wanted: None,
            next_history_request: 1,
            pending_history_file: None,
            list_wanted: None,
            next_browse_request: 1,
            review_base: ReviewBase::Unstaged,
            review_split,
            prompt: None,
            theme,
            pending_focus_new: false,
            pending_focus_new_checkout: None,
            pending_focus_new_project: false,
            pending_focus_new_repository: None,
            state_flashes: std::collections::HashMap::new(),
            bell_pending: false,
            out,
        }
    }
}


#[cfg(test)]
mod tests;
