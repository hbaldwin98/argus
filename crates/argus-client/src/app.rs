use argus_protocol::{
    CheckoutId, CheckoutInfo, ClientMsg, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId,
    ProjectInfo, RepositoryId, RepositoryInfo, ReviewAnchor, ServerMsg, WorkspaceId, WorkspaceInfo,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use crate::dirpicker::{DirAction, DirPicker, DirTarget};
use crate::fuzzy::Fuzzy;
use crate::grid::Grid;
use crate::history::{Drill, HistoryView};
use crate::keys::{encode_key, is_leader};
use crate::mouse::encode_mouse;
use crate::review::ReviewView;
use crate::theme::Theme;
use argus_protocol::ReviewBase;

mod actions;
mod input;
mod mouse;
mod nav;
mod pickers;
mod server;

const STATE_FLASH: std::time::Duration = std::time::Duration::from_millis(900);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    Repositories,
    Checkouts,
    Panes,
    PaneContent,
    /// A mode of the rightmost column, not a fifth column of its own.
    Review,
    /// A floating window over everything else — see [`Overlay`].
    Overlay,
}

/// One rendered panel: the whole card, and the padded area its rows live
/// in. Both are needed — a click on a row selects it, but a click anywhere
/// else on the card still moves focus there.
#[derive(Debug, Clone, Copy, Default)]
pub struct Panel {
    pub outer: Rect,
    pub inner: Rect,
    /// The list index drawn on this card's first row. A column taller than
    /// its card is scrolled, and then a row on screen is not the row's
    /// index: everything mapping a click back to a row has to add this,
    /// and the next frame scrolls from here rather than recomputing an
    /// offset from the selection alone.
    pub first: usize,
}

/// Screen regions from the most recent render, so mouse clicks can be
/// mapped back onto tree rows / pane cells without duplicating layout math.
#[derive(Debug, Clone, Copy, Default)]
pub struct Layout {
    pub projects: Panel,
    pub repositories: Panel,
    pub checkouts: Panel,
    pub panes: Panel,
    pub content: Panel,
    /// Zero-sized when no overlay is up.
    pub overlay: Panel,
    /// Where the last frame put the hardware cursor, `None` when it hid it.
    /// Recorded as well as applied so the decision — which is one decision
    /// for the whole frame, made across several layers — can be asserted on.
    pub cursor: Option<crate::ui::CursorPlacement>,
}

/// What confirming a picker selection does. The picker is one widget with
/// one set of keys; this is the only thing that differs between uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerKind {
    /// Spawn the chosen agent template in the selected checkout.
    Agent,
    /// Switch to the chosen workspace. Ids ride along per row, and the
    /// bare names beside them: the rows themselves carry rollup counts, so
    /// they are not what "does this name already exist" can be asked of.
    Workspace {
        ids: Vec<WorkspaceId>,
        names: Vec<String>,
    },
    /// Switch the color theme.
    Theme,
    /// `git switch` the current checkout to the chosen branch. The last
    /// row is a synthetic "create" entry when the query names no existing
    /// branch, so making one is the same gesture as picking one.
    Branch { checkout: CheckoutId },
    /// Open the chosen file in the user's editor.
    File { checkout: CheckoutId },
    /// Jump the review cursor to the chosen changed file.
    Change,
    /// Send a prepared review comment to one live agent pane.
    ReviewRecipient {
        panes: Vec<PaneId>,
        checkout: CheckoutId,
        anchor: ReviewAnchor,
        body: String,
    },
}

impl PickerKind {
    /// Whether typing filters the list. The short lists don't need it, and
    /// a query line over four themes would be clutter.
    pub fn is_fuzzy(&self) -> bool {
        matches!(
            self,
            PickerKind::Branch { .. }
                | PickerKind::File { .. }
                | PickerKind::Change
                | PickerKind::Workspace { .. }
        )
    }

    /// Paths score differently from plain words.
    fn matcher(&self) -> Fuzzy {
        match self {
            PickerKind::File { .. } | PickerKind::Change => Fuzzy::paths(),
            _ => Fuzzy::new(),
        }
    }
}

pub struct Picker {
    pub kind: PickerKind,
    pub title: &'static str,
    /// Everything on offer, in the order the daemon sent it.
    pub items: Vec<String>,
    /// What the user has typed, on a fuzzy picker.
    pub query: String,
    /// Indices into `items`, best match first. `sel` indexes into *this*.
    pub shown: Vec<usize>,
    pub sel: usize,
    /// An extra row offered below the matches — creating the branch you
    /// just typed the name of.
    pub create: Option<String>,
}

impl Picker {
    pub fn new(kind: PickerKind, title: &'static str, items: Vec<String>, sel: usize) -> Self {
        let shown = (0..items.len()).collect();
        Picker {
            kind,
            title,
            items,
            query: String::new(),
            shown,
            sel,
            create: None,
        }
    }

    /// The item under the cursor, or `None` when the cursor is on the
    /// create row.
    pub fn selected(&self) -> Option<&str> {
        let idx = *self.shown.get(self.sel)?;
        self.items.get(idx).map(String::as_str)
    }

    /// How many rows are on screen, the create row included.
    pub fn len(&self) -> usize {
        self.shown.len() + usize::from(self.create.is_some())
    }

    pub fn is_fuzzy(&self) -> bool {
        self.kind.is_fuzzy()
    }

    /// Sets the query and re-filters. For tests and dumps; the app itself
    /// goes through the key handler.
    #[cfg(test)]
    pub fn type_query(&mut self, q: &str) {
        self.query = q.to_string();
        self.refilter();
    }

    fn on_create_row(&self) -> bool {
        self.create.is_some() && self.sel == self.shown.len()
    }

    /// Re-filters after a keystroke, keeping the cursor in range. The
    /// cursor goes back to the top: after a new query the old position
    /// refers to a row that is no longer there.
    fn refilter(&mut self) {
        let mut matcher = self.kind.matcher();
        // Workspace rows carry rollup counts, so they are matched on their
        // names — otherwise typing a digit would "find" a workspace by how
        // many panes it happens to be running.
        self.shown = match &self.kind {
            PickerKind::Workspace { names, .. } => matcher.filter(&self.query, names),
            _ => matcher.filter(&self.query, &self.items),
        };

        // Offering to create a branch that already exists would be a
        // second, worse way to switch to it.
        self.create = match &self.kind {
            PickerKind::Branch { .. } => {
                let q = self.query.trim();
                (!q.is_empty() && !self.items.iter().any(|b| b == q)).then(|| q.to_string())
            }
            PickerKind::Workspace { names, .. } => {
                let q = self.query.trim();
                (!q.is_empty() && !names.iter().any(|n| n == q)).then(|| q.to_string())
            }
            _ => None,
        };
        self.sel = 0;
    }
}

/// A window floating above the columns, for things the five-column spine
/// has no room for. Unlike a picker it can be large and can hold a live
/// pane: a terminal editor in a 38%-wide column is unusable, and the whole
/// point of `$EDITOR` support is that it be usable.
///
/// It floats rather than replacing the columns — the tree stays visible
/// around the edges, which is the same rule every other view here follows.
pub enum Overlay {
    /// A pty pane, drawn large. The title is carried rather than looked up
    /// so it survives the pane leaving the tree.
    Pane {
        pane: PaneId,
        title: String,
        /// Kill the pane when the window closes. True for editors: they
        /// are not listed in the panes column, so a surviving one would be
        /// a process with no window and no way back to it.
        ephemeral: bool,
    },
    /// Preferences, with room to say what each one does.
    Settings { sel: usize },
    /// The diff. The view itself lives on `App::review`; this only says
    /// which window is up. Floating rather than in the column so reading a
    /// diff never costs you sight of the agent that produced it.
    Review,
    /// Recent commits and the files each changed. Same window as review;
    /// opening a commit replaces this with [`Overlay::Review`] without
    /// dropping the list, so going back is instant.
    History,
}

impl Overlay {
    fn pane(&self) -> Option<PaneId> {
        match self {
            Overlay::Pane { pane, .. } => Some(*pane),
            Overlay::Settings { .. } | Overlay::Review | Overlay::History => None,
        }
    }
}

/// The rows of the settings panel, in the order they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Editor,
    EditorCmd,
    Theme,
    Notifications,
}

impl Setting {
    pub const ALL: &'static [Setting] = &[
        Setting::Editor,
        Setting::EditorCmd,
        Setting::Theme,
        Setting::Notifications,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Setting::Editor => "editor opens",
            Setting::EditorCmd => "editor command",
            Setting::Theme => "theme",
            Setting::Notifications => "notifications",
        }
    }
}

/// What a `ConfirmRemove` prompt is about to take away. A checkout's
/// removal deletes its worktree and branch, and a branch's deletes the
/// branch; the other two only stop showing something, leaving every file
/// where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveTarget {
    Checkout(CheckoutId),
    Repository(RepositoryId),
    Project(ProjectId),
    /// A branch with no directory of its own. It carries the checkout git
    /// will be run from, precisely because the branch hasn't got one.
    Branch {
        checkout: CheckoutId,
        branch: String,
    },
}

impl RemoveTarget {
    fn message(&self) -> ClientMsg {
        match self {
            RemoveTarget::Checkout(checkout) => ClientMsg::RemoveCheckout {
                checkout: *checkout,
            },
            RemoveTarget::Repository(repository) => ClientMsg::RemoveRepository {
                repository: *repository,
            },
            RemoveTarget::Project(project) => ClientMsg::RemoveProject { project: *project },
            RemoveTarget::Branch { checkout, branch } => ClientMsg::DeleteBranch {
                checkout: *checkout,
                branch: branch.clone(),
            },
        }
    }

    /// Popup title and the line under the name — what the user is agreeing
    /// to, which for two of the four is "nothing on disk".
    pub fn wording(&self) -> (&'static str, &'static str) {
        match self {
            RemoveTarget::Checkout(_) => {
                ("remove checkout?", "  — worktree, branch, and its panes")
            }
            RemoveTarget::Repository(_) => {
                ("remove repository?", "  — from this panel only; files stay")
            }
            RemoveTarget::Project(_) => ("remove project?", "  — from this panel only; files stay"),
            RemoveTarget::Branch { .. } => (
                "delete branch?",
                "  — the local branch only; the remote is untouched",
            ),
        }
    }
}

/// A modal text/confirm prompt, mutually exclusive with `Picker`. Both new
/// worktree (free text) and remove (yes/no) go through this so
/// there's one input path and one place `on_mouse` has to know to ignore.
pub enum Prompt {
    NewWorktree {
        base: CheckoutId,
        input: String,
    },
    ConfirmRemove {
        target: RemoveTarget,
        label: String,
    },
    Comment {
        anchor: ReviewAnchor,
        input: String,
    },
    /// The editor command, typed rather than cycled — it is free text.
    EditorCommand {
        input: String,
    },
}

/// One row of the checkouts column, as an index into the repository's own
/// `checkouts` or `branches`. The two kinds interleave — the main branch
/// leads the column whichever it turns out to be — so the column's order is
/// [`App::checkout_rows`] rather than either list on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutRow {
    Checkout(usize),
    Branch(usize),
    /// A branch that exists on a remote and nowhere else — an index into
    /// `remote_branches`, which holds them as `origin/feature`.
    Remote(usize),
}

/// What the checkouts column had selected, as an identity rather than a
/// position. Row indices shift under the column whenever a branch row
/// appears or disappears above the selection, so a tree arriving from the
/// daemon has to be able to find the same row again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutAnchor {
    Checkout(CheckoutId),
    /// A branch with no directory, held by name: the checkout that would
    /// give it one is the same row as far as the user is concerned.
    Branch(String),
    Remote(String),
}

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

fn panes_in(tree: &[ProjectInfo]) -> impl Iterator<Item = &PaneInfo> {
    tree.iter()
        .flat_map(|project| project.repositories.iter())
        .flat_map(|repository| repository.checkouts.iter())
        .flat_map(|checkout| checkout.panes.iter())
}

fn state_rank(status: PaneStatus) -> u8 {
    match status {
        PaneStatus::Exited { code: Some(0) } => 0,
        PaneStatus::Idle => 1,
        PaneStatus::Done => 2,
        PaneStatus::Working => 3,
        PaneStatus::Exited { .. } => 4,
        PaneStatus::NeedsReview => 5,
        PaneStatus::Failed => 6,
        PaneStatus::Waiting => 7,
    }
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
        if state_rank(child.status) > state_rank(effective.0) {
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
    /// True when the projects column is collapsed to a thin strip. Stored
    /// both here (for the renderer) and on `settings` (so it persists).
    pub projects_collapsed: bool,
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
    pub settings: crate::settings::Settings,
    /// False for an app that must not write to the user's config — every
    /// test, and anything constructed with [`App::new`].
    persist_settings: bool,
    /// The next pane the daemon tells us about should open in an overlay.
    /// Set when an editor is spawned for one.
    pending_overlay_new: bool,
    pub review: Option<ReviewView>,
    pub history: Option<HistoryView>,
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
        App {
            tree: Vec::new(),
            templates: Vec::new(),
            workspaces: Vec::new(),
            open_workspace: String::new(),
            // A remembered collapsed strip is not a focus target, so a
            // restart from that state lands a column further in.
            focus: if settings.projects_collapsed {
                Focus::Repositories
            } else {
                Focus::Projects
            },
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
            projects_collapsed: settings.projects_collapsed,
            show_branches: false,
            resizing_gutter: None,
            picker: None,
            dir_picker: None,
            overlay: None,
            settings,
            persist_settings: persist,
            pending_overlay_new: false,
            review: None,
            history: None,
            review_wanted: None,
            next_review_request: 1,
            history_wanted: None,
            next_history_request: 1,
            pending_history_file: None,
            list_wanted: None,
            next_browse_request: 1,
            review_base: ReviewBase::Unstaged,
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

/// Which list row a point falls on. Rows are [`crate::ui::ROW_HEIGHT`]
/// lines tall, and either of an item's lines counts as that item.
fn row_in(area: Rect, x: u16, y: u16) -> Option<usize> {
    if !in_rect(area, x, y) {
        return None;
    }
    Some(((y - area.y) / crate::ui::ROW_HEIGHT) as usize)
}

fn in_rect(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{
        Cell, CellSpan, CheckoutId, GitStatus, PaneKind, PaneStatus, ProjectId, RepositoryId,
        RepositoryInfo,
    };
    use argus_protocol::{DirEntry, DirListing};
    use crossterm::event::KeyModifiers;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    fn pane(id: u64, title: &str) -> PaneInfo {
        PaneInfo {
            id: PaneId(id),
            kind: PaneKind::Shell,
            title: title.to_string(),
            status: PaneStatus::Idle,
            note: None,
            template: None,
            children: Vec::new(),
        }
    }

    fn checkout(id: u64, name: &str, primary: bool, panes: Vec<PaneInfo>) -> CheckoutInfo {
        CheckoutInfo {
            id: CheckoutId(id),
            name: name.to_string(),
            path: format!("/repo/{name}"),
            primary,
            git: None,
            panes,
        }
    }

    fn repository(id: u64, name: &str, checkouts: Vec<CheckoutInfo>) -> RepositoryInfo {
        RepositoryInfo {
            id: RepositoryId(id),
            name: name.to_string(),
            checkouts,
            branches: Vec::new(),
            default_branch: None,
            remote_branches: Vec::new(),
        }
    }

    /// Two projects; the first has a primary checkout with two panes and a
    /// linked worktree with none, the second has a single empty checkout.
    fn tree() -> Vec<ProjectInfo> {
        vec![
            ProjectInfo {
                id: ProjectId(1),
                name: "argus".to_string(),
                repositories: vec![repository(
                    5,
                    "orion",
                    vec![
                        checkout(
                            10,
                            "master",
                            true,
                            vec![pane(100, "shell"), pane(101, "claude")],
                        ),
                        checkout(11, "feat", false, vec![]),
                    ],
                )],
            },
            ProjectInfo {
                id: ProjectId(2),
                name: "other".to_string(),
                repositories: vec![repository(
                    6,
                    "other-repo",
                    vec![checkout(20, "main", true, vec![])],
                )],
            },
        ]
    }

    struct Harness {
        app: App,
        rx: UnboundedReceiver<ClientMsg>,
    }

    impl Harness {
        /// An app that has already received the fixture tree, with the
        /// resulting Subscribe traffic drained so tests assert only on what
        /// they themselves trigger.
        fn new() -> Self {
            let (tx, rx) = unbounded_channel();
            let mut app = App::new(tx);
            app.on_server_msg(ServerMsg::Tree(tree()));
            app.templates = vec!["claude".to_string(), "codex".to_string()];
            let mut h = Harness { app, rx };
            h.sent();
            h
        }

        fn key(&mut self, code: KeyCode) {
            self.app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        }

        fn leader(&mut self) {
            self.key(KeyCode::Null);
        }

        fn keys(&mut self, s: &str) {
            for c in s.chars() {
                self.key(KeyCode::Char(c));
            }
        }

        /// Answers the browser's outstanding listing request, the way the
        /// daemon would.
        fn browse(&mut self, path: &str, parent: Option<&str>, entries: &[(&str, bool)]) {
            let request_id = self
                .app
                .dir_picker
                .as_ref()
                .and_then(|p| p.pending)
                .expect("the browser is waiting for a listing");
            self.app.on_server_msg(ServerMsg::Directories(DirListing {
                request_id,
                path: path.to_string(),
                parent: parent.map(str::to_string),
                entries: entries
                    .iter()
                    .map(|(name, is_repo)| DirEntry {
                        name: name.to_string(),
                        is_repo: *is_repo,
                    })
                    .collect(),
                error: None,
            }));
        }

        fn sent(&mut self) -> Vec<ClientMsg> {
            let mut out = Vec::new();
            while let Ok(msg) = self.rx.try_recv() {
                out.push(msg);
            }
            out
        }
    }

    // --- Miller-column navigation -----------------------------------------

    #[test]
    fn starts_focused_on_projects() {
        let h = Harness::new();
        assert_eq!(h.app.focus, Focus::Projects);
        assert_eq!(h.app.current_project().unwrap().name, "argus");
    }

    #[test]
    fn l_descends_and_h_ascends_through_every_column() {
        let mut h = Harness::new();
        for expected in [
            Focus::Repositories,
            Focus::Checkouts,
            Focus::Panes,
            Focus::PaneContent,
        ] {
            h.key(KeyCode::Char('l'));
            assert_eq!(h.app.focus, expected);
        }
        // Leaving the innermost column needs the leader chord: a bare `h`
        // there is a character typed at the child, not a navigation key.
        h.leader();
        h.key(KeyCode::Esc);
        assert_eq!(h.app.focus, Focus::Panes);
        for expected in [Focus::Checkouts, Focus::Repositories, Focus::Projects] {
            h.key(KeyCode::Char('h'));
            assert_eq!(h.app.focus, expected);
        }
    }

    #[test]
    fn ascending_past_projects_is_a_no_op() {
        let mut h = Harness::new();
        h.keys("hhh");
        assert_eq!(h.app.focus, Focus::Projects, "must not fall off the left");
    }

    #[test]
    fn cannot_descend_into_a_checkout_with_no_panes() {
        let mut h = Harness::new();
        h.keys("llj"); // checkouts column, select the linked worktree
        assert_eq!(h.app.current_checkout().unwrap().name, "feat");
        h.keys("lll");
        assert_eq!(h.app.focus, Focus::Panes, "no pane to descend into");
    }

    #[test]
    fn j_and_k_move_within_the_focused_column_only() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('j'));
        assert_eq!(h.app.sel_project, 1);
        assert_eq!(h.app.sel_checkout, 0, "other columns untouched");
        h.key(KeyCode::Char('k'));
        assert_eq!(h.app.sel_project, 0);
    }

    #[test]
    fn selection_does_not_run_off_either_end() {
        let mut h = Harness::new();
        h.keys("kkk");
        assert_eq!(h.app.sel_project, 0);
        h.keys("jjjjj");
        assert_eq!(h.app.sel_project, 1, "clamped to the last project");
    }

    #[test]
    fn descending_resets_the_child_columns_selection() {
        let mut h = Harness::new();
        h.keys("lllj"); // into panes, select the second pane
        assert_eq!(h.app.sel_pane, 1);
        h.keys("hhh"); // back to projects
        h.keys("lll"); // descend again
        assert_eq!(h.app.sel_pane, 0, "re-entering a column starts at the top");
    }

    #[test]
    fn moving_to_a_project_with_fewer_checkouts_clamps_the_selection() {
        let mut h = Harness::new();
        h.keys("llj"); // checkouts, index 1
        assert_eq!(h.app.sel_checkout, 1);
        h.app.sel_project = 1; // "other" has only one checkout
        h.key(KeyCode::Char('j'));
        assert_eq!(h.app.sel_checkout, 0, "clamped into range");
    }

    /// The bug this guards: a poll that has not cached the primary
    /// checkout's branch yet leaves `master` looking like a branch nobody
    /// is on, which pins a row above every checkout. With a bare index the
    /// cursor slid up a row on each such tree until it sat on the pinned
    /// main row, and the worktree could not be worked in at all.
    #[test]
    fn a_branch_row_appearing_above_the_selection_does_not_drag_it_off_the_worktree() {
        let mut h = Harness::new();
        h.keys("llj"); // checkouts column, the "feat" worktree
        assert_eq!(h.app.current_checkout().map(|c| c.id), Some(CheckoutId(11)));

        let mut t = tree();
        let r = &mut t[0].repositories[0];
        r.default_branch = Some("master".to_string());
        // The status sweep has not landed, so master reads as unoccupied.
        r.branches = vec!["master".to_string()];
        h.app.on_server_msg(ServerMsg::Tree(t));

        assert_eq!(
            h.app.current_checkout().map(|c| c.id),
            Some(CheckoutId(11)),
            "the cursor must stay on the worktree it was on"
        );
    }

    /// The bug this guards: `sel_checkout` is a row in the drawn column,
    /// but following the watched pane set it from the checkout's position
    /// in `checkouts`. Those agree only while a checkout is sitting on the
    /// main branch; once none is, the main branch takes a pinned row of its
    /// own and every checkout is a row lower. Watching an agent then threw
    /// the cursor onto that pinned row, where there is no checkout and so
    /// no agent — and every later tree kept it there.
    #[test]
    fn watching_an_agent_off_the_main_branch_stays_on_its_checkout() {
        let mut h = Harness::new();
        h.keys("lll");
        assert_eq!(h.app.current_pane().map(|p| p.id), Some(PaneId(100)));

        let mut t = tree();
        let r = &mut t[0].repositories[0];
        // Both checkouts have been switched off the main branch, so it is a
        // branch nobody is on and is pinned above them.
        r.default_branch = Some("dev".to_string());
        r.branches = vec!["dev".to_string()];
        h.app.on_server_msg(ServerMsg::Tree(t));

        assert_eq!(h.app.sel_checkout, 1, "the pinned branch row sits above it");
        assert_eq!(
            h.app.current_pane().map(|p| p.id),
            Some(PaneId(100)),
            "the agent being watched must still be the selected pane"
        );
    }

    #[test]
    fn n_lands_on_the_checkout_row_of_the_pane_that_needs_attention() {
        let mut h = Harness::new();
        let mut t = tree();
        let r = &mut t[0].repositories[0];
        r.default_branch = Some("dev".to_string());
        r.branches = vec!["dev".to_string()];
        r.checkouts[0].panes[1].status = PaneStatus::Waiting;
        h.app.on_server_msg(ServerMsg::Tree(t));

        h.key(KeyCode::Char('N'));

        assert_eq!(h.app.column_pane(), Some(PaneId(101)));
        assert_eq!(h.app.current_checkout().map(|c| c.id), Some(CheckoutId(10)));
    }

    #[test]
    fn a_new_worktree_is_selected_by_its_row_not_its_index() {
        let mut h = Harness::new();
        let mut t = tree();
        let r = &mut t[0].repositories[0];
        r.default_branch = Some("dev".to_string());
        r.branches = vec!["dev".to_string()];
        r.checkouts.push(checkout(12, "spike", false, vec![]));
        h.app.pending_focus_new_checkout = Some(RepositoryId(5));
        h.app.on_server_msg(ServerMsg::Tree(t));

        assert_eq!(
            h.app.current_checkout().map(|c| c.id),
            Some(CheckoutId(12)),
            "the worktree just created is the row to land on"
        );
    }

    #[test]
    fn the_checkout_selection_survives_a_checkout_added_above_it() {
        let mut h = Harness::new();
        h.keys("llj");
        assert_eq!(h.app.current_checkout().map(|c| c.id), Some(CheckoutId(11)));

        let mut t = tree();
        t[0].repositories[0]
            .checkouts
            .insert(0, checkout(12, "hotfix", false, vec![]));
        h.app.on_server_msg(ServerMsg::Tree(t));

        assert_eq!(
            h.app.current_checkout().map(|c| c.id),
            Some(CheckoutId(11)),
            "a new row above the cursor must not move it"
        );
    }

    /// A branch row is the offer of a checkout, so when one appears the
    /// user is still on the same thing and should end up inside it.
    #[test]
    fn a_selected_branch_row_is_followed_into_the_checkout_that_takes_it() {
        let mut h = Harness::new();
        h.app.show_branches = true;
        let mut t = tree();
        t[0].repositories[0].branches = vec!["spike".to_string()];
        h.app.on_server_msg(ServerMsg::Tree(t));
        h.keys("ll");
        // Rows: master, feat, then the free "spike" branch.
        h.app.sel_checkout = 2;
        assert_eq!(h.app.current_branch_row(), Some("spike"));

        let mut t = tree();
        let mut c = checkout(13, "spike", false, vec![]);
        c.git = Some(argus_protocol::GitStatus {
            branch: Some("spike".to_string()),
            dirty: false,
            changed_files: 0,
            ahead: 0,
            behind: 0,
        });
        t[0].repositories[0].checkouts.push(c);
        h.app.on_server_msg(ServerMsg::Tree(t));

        assert_eq!(
            h.app.current_checkout().map(|c| c.id),
            Some(CheckoutId(13)),
            "the branch row became a checkout; follow it in"
        );
    }

    // --- live-view subscription -------------------------------------------

    #[test]
    fn the_live_view_subscribes_to_the_selected_pane_without_descending() {
        // The rightmost column always shows a pane; it never has to take
        // over the screen for content to be visible.
        let mut h = Harness::new();
        assert_eq!(
            h.app.column_pane(),
            Some(PaneId(100)),
            "first pane, from Projects focus"
        );
        assert!(h.app.grids.contains_key(&PaneId(100)));
        assert!(h.sent().is_empty());
    }

    #[test]
    fn changing_pane_selection_unsubscribes_the_old_and_subscribes_the_new() {
        let mut h = Harness::new();
        h.keys("lllj");
        let msgs = h.sent();
        assert!(
            matches!(msgs[0], ClientMsg::Unsubscribe { pane: PaneId(100) }),
            "{msgs:?}"
        );
        assert!(
            matches!(msgs[1], ClientMsg::Subscribe { pane: PaneId(101) }),
            "{msgs:?}"
        );
        assert_eq!(h.app.column_pane(), Some(PaneId(101)));
        assert!(
            !h.app.grids.contains_key(&PaneId(100)),
            "the old grid is dropped"
        );
    }

    #[test]
    fn selecting_a_paneless_checkout_unsubscribes_and_clears_the_grid() {
        let mut h = Harness::new();
        h.keys("llj");
        assert_eq!(h.app.column_pane(), None);
        assert!(h.app.grids.is_empty(), "stale content must not linger");
        assert!(matches!(h.sent()[0], ClientMsg::Unsubscribe { .. }));
    }

    #[test]
    fn ascending_out_of_a_pane_keeps_it_subscribed() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.leader();
        h.key(KeyCode::Esc);
        assert_eq!(h.app.focus, Focus::Panes);
        assert_eq!(
            h.app.column_pane(),
            Some(PaneId(100)),
            "live view keeps showing it"
        );
        assert!(h.sent().is_empty(), "no resubscribe churn");
    }

    #[test]
    fn damage_for_an_unsubscribed_pane_is_ignored() {
        let mut h = Harness::new();
        h.app.grids.insert(
            PaneId(100),
            crate::grid::Grid::new(vec![vec![Cell::default()]]),
        );
        h.app.on_server_msg(ServerMsg::Damage {
            mouse: Default::default(),
            alternate_screen: false,
            pane: PaneId(999),
            cursor: Default::default(),
            spans: vec![CellSpan {
                row: 0,
                col: 0,
                cells: vec![Cell {
                    ch: "X".into(),
                    ..Default::default()
                }],
            }],
        });
        assert_eq!(h.app.grids[&PaneId(100)].cells[0][0].ch, " ");
    }

    #[test]
    fn a_snapshot_for_the_subscribed_pane_installs_the_grid() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::PaneSnapshot {
            mouse: Default::default(),
            alternate_screen: false,
            pane: PaneId(100),
            rows: 1,
            cols: 1,
            cells: vec![vec![Cell::default()]],
            cursor: argus_protocol::Cursor {
                row: 0,
                col: 0,
                visible: true,
                ..Default::default()
            },
        });
        assert!(h.app.grids.contains_key(&PaneId(100)));
    }

    #[test]
    fn damage_carries_the_childs_alternate_screen() {
        let mut h = Harness::new();
        h.app.grids.insert(
            PaneId(100),
            crate::grid::Grid::new(vec![vec![Cell::default()]]),
        );
        h.app.on_server_msg(ServerMsg::Damage {
            mouse: Default::default(),
            alternate_screen: true,
            pane: PaneId(100),
            cursor: Default::default(),
            spans: vec![],
        });
        assert!(h.app.grids[&PaneId(100)].alternate_screen);
    }

    // --- typing into a pane ------------------------------------------------

    #[test]
    fn keys_reach_the_child_when_inside_a_pane() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.keys("echo");
        h.key(KeyCode::Enter);

        let bytes: Vec<u8> = h
            .sent()
            .into_iter()
            .flat_map(|m| match m {
                ClientMsg::Input { pane, bytes } => {
                    assert_eq!(pane, PaneId(100));
                    bytes
                }
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(bytes, b"echo\r");
    }

    #[test]
    fn a_pointer_crossing_the_screen_is_not_a_reason_to_redraw() {
        let h = Harness::new();
        let moved = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 4,
            row: 4,
            modifiers: KeyModifiers::NONE,
        };

        assert!(h.app.mouse_is_idle(&moved));
        assert!(!h.app.mouse_is_idle(&MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            ..moved
        }));
    }

    #[test]
    fn ctrl_v_pastes_from_inside_a_pane_rather_than_reaching_the_child() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.app.clipboard = || Some("one\ntwo".to_string());

        h.app
            .on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));

        assert!(
            matches!(
                h.sent().as_slice(),
                [ClientMsg::Paste { pane: PaneId(100), text }] if text == "one\ntwo"
            ),
            "ctrl-v must not go to the child as a keystroke"
        );
    }

    #[test]
    fn ctrl_shift_v_pastes_too() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.app.clipboard = || Some("x".to_string());

        h.app.on_key(KeyEvent::new(
            KeyCode::Char('V'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));

        assert!(matches!(h.sent().as_slice(), [ClientMsg::Paste { .. }]));
    }

    #[test]
    fn the_paste_key_sends_the_clipboard_as_one_message() {
        // The point of an explicit key: no inference, and the newlines
        // stay newlines instead of arriving as a run of Enters.
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.app.clipboard = || {
            Some(
                "first
second
"
                .to_string(),
            )
        };

        h.app
            .on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));

        assert!(matches!(
            h.sent().as_slice(),
            [ClientMsg::Paste { pane: PaneId(100), text }] if text == "first
second
"
        ));
        assert!(h.app.status.contains("2 lines"), "{}", h.app.status);
    }

    #[test]
    fn the_paste_key_says_so_rather_than_failing_silently() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.app.clipboard = || None;

        h.app
            .on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));

        assert!(h.sent().is_empty(), "nothing to paste, nothing sent");
        assert!(
            h.app.status_alert,
            "a clipboard that cannot be read is worth saying"
        );
    }

    #[test]
    fn a_paste_reaches_the_child_as_one_message() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();

        h.app.on_paste("first\nsecond".to_string());

        assert!(matches!(
            h.sent().as_slice(),
            [ClientMsg::Paste { pane: PaneId(100), text }] if text == "first\nsecond"
        ));
    }

    #[test]
    fn navigation_keys_are_typed_not_interpreted_inside_a_pane() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.keys("hjkq");
        assert_eq!(h.app.focus, Focus::PaneContent, "still typing");
        assert!(!h.app.should_quit, "q must not detach from inside a pane");
        assert_eq!(h.sent().len(), 4, "all four went to the child");
    }

    #[test]
    fn leader_then_esc_leaves_the_pane_without_typing_anything() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.leader();
        assert!(h.app.leader_pending);
        assert!(h.sent().is_empty(), "the leader itself is never forwarded");
        h.app.pane_fullscreen = true;

        h.key(KeyCode::Esc);
        assert_eq!(h.app.focus, Focus::Panes);
        assert!(!h.app.leader_pending);
        assert!(
            !h.app.pane_fullscreen,
            "leaving restores the navigation columns"
        );
        assert!(h.sent().is_empty());
    }

    #[test]
    fn leader_then_f_toggles_pane_fullscreen_without_typing() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();

        h.leader();
        h.key(KeyCode::Char('f'));
        assert!(h.app.pane_fullscreen);
        assert!(
            h.sent().is_empty(),
            "the fullscreen chord never reaches the child"
        );

        h.leader();
        h.key(KeyCode::Char('f'));
        assert!(!h.app.pane_fullscreen);
        assert!(h.sent().is_empty());
    }

    #[test]
    fn leader_then_x_closes_the_pane() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.app.pane_fullscreen = true;
        h.leader();
        h.key(KeyCode::Char('x'));
        assert!(matches!(h.sent()[0], ClientMsg::Kill { pane: PaneId(100) }));
        assert_eq!(
            h.app.focus,
            Focus::Panes,
            "land back in the list, not on another pane"
        );
        assert!(
            !h.app.pane_fullscreen,
            "closing restores the navigation columns"
        );
    }

    #[test]
    fn an_unbound_leader_chord_is_swallowed_not_typed() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.leader();
        h.key(KeyCode::Char('Q'));
        assert!(h.sent().is_empty());
        assert!(!h.app.leader_pending, "chord consumed");
    }

    // --- branches without a checkout ----------------------------------------

    /// The fixture tree with two branches nothing is sitting on, the
    /// column expanded to show them, and the selection parked on the first.
    fn harness_on_a_branch_row() -> Harness {
        let mut h = Harness::new();
        h.app.tree[0].repositories[0].branches =
            vec!["hotfix/tls".to_string(), "spike".to_string()];
        h.keys("ll"); // into the checkouts column
        h.key(KeyCode::Char('B')); // and show the branches at all
        h.keys("jj"); // past both checkouts, onto the first branch
        h.sent();
        h
    }

    #[test]
    fn the_branches_stay_out_of_the_column_until_they_are_asked_for() {
        // The column is for what is running. Forty branches on top of two
        // checkouts is the checkouts buried, not the branches surfaced.
        let mut h = Harness::new();
        h.app.tree[0].repositories[0].branches =
            vec!["hotfix/tls".to_string(), "spike".to_string()];
        h.keys("ll");

        assert_eq!(h.app.checkout_row_count(), 2, "the two checkouts, only");
        assert_eq!(h.app.current_branch_row(), None);

        h.key(KeyCode::Char('B'));
        assert_eq!(h.app.checkout_row_count(), 4);

        h.key(KeyCode::Char('B'));
        assert_eq!(h.app.checkout_row_count(), 2, "and away again");
    }

    #[test]
    fn the_main_branch_leads_the_column_even_with_nothing_sitting_on_it() {
        // Whatever it is named: this repository's is "trunk", and it is
        // still the row everything else is measured against.
        let mut h = Harness::new();
        let r = &mut h.app.tree[0].repositories[0];
        r.branches = vec!["spike".to_string(), "trunk".to_string()];
        r.default_branch = Some("trunk".to_string());
        h.keys("ll");

        assert_eq!(
            h.app.checkout_row_count(),
            3,
            "the main branch plus the two checkouts — and not `spike`"
        );
        assert_eq!(h.app.current_branch_row(), Some("trunk"), "at the top");
        h.key(KeyCode::Char('j'));
        assert_eq!(
            h.app.current_checkout().map(|c| c.id),
            Some(CheckoutId(10)),
            "the checkouts follow it in their own order"
        );
    }

    #[test]
    fn the_checkout_sitting_on_the_main_branch_leads_the_column() {
        // Same rule, the other way round: `feat` is the second checkout in
        // the tree, but it is where main lives, so it is the first row.
        let mut h = Harness::new();
        h.app.tree[0].repositories[0].default_branch = Some("feat".to_string());
        h.keys("ll");

        assert_eq!(h.app.checkout_row_count(), 2, "no branch row is invented");
        assert_eq!(h.app.current_checkout().map(|c| c.id), Some(CheckoutId(11)));
    }

    #[test]
    fn d_on_a_branch_row_offers_to_delete_the_branch_itself() {
        let mut h = harness_on_a_branch_row();

        h.key(KeyCode::Char('D'));
        match &h.app.prompt {
            Some(Prompt::ConfirmRemove { target, label }) => {
                assert_eq!(
                    *target,
                    RemoveTarget::Branch {
                        checkout: CheckoutId(10),
                        branch: "hotfix/tls".to_string(),
                    },
                    "the primary checkout is what git is run from"
                );
                assert_eq!(label, "hotfix/tls");
            }
            _ => panic!("expected a confirmation prompt"),
        }
        assert!(h.sent().is_empty(), "nothing sent before confirming");

        h.key(KeyCode::Char('y'));
        assert!(matches!(
            h.sent().as_slice(),
            [ClientMsg::DeleteBranch { checkout: CheckoutId(10), branch }] if branch == "hotfix/tls"
        ));
    }

    /// The fixture tree with one branch that exists only on the remote,
    /// the column expanded, and the selection parked on it.
    fn harness_on_a_remote_branch_row() -> Harness {
        let mut h = Harness::new();
        h.app.tree[0].repositories[0].remote_branches = vec!["origin/from-elsewhere".to_string()];
        h.keys("ll");
        h.key(KeyCode::Char('B'));
        h.keys("jj"); // past both checkouts
        h.sent();
        h
    }

    #[test]
    fn a_remote_only_branch_is_offered_under_the_name_it_would_have_here() {
        let mut h = harness_on_a_remote_branch_row();

        assert_eq!(
            h.app.current_remote_row(),
            Some("origin/from-elsewhere"),
            "the row says where it is"
        );
        assert_eq!(
            h.app.current_branch_row(),
            Some("from-elsewhere"),
            "but what you would switch to is the branch, not the remote's name for it"
        );

        h.key(KeyCode::Enter);
        assert!(
            matches!(
                h.sent().as_slice(),
                [ClientMsg::SwitchBranch { checkout: CheckoutId(10), branch }]
                    if branch == "from-elsewhere"
            ),
            "git makes the local branch off the remote one; we only name it"
        );
    }

    #[test]
    fn n_on_a_remote_branch_gives_it_a_worktree_under_its_local_name() {
        let mut h = harness_on_a_remote_branch_row();

        h.key(KeyCode::Char('n'));

        assert!(matches!(
            h.sent().as_slice(),
            [ClientMsg::CreateWorktree { checkout: CheckoutId(10), branch }]
                if branch == "from-elsewhere"
        ));
    }

    #[test]
    fn d_on_a_remote_branch_is_refused_rather_than_becoming_a_push() {
        let mut h = harness_on_a_remote_branch_row();

        h.key(KeyCode::Char('D'));

        assert!(h.app.prompt.is_none(), "no confirmation is even offered");
        assert!(h.sent().is_empty());
        assert!(h.app.status.contains("remote"), "got {:?}", h.app.status);
    }

    #[test]
    fn fetch_and_pull_run_in_the_selected_checkout() {
        let mut h = Harness::new();
        h.keys("llj"); // the linked worktree
        h.sent();

        h.key(KeyCode::Char('F'));
        assert!(matches!(
            h.sent().as_slice(),
            [ClientMsg::Fetch {
                checkout: CheckoutId(11)
            }]
        ));

        h.key(KeyCode::Char('P'));
        assert!(matches!(
            h.sent().as_slice(),
            [ClientMsg::Pull {
                checkout: CheckoutId(11)
            }]
        ));
    }

    #[test]
    fn a_fetch_from_a_branch_row_falls_back_to_the_primary_checkout() {
        // A branch with no directory has nowhere of its own to run git.
        let mut h = harness_on_a_remote_branch_row();

        h.key(KeyCode::Char('F'));

        assert!(matches!(
            h.sent().as_slice(),
            [ClientMsg::Fetch {
                checkout: CheckoutId(10)
            }]
        ));
    }

    #[test]
    fn branch_rows_come_after_the_checkouts_and_carry_no_checkout() {
        let mut h = harness_on_a_branch_row();

        assert_eq!(h.app.checkout_row_count(), 4, "two checkouts, two branches");
        assert_eq!(h.app.current_branch_row(), Some("hotfix/tls"));
        assert!(
            h.app.current_checkout().is_none(),
            "a branch row has no checkout, so nothing hangs off it"
        );
        assert!(h.app.current_pane().is_none());

        h.key(KeyCode::Char('j'));
        assert_eq!(h.app.current_branch_row(), Some("spike"));
        h.key(KeyCode::Char('j'));
        assert_eq!(
            h.app.current_branch_row(),
            Some("spike"),
            "the last branch is the last row"
        );
    }

    #[test]
    fn enter_on_a_branch_row_switches_the_primary_checkout_to_it() {
        let mut h = harness_on_a_branch_row();

        h.key(KeyCode::Enter);

        assert!(
            matches!(
                h.sent().as_slice(),
                [ClientMsg::SwitchBranch { checkout: CheckoutId(10), branch }] if branch == "hotfix/tls"
            ),
            "the primary checkout is where a branch with no directory goes"
        );
        assert_eq!(
            h.app.focus,
            Focus::Checkouts,
            "there is nothing to descend into"
        );
    }

    #[test]
    fn n_on_a_branch_row_gives_that_branch_a_worktree_without_asking_for_a_name() {
        let mut h = harness_on_a_branch_row();

        h.key(KeyCode::Char('n'));

        assert!(h.app.prompt.is_none(), "the branch is already named");
        assert!(matches!(
            h.sent().as_slice(),
            [ClientMsg::CreateWorktree { checkout: CheckoutId(10), branch }] if branch == "hotfix/tls"
        ));
    }

    #[test]
    fn a_branch_that_gets_a_checkout_stops_being_a_row_of_its_own() {
        // The daemon decides this — a branch is listed only while no
        // checkout is on it — so the client must not hold a selection that
        // outlives the row.
        let mut h = harness_on_a_branch_row();
        h.key(KeyCode::Char('j')); // the last row

        let mut tree = tree();
        tree[0].repositories[0].branches = vec!["hotfix/tls".to_string()];
        h.app.on_server_msg(ServerMsg::Tree(tree));

        assert_eq!(h.app.checkout_row_count(), 3);
        assert_eq!(h.app.sel_checkout, 2, "clamped onto the row that is left");
        assert_eq!(h.app.current_branch_row(), Some("hotfix/tls"));
    }

    // --- spawning ----------------------------------------------------------

    #[test]
    fn s_spawns_a_shell_in_the_selected_checkout_and_focuses_it() {
        let mut h = Harness::new();
        h.keys("llj"); // the linked worktree, which has no panes
        h.sent();
        h.key(KeyCode::Char('s'));
        assert!(
            matches!(
                h.sent()[0],
                ClientMsg::SpawnShell {
                    checkout: CheckoutId(11)
                }
            ),
            "spawns into the selected checkout"
        );

        // The daemon's next tree carries the new pane.
        let mut t = tree();
        t[0].repositories[0].checkouts[1]
            .panes
            .push(pane(102, "shell"));
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.sel_pane, 0);
        assert_eq!(
            h.app.focus,
            Focus::PaneContent,
            "drops you straight into it"
        );
        assert_eq!(h.app.column_pane(), Some(PaneId(102)));
    }

    #[test]
    fn a_spawn_focuses_the_newest_pane_not_the_first() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('s'));
        h.sent();
        let mut t = tree();
        t[0].repositories[0].checkouts[0]
            .panes
            .push(pane(102, "shell"));
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.column_pane(), Some(PaneId(102)));
    }

    #[test]
    fn a_selected_pane_is_followed_when_it_moves_to_another_checkout() {
        let mut h = Harness::new();
        h.app.focus = Focus::PaneContent;
        h.app.sel_pane = 1;
        assert_eq!(h.app.column_pane(), Some(PaneId(101)));

        let mut moved = tree();
        let pane = moved[0].repositories[0].checkouts[0].panes.remove(1);
        moved[0].repositories[0].checkouts[1].panes.push(pane);
        h.app.on_server_msg(ServerMsg::Tree(moved));

        assert_eq!(h.app.sel_checkout, 1);
        assert_eq!(h.app.column_pane(), Some(PaneId(101)));
        assert_eq!(h.app.focus, Focus::PaneContent);
    }

    #[test]
    fn a_selected_pane_is_followed_when_it_moves_to_another_repository() {
        let mut h = Harness::new();
        h.app.focus = Focus::PaneContent;
        h.app.sel_pane = 1;

        let mut moved = tree();
        let pane = moved[0].repositories[0].checkouts[0].panes.remove(1);
        moved[0].repositories.push(repository(
            7,
            "satellite",
            vec![checkout(30, "main", true, vec![pane])],
        ));
        h.app.on_server_msg(ServerMsg::Tree(moved));

        assert_eq!(h.app.sel_repository, 1);
        assert_eq!(h.app.current_repository().unwrap().name, "satellite");
        assert_eq!(h.app.column_pane(), Some(PaneId(101)));
        assert_eq!(h.app.focus, Focus::PaneContent);
    }

    #[test]
    fn a_background_pane_move_does_not_hijack_project_navigation() {
        let mut h = Harness::new();
        h.app.focus = Focus::Projects;

        let mut moved = tree();
        let pane = moved[0].repositories[0].checkouts[0].panes.remove(0);
        moved[0].repositories[0].checkouts[1].panes.push(pane);
        h.app.on_server_msg(ServerMsg::Tree(moved));

        assert_eq!(h.app.sel_checkout, 0);
        assert_eq!(h.app.focus, Focus::Projects);
    }

    #[test]
    fn a_picks_an_agent_template_and_spawns_it() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('a'));
        assert!(h.app.picker.is_some());
        h.key(KeyCode::Char('j'));
        assert_eq!(h.app.picker.as_ref().unwrap().sel, 1);
        h.key(KeyCode::Enter);
        assert!(h.app.picker.is_none());
        match &h.sent()[0] {
            ClientMsg::SpawnAgent { checkout, template } => {
                assert_eq!(*checkout, CheckoutId(10));
                assert_eq!(template, "codex");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn esc_cancels_the_agent_picker_without_spawning() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('a'));
        h.key(KeyCode::Esc);
        assert!(h.app.picker.is_none());
        assert!(h.sent().is_empty());
    }

    #[test]
    fn the_picker_selection_does_not_run_past_the_ends() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('a'));
        h.keys("jjj");
        assert_eq!(h.app.picker.as_ref().unwrap().sel, 1, "two templates");
        h.keys("kkk");
        assert_eq!(h.app.picker.as_ref().unwrap().sel, 0);
    }

    #[test]
    fn the_picker_swallows_navigation_keys() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('a'));
        h.keys("ll");
        assert_eq!(
            h.app.focus,
            Focus::Projects,
            "column focus must not move behind the modal"
        );
    }

    // --- prompts -----------------------------------------------------------

    #[test]
    fn n_in_the_projects_column_opens_the_directory_browser() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        assert!(h.app.dir_picker.is_some());
        // The browser asks where to start rather than guessing: only the
        // daemon knows what it can see.
        match &h.sent()[0] {
            ClientMsg::ListDirectories { path, .. } => assert_eq!(path, ""),
            other => panic!("unexpected {other:?}"),
        }

        h.browse("/some", Some("/"), &[("dir", false)]);
        h.keys("dir");
        h.key(KeyCode::Enter);
        match &h.sent()[0] {
            ClientMsg::AddProject { path } => assert_eq!(path, "/some/dir"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(h.app.dir_picker.is_none());
    }

    #[test]
    fn tab_walks_into_a_directory_and_enter_adds_where_you_land() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.browse("/home", Some("/"), &[("u", false)]);
        h.sent();

        h.keys("u");
        h.key(KeyCode::Tab);
        match &h.sent()[0] {
            ClientMsg::ListDirectories { path, .. } => assert_eq!(path, "/home/u"),
            other => panic!("unexpected {other:?}"),
        }

        h.browse("/home/u", Some("/home"), &[("code", true)]);
        h.key(KeyCode::Enter);
        match &h.sent()[0] {
            ClientMsg::AddProject { path } => {
                assert_eq!(path, "/home/u", "the first row is the directory you are in");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_listing_for_a_directory_already_left_is_dropped() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.browse("/home", Some("/"), &[("u", false)]);
        h.sent();
        h.keys("u");
        h.key(KeyCode::Tab);
        h.sent();

        h.app.on_server_msg(ServerMsg::Directories(DirListing {
            request_id: 999,
            path: "/stale".to_string(),
            parent: None,
            entries: Vec::new(),
            error: None,
        }));
        assert_eq!(h.app.dir_picker.as_ref().unwrap().path, "/home");
    }

    #[test]
    fn a_new_project_becomes_the_selected_one() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.browse("/d", Some("/"), &[]);
        h.key(KeyCode::Enter);
        h.sent();

        let mut t = tree();
        t.push(ProjectInfo {
            id: ProjectId(3),
            name: "new".to_string(),
            repositories: vec![repository(
                7,
                "new-repo",
                vec![checkout(30, "new", true, vec![])],
            )],
        });
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.current_project().unwrap().name, "new");
    }

    #[test]
    fn n_in_the_repositories_column_adds_a_repository_to_that_project() {
        let mut h = Harness::new();
        h.keys("l");
        h.sent();
        assert_eq!(h.app.focus, Focus::Repositories);

        h.key(KeyCode::Char('n'));
        assert!(h.app.dir_picker.is_some());

        h.browse("/some", Some("/"), &[("repo", true)]);
        h.sent();
        h.keys("repo");
        h.key(KeyCode::Enter);
        match &h.sent()[0] {
            ClientMsg::AddRepository { project, path } => {
                assert_eq!(*project, ProjectId(1), "the project in view");
                assert_eq!(path, "/some/repo");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(h.app.dir_picker.is_none());
    }

    #[test]
    fn a_new_repository_becomes_the_selected_one() {
        let mut h = Harness::new();
        h.keys("l");
        h.key(KeyCode::Char('n'));
        h.browse("/r", Some("/"), &[]);
        h.key(KeyCode::Enter);
        h.sent();

        let mut t = tree();
        t[0].repositories.push(repository(
            7,
            "added",
            vec![checkout(30, "main", true, vec![])],
        ));
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.current_repository().unwrap().name, "added");
    }

    #[test]
    fn esc_cancels_adding_a_repository() {
        let mut h = Harness::new();
        h.keys("l");
        h.sent();
        h.key(KeyCode::Char('n'));
        h.browse("/some", Some("/"), &[("repo", true)]);
        h.sent();
        h.keys("re");
        h.key(KeyCode::Esc);
        assert!(h.app.dir_picker.is_none());
        assert!(h.sent().is_empty());
    }

    #[test]
    fn n_in_the_checkouts_column_prompts_for_a_branch() {
        let mut h = Harness::new();
        h.keys("ll");
        h.sent();
        h.key(KeyCode::Char('n'));
        assert!(matches!(h.app.prompt, Some(Prompt::NewWorktree { .. })));

        h.keys("feat/x");
        h.key(KeyCode::Enter);
        match &h.sent()[0] {
            ClientMsg::CreateWorktree { checkout, branch } => {
                assert_eq!(
                    *checkout,
                    CheckoutId(10),
                    "branched off the selected checkout"
                );
                assert_eq!(branch, "feat/x");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_worktree_prompt_can_be_edited_and_cancelled() {
        let mut h = Harness::new();
        h.keys("lln");
        h.key(KeyCode::Up);
        h.keys("draft");
        h.key(KeyCode::Backspace);

        match &h.app.prompt {
            Some(Prompt::NewWorktree { input, .. }) => assert_eq!(input, "draf"),
            _ => panic!("expected a worktree prompt"),
        }

        h.key(KeyCode::Esc);
        assert!(h.app.prompt.is_none());
        assert!(h.sent().is_empty(), "cancelling must not create a worktree");
    }

    #[test]
    fn a_new_worktree_becomes_the_selected_checkout() {
        let mut h = Harness::new();
        h.keys("lln");
        h.keys("x");
        h.key(KeyCode::Enter);
        h.sent();

        let mut t = tree();
        t[0].repositories[0]
            .checkouts
            .push(checkout(12, "x", false, vec![]));
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.current_checkout().unwrap().name, "x");
    }

    #[test]
    fn a_pending_new_worktree_restores_the_columns_before_moving_selection() {
        let mut h = Harness::new();
        h.keys("llll");
        h.leader();
        h.key(KeyCode::Char('f'));
        h.app.pending_focus_new_checkout = Some(RepositoryId(5));
        let mut t = tree();
        t[0].repositories[0]
            .checkouts
            .push(checkout(12, "x", false, vec![]));

        h.app.on_server_msg(ServerMsg::Tree(t));

        assert_eq!(h.app.current_checkout().unwrap().name, "x");
        assert_eq!(h.app.focus, Focus::Panes);
        assert!(!h.app.pane_fullscreen);
    }

    #[test]
    fn a_new_worktree_selects_its_repository_even_if_navigation_moved() {
        let mut h = Harness::new();
        h.keys("lln");
        h.keys("x");
        h.key(KeyCode::Enter);
        h.sent();

        let mut t = tree();
        t[0].repositories.push(repository(
            7,
            "satellite",
            vec![checkout(30, "main", true, vec![])],
        ));
        t[0].repositories[0]
            .checkouts
            .push(checkout(12, "x", false, vec![]));
        h.app.sel_repository = 1;
        h.app.on_server_msg(ServerMsg::Tree(t));

        assert_eq!(h.app.current_repository().unwrap().id, RepositoryId(5));
        assert_eq!(h.app.current_checkout().unwrap().name, "x");
    }

    #[test]
    fn checkout_commands_use_the_repository_selections_current_checkout() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('l'));
        assert_eq!(h.app.focus, Focus::Repositories);
        h.sent();

        h.key(KeyCode::Char('s'));

        assert!(matches!(
            h.sent().as_slice(),
            [ClientMsg::SpawnShell {
                checkout: CheckoutId(10)
            }]
        ));
    }

    #[test]
    fn n_does_nothing_in_the_pane_columns() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.key(KeyCode::Char('n'));
        assert!(h.app.prompt.is_none(), "no checkout context to branch from");
        assert!(h.app.dir_picker.is_none());
    }

    #[test]
    fn an_empty_prompt_sends_nothing() {
        let mut h = Harness::new();
        h.keys("ll");
        h.sent();
        h.key(KeyCode::Char('n'));
        h.keys("   ");
        h.key(KeyCode::Enter);
        assert!(h.app.prompt.is_none());
        assert!(h.sent().is_empty(), "whitespace is not a branch name");
    }

    #[test]
    fn esc_cancels_a_prompt_and_backspace_edits_it() {
        let mut h = Harness::new();
        h.keys("ll");
        h.sent();
        h.key(KeyCode::Char('n'));
        h.keys("abc");
        h.key(KeyCode::Backspace);
        match &h.app.prompt {
            Some(Prompt::NewWorktree { input, .. }) => assert_eq!(input, "ab"),
            _ => panic!("expected the new-worktree prompt to still be open"),
        }
        h.key(KeyCode::Esc);
        assert!(h.app.prompt.is_none());
        assert!(h.sent().is_empty());
    }

    #[test]
    fn the_directory_browser_swallows_navigation_keys() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.browse("/home", Some("/"), &[("u", false)]);
        h.keys("jl");
        assert_eq!(h.app.sel_project, 0, "j typed into the query, not a move");
        assert_eq!(h.app.focus, Focus::Projects);
        assert_eq!(h.app.dir_picker.as_ref().unwrap().query, "jl");
    }

    #[test]
    fn a_pasted_path_goes_into_the_browser_and_not_a_pane() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.browse("/home", Some("/"), &[]);
        h.sent();
        h.app.on_paste("/var/src".to_string());
        h.key(KeyCode::Tab);
        match &h.sent()[0] {
            ClientMsg::ListDirectories { path, .. } => assert_eq!(path, "/var/src"),
            other => panic!("unexpected {other:?}"),
        }
    }

    // --- removing a checkout ------------------------------------------------

    #[test]
    fn the_primary_checkout_cannot_be_removed() {
        let mut h = Harness::new();
        h.keys("ll");
        h.sent();
        h.key(KeyCode::Char('D'));
        assert!(h.app.prompt.is_none(), "no confirmation is even offered");
        assert!(h.sent().is_empty(), "and nothing is sent to the daemon");
        assert!(h.app.status.contains("primary"));
    }

    #[test]
    fn removing_a_linked_worktree_asks_first_then_sends() {
        let mut h = Harness::new();
        h.keys("llj");
        h.sent();
        h.key(KeyCode::Char('D'));
        match &h.app.prompt {
            Some(Prompt::ConfirmRemove { target, label }) => {
                assert_eq!(*target, RemoveTarget::Checkout(CheckoutId(11)));
                assert_eq!(label, "feat", "the confirmation names what it will delete");
            }
            _ => panic!("expected a confirmation prompt"),
        }
        assert!(h.sent().is_empty(), "nothing sent before confirming");

        h.key(KeyCode::Char('y'));
        assert!(matches!(
            h.sent()[0],
            ClientMsg::RemoveCheckout {
                checkout: CheckoutId(11)
            }
        ));
        assert!(h.app.prompt.is_none());
    }

    #[test]
    fn removing_a_project_asks_first_then_sends() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('D'));
        match &h.app.prompt {
            Some(Prompt::ConfirmRemove { target, label }) => {
                assert_eq!(*target, RemoveTarget::Project(ProjectId(1)));
                assert_eq!(label, "argus");
            }
            other => panic!("expected a project confirmation, got {:?}", other.is_some()),
        }
        h.key(KeyCode::Char('y'));
        assert!(matches!(
            h.sent()[0],
            ClientMsg::RemoveProject {
                project: ProjectId(1)
            }
        ));
    }

    #[test]
    fn removing_a_repository_asks_first_then_sends() {
        let mut h = Harness::new();
        h.keys("l");
        h.sent();
        h.key(KeyCode::Char('D'));
        match &h.app.prompt {
            Some(Prompt::ConfirmRemove { target, label }) => {
                assert_eq!(*target, RemoveTarget::Repository(RepositoryId(5)));
                assert_eq!(label, "orion");
            }
            other => panic!(
                "expected a repository confirmation, got {:?}",
                other.is_some()
            ),
        }
        h.key(KeyCode::Char('y'));
        assert!(matches!(
            h.sent()[0],
            ClientMsg::RemoveRepository {
                repository: RepositoryId(5)
            }
        ));
    }

    #[test]
    fn a_removal_confirmation_says_whether_files_are_going_away() {
        // The whole point of the project/repository removals is that they
        // are not deletions, and the popup is the only place that says so.
        for target in [
            RemoveTarget::Project(ProjectId(1)),
            RemoveTarget::Repository(RepositoryId(5)),
        ] {
            assert!(target.wording().1.contains("files stay"));
        }
        assert!(RemoveTarget::Checkout(CheckoutId(11))
            .wording()
            .1
            .contains("worktree"));
    }

    #[test]
    fn declining_the_removal_sends_nothing() {
        for decline in ['n', 'q'] {
            let mut h = Harness::new();
            h.keys("llj");
            h.sent();
            h.key(KeyCode::Char('D'));
            h.key(KeyCode::Char(decline));
            if decline == 'n' {
                assert!(h.app.prompt.is_none(), "n declines");
            }
            assert!(h.sent().is_empty(), "{decline} must not delete anything");
        }
    }

    #[test]
    fn d_does_nothing_in_the_pane_columns() {
        // `D` follows focus through the three tree columns; a pane is
        // closed with `x` instead, and has nothing to remove.
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.key(KeyCode::Char('D'));
        assert!(
            h.app.prompt.is_none(),
            "no removal offered from the panes column"
        );
        assert!(h.sent().is_empty());
    }

    // --- closing panes ------------------------------------------------------

    #[test]
    fn x_closes_the_selected_pane_from_the_panes_column() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.key(KeyCode::Char('x'));
        assert!(matches!(h.sent()[0], ClientMsg::Kill { pane: PaneId(100) }));
    }

    #[test]
    fn x_does_nothing_from_the_other_columns() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('x'));
        h.key(KeyCode::Char('l'));
        h.sent();
        h.key(KeyCode::Char('x'));
        assert!(h.sent().is_empty(), "x is not a global delete");
    }

    // --- tree updates -------------------------------------------------------

    #[test]
    fn a_shrinking_tree_clamps_the_selection_instead_of_dangling() {
        let mut h = Harness::new();
        h.keys("lllj"); // second pane of the first checkout
        h.sent();
        let mut t = tree();
        t[0].repositories[0].checkouts[0].panes.pop();
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.sel_pane, 0);
        assert_eq!(h.app.column_pane(), Some(PaneId(100)));
    }

    #[test]
    fn a_fullscreen_pane_that_vanishes_restores_the_pane_list() {
        let mut h = Harness::new();
        h.keys("llll");
        h.leader();
        h.key(KeyCode::Char('f'));
        let mut t = tree();
        t[0].repositories[0].checkouts[0].panes.remove(0);

        h.app.on_server_msg(ServerMsg::Tree(t));

        assert_eq!(h.app.focus, Focus::Panes);
        assert!(!h.app.pane_fullscreen);
        assert_eq!(h.app.column_pane(), Some(PaneId(101)));
    }

    #[test]
    fn an_empty_tree_leaves_nothing_selected_and_nothing_subscribed() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Tree(Vec::new()));
        assert!(h.app.current_project().is_none());
        assert_eq!(h.app.sel_project, 0);
        assert_eq!(h.app.column_pane(), None);
    }

    #[test]
    fn templates_arrive_out_of_band() {
        let (tx, _rx) = unbounded_channel();
        let mut app = App::new(tx);
        assert!(app.templates.is_empty());
        app.on_server_msg(ServerMsg::Templates(vec!["claude".to_string()]));
        assert_eq!(app.templates, vec!["claude"]);
    }

    #[test]
    fn a_picker_will_not_open_with_no_templates() {
        let (tx, _rx) = unbounded_channel();
        let mut app = App::new(tx);
        app.on_server_msg(ServerMsg::Tree(tree()));
        app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(app.picker.is_none());
    }

    #[test]
    fn a_pane_exit_is_reported_in_the_status_line() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::PaneClosed {
            pane: PaneId(100),
            code: Some(1),
        });
        assert_eq!(
            h.app.status, "pane exited with code 1",
            "the bar is prose, not a Debug dump"
        );
        assert!(
            h.app.status_alert,
            "a failed exit is the thing you have to read"
        );
    }

    #[test]
    fn a_fullscreen_pane_exit_restores_the_pane_list() {
        let mut h = Harness::new();
        h.keys("llll");
        h.leader();
        h.key(KeyCode::Char('f'));

        h.app.on_server_msg(ServerMsg::PaneClosed {
            pane: PaneId(100),
            code: Some(0),
        });

        assert_eq!(h.app.focus, Focus::Panes);
        assert!(!h.app.pane_fullscreen);
    }

    #[test]
    fn a_clean_exit_is_news_but_a_kill_is_an_alarm() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::PaneClosed {
            pane: PaneId(100),
            code: Some(0),
        });
        assert_eq!(h.app.status, "pane exited");
        assert!(
            !h.app.status_alert,
            "a clean exit is no louder here than the ✓ on its row"
        );

        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::PaneClosed {
            pane: PaneId(100),
            code: None,
        });
        assert_eq!(h.app.status, "pane was killed");
        assert!(h.app.status_alert);
    }

    #[test]
    fn a_daemon_error_is_surfaced_not_swallowed() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Error {
            message: "git worktree add failed".to_string(),
        });
        assert!(h.app.status.contains("git worktree add failed"));
    }

    #[test]
    fn a_message_gives_the_bar_back_on_the_next_keypress() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Error {
            message: "git worktree add failed".to_string(),
        });
        h.key(KeyCode::Char('j'));
        assert!(
            h.app.status.is_empty(),
            "an unread error would hide the breadcrumb forever: {}",
            h.app.status
        );
    }

    #[test]
    fn a_click_acknowledges_a_message_but_a_mouse_move_does_not() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app.on_server_msg(ServerMsg::PaneClosed {
            pane: PaneId(100),
            code: Some(1),
        });
        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 2,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        assert!(
            !h.app.status.is_empty(),
            "drifting across the terminal is not reading it"
        );

        h.app.on_mouse(click(2, 3));
        assert!(h.app.status.is_empty(), "{}", h.app.status);
    }

    #[test]
    fn git_status_rides_along_on_checkout_rows() {
        let mut h = Harness::new();
        let mut t = tree();
        t[0].repositories[0].checkouts[0].git = Some(GitStatus {
            branch: Some("master".to_string()),
            dirty: true,
            changed_files: 2,
            ahead: 1,
            behind: 0,
        });
        h.app.on_server_msg(ServerMsg::Tree(t));
        let g = h.app.current_checkout().unwrap().git.as_ref().unwrap();
        assert_eq!(g.branch.as_deref(), Some("master"));
        assert_eq!(g.changed_files, 2);
    }

    #[test]
    fn q_detaches_from_the_nav_columns() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('q'));
        assert!(h.app.should_quit);
    }

    // --- mouse --------------------------------------------------------------

    fn click(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Five cards side by side, each with a one-cell frame around its rows,
    /// so tests can click both a row and the chrome around it.
    fn laid_out(h: &mut Harness) {
        let panel = |x: u16, w: u16| Panel {
            outer: Rect::new(x, 0, w, 8),
            inner: Rect::new(x + 1, 1, w - 2, 6),
            first: 0,
        };
        h.app.layout = Layout {
            projects: panel(0, 12),
            repositories: panel(12, 12),
            checkouts: panel(24, 12),
            panes: panel(36, 12),
            content: panel(48, 20),
            overlay: Panel::default(),
            cursor: None,
        };
    }

    /// Say that `pane`'s child has asked for SGR mouse reporting. Nothing is
    /// forwarded to a child that hasn't, so any test about forwarding has to
    /// establish this first.
    fn wants_mouse(h: &mut Harness, pane: PaneId) {
        let grid = h
            .app
            .grids
            .entry(pane)
            .or_insert_with(|| crate::grid::Grid::new(Vec::new()));
        grid.mouse = argus_protocol::MouseTracking {
            mode: argus_protocol::MouseMode::ButtonMotion,
            encoding: argus_protocol::MouseEncoding::Sgr,
        };
    }

    /// Say that `pane`'s child is drawing on the alternate screen without
    /// mouse reporting — Claude, Codex, and Cursor Agent's usual mode.
    fn on_alt_screen(h: &mut Harness, pane: PaneId) {
        h.app
            .grids
            .entry(pane)
            .or_insert_with(|| crate::grid::Grid::new(Vec::new()))
            .alternate_screen = true;
    }

    fn drag(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn dragging_a_gutter_resizes_the_two_adjacent_columns() {
        let mut h = Harness::new();
        let panel = |x: u16, w: u16| Panel {
            outer: Rect::new(x, 0, w, 8),
            inner: Rect::new(x + 1, 1, w.saturating_sub(2), 6),
            first: 0,
        };
        h.app.layout = Layout {
            projects: panel(0, 12),
            repositories: panel(13, 12),
            checkouts: panel(26, 12),
            panes: panel(39, 12),
            content: panel(52, 20),
            overlay: Panel::default(),
            cursor: None,
        };

        h.app.on_mouse(click(12, 3));
        h.app.on_mouse(drag(16, 3));
        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 16,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(h.app.column_widths, Some(vec![16, 8, 12, 12, 20]));
        assert_eq!(h.app.settings.column_widths, h.app.column_widths);
    }

    #[test]
    fn old_four_column_widths_fall_back_to_the_five_column_layout() {
        let (tx, _rx) = unbounded_channel();
        let settings = crate::settings::Settings {
            column_widths: Some(vec![10, 20, 30, 40]),
            ..crate::settings::Settings::default()
        };

        let app = App::build(tx, settings, false);

        assert_eq!(app.column_widths, None);
    }

    #[test]
    fn saved_five_column_widths_are_restored_at_startup() {
        let (tx, _rx) = unbounded_channel();
        let settings = crate::settings::Settings {
            column_widths: Some(vec![10, 15, 20, 25, 30]),
            ..crate::settings::Settings::default()
        };

        let app = App::build(tx, settings, false);

        assert_eq!(app.column_widths, Some(vec![10, 15, 20, 25, 30]));
    }

    #[test]
    fn dragging_a_gutter_cannot_collapse_either_column() {
        let mut h = Harness::new();
        let panel = |x: u16, w: u16| Panel {
            outer: Rect::new(x, 0, w, 8),
            inner: Rect::new(x + 1, 1, w.saturating_sub(2), 6),
            first: 0,
        };
        h.app.layout = Layout {
            projects: panel(0, 12),
            repositories: panel(13, 12),
            checkouts: panel(26, 12),
            panes: panel(39, 12),
            content: panel(52, 20),
            overlay: Panel::default(),
            cursor: None,
        };

        h.app.on_mouse(click(12, 3));
        h.app.on_mouse(drag(30, 3));

        assert_eq!(h.app.column_widths, Some(vec![16, 8, 12, 12, 20]));
    }

    #[test]
    fn clicking_a_row_selects_it_and_focuses_that_column() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app.on_mouse(click(26, 3)); // the checkouts card, second row
        assert_eq!(h.app.focus, Focus::Checkouts);
        assert_eq!(h.app.sel_checkout, 1);
    }

    #[test]
    fn clicking_a_child_row_selects_the_pane_it_runs_in() {
        // A child is drawn as its own row but is not somewhere to go: the
        // only thing to select there is the pane it is running inside.
        let mut h = Harness::new();
        laid_out(&mut h);
        if let Some(p) = h.app.tree[0].repositories[0].checkouts[0].panes.get_mut(0) {
            p.children = vec![argus_protocol::ChildAgentInfo {
                label: "running the tests".to_string(),
                status: PaneStatus::Working,
                note: None,
            }];
        }

        // Row 1 of the panes column is the first pane's child; row 2 is the
        // second pane, which without children would have been row 1.
        h.app.on_mouse(click(38, 3));
        assert_eq!(h.app.focus, Focus::Panes);
        assert_eq!(h.app.sel_pane, 0, "a child row means its parent");

        h.app.on_mouse(click(38, 5));
        assert_eq!(h.app.sel_pane, 1, "and the rows below it have shifted down");
    }

    #[test]
    fn clicking_an_already_selected_row_descends() {
        let mut h = Harness::new();
        laid_out(&mut h);
        // Row 1 isn't the current selection, so the first click only selects.
        h.app.on_mouse(click(2, 3));
        assert_eq!(h.app.focus, Focus::Projects);
        assert_eq!(h.app.sel_project, 1);
        // Clicking the now-selected row again opens it.
        h.app.on_mouse(click(2, 3));
        assert_eq!(h.app.focus, Focus::Repositories, "second click opens it");
    }

    #[test]
    fn clicking_past_the_last_row_keeps_the_selection() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.keys("lll"); // focus is off in the panes column
        h.sent();

        h.app.on_mouse(click(2, 6)); // empty space below the project rows

        assert_eq!(h.app.focus, Focus::Projects, "the click still moves focus");
        assert_eq!(h.app.sel_project, 0, "but selects nothing new");
    }

    #[test]
    fn clicking_a_cards_frame_moves_focus_without_touching_the_selection() {
        // "Go there" and "pick that" are different gestures; only the
        // second should move a cursor.
        let mut h = Harness::new();
        laid_out(&mut h);
        h.keys("l");
        h.app.sel_checkout = 1;
        h.sent();

        h.app.on_mouse(click(0, 0)); // the projects card's top-left corner

        assert_eq!(h.app.focus, Focus::Projects);
        assert_eq!(h.app.sel_checkout, 1, "the other column keeps its place");
    }

    #[test]
    fn clicking_the_border_of_the_column_you_are_in_does_not_descend() {
        // Only a click on the selected *row* opens it.
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app.on_mouse(click(0, 0));
        h.app.on_mouse(click(0, 0));
        assert_eq!(h.app.focus, Focus::Projects);
    }

    #[test]
    fn either_line_of_a_two_line_row_selects_that_row() {
        // The detail line is part of the item, not a gap between items.
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app.on_mouse(click(2, 3)); // name line of row 1
        assert_eq!(h.app.sel_project, 1);

        h.app.sel_project = 0;
        h.app.on_mouse(click(2, 4)); // detail line of the same row
        assert_eq!(h.app.sel_project, 1);
    }

    #[test]
    fn scrolling_a_background_column_does_not_steal_focus() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.keys("llll"); // typing into a pane
        h.sent();
        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 2,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(h.app.sel_project, 1, "the scroll still moved the list");
        assert_eq!(h.app.focus, Focus::PaneContent, "but focus stayed put");
    }

    #[test]
    fn clicking_the_live_view_switches_to_typing_and_forwards_the_click() {
        let mut h = Harness::new();
        laid_out(&mut h);
        let pane = h.app.column_pane().unwrap();
        wants_mouse(&mut h, pane);
        h.app.on_mouse(click(54, 3));
        assert_eq!(h.app.focus, Focus::PaneContent);
        assert!(
            h.sent()
                .iter()
                .any(|m| matches!(m, ClientMsg::Input { .. })),
            "the child gets the click too"
        );
    }

    #[test]
    fn releasing_in_the_live_view_is_forwarded_when_not_resizing() {
        let mut h = Harness::new();
        laid_out(&mut h);
        let pane = h.app.column_pane().unwrap();
        wants_mouse(&mut h, pane);
        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 54,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });

        assert!(
            h.sent().iter().any(|message| matches!(
                message,
                ClientMsg::Input { bytes, .. } if bytes.ends_with(b"m")
            )),
            "the child gets an ordinary release"
        );
    }

    #[test]
    fn nothing_is_forwarded_to_a_child_that_never_asked_for_the_mouse() {
        // The bug: an agent that does no mouse reporting was still sent
        // `ESC [ < ... M` for every click and wheel turn, and typed it into
        // its prompt.
        let mut h = Harness::new();
        laid_out(&mut h);
        h.sent();

        h.app.on_mouse(click(54, 3));
        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 54,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });

        assert!(
            !h.sent()
                .iter()
                .any(|m| matches!(m, ClientMsg::Input { .. })),
            "no mouse bytes reach a child that reports no mouse"
        );
        assert_eq!(
            h.app.focus,
            Focus::PaneContent,
            "the click still selects the live view"
        );
    }

    #[test]
    fn a_wheel_over_an_alt_screen_tui_arrives_as_arrows() {
        // Codex enables DECSET 1007 rather than mouse tracking; Claude and
        // Cursor Agent take the alternate screen the same way. A swallowed
        // wheel is a conversation that cannot scroll.
        let mut h = Harness::new();
        laid_out(&mut h);
        let pane = h.app.column_pane().unwrap();
        on_alt_screen(&mut h, pane);
        h.sent();

        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 54,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 54,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });

        let bytes: Vec<Vec<u8>> = h
            .sent()
            .into_iter()
            .filter_map(|m| match m {
                ClientMsg::Input { bytes, .. } => Some(bytes),
                _ => None,
            })
            .collect();
        assert_eq!(bytes, [b"\x1b[B".to_vec(), b"\x1b[A".to_vec()]);
    }

    #[test]
    fn a_mouse_tracking_child_still_gets_wheel_reports_not_arrows() {
        // OpenCode enables SGR mouse reporting (and the alternate screen).
        // Those reports must win over the cursor-key fallback.
        let mut h = Harness::new();
        laid_out(&mut h);
        let pane = h.app.column_pane().unwrap();
        wants_mouse(&mut h, pane);
        on_alt_screen(&mut h, pane);
        h.sent();

        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 54,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });

        let bytes = h.sent().iter().find_map(|m| match m {
            ClientMsg::Input { bytes, .. } => Some(bytes.clone()),
            _ => None,
        });
        let bytes = bytes.expect("the child gets a mouse report");
        assert!(
            bytes.starts_with(b"\x1b[<65;"),
            "SGR wheel down, not a cursor key: {bytes:?}"
        );
    }

    #[test]
    fn a_wheel_over_the_live_view_never_scrolls_the_columns_behind_it() {
        let mut h = Harness::new();
        laid_out(&mut h);
        let before = h.app.sel_project;

        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 54,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(h.app.sel_project, before);
    }

    #[test]
    fn a_release_is_dropped_for_a_child_that_only_reports_presses() {
        let mut h = Harness::new();
        laid_out(&mut h);
        let pane = h.app.column_pane().unwrap();
        wants_mouse(&mut h, pane);
        h.app.grids.get_mut(&pane).unwrap().mouse.mode = argus_protocol::MouseMode::Press;
        h.sent();

        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 54,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });

        assert!(!h
            .sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Input { .. })));
    }

    #[test]
    fn the_mouse_is_ignored_while_a_modal_is_open() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.key(KeyCode::Char('n'));
        h.app.on_mouse(click(14, 3));
        assert_eq!(
            h.app.sel_checkout, 0,
            "click must not navigate behind the modal"
        );
        assert!(h.app.dir_picker.is_some());
    }

    #[test]
    fn resize_is_forwarded_for_the_named_pane() {
        let mut h = Harness::new();
        h.app.resize_pane(PaneId(100), 30, 100);
        match &h.sent()[0] {
            ClientMsg::Resize { pane, rows, cols } => {
                assert_eq!(*pane, PaneId(100));
                assert_eq!((*rows, *cols), (30, 100));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn every_pane_on_screen_is_sized_from_its_own_area() {
        // A floating editor and the column behind it are different widths;
        // sizing both from one of them wraps the other wrongly.
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app
            .open_overlay_pane(PaneId(700), "vim".to_string(), false);
        h.app.layout.overlay = Panel {
            outer: Rect::new(2, 1, 60, 20),
            inner: Rect::new(3, 2, 58, 18),
            first: 0,
        };

        let live = h.app.live_panes();
        assert_eq!(live.len(), 2, "the column's pane and the floating one");
        assert_eq!(live[0].1, h.app.layout.content.inner);
        assert_eq!(live[1].1, h.app.layout.overlay.inner);
    }

    // --- workspaces ---------------------------------------------------------

    fn workspaces(open: &str) -> Vec<argus_protocol::WorkspaceInfo> {
        ["default", "work", "weekend"]
            .iter()
            .enumerate()
            .map(|(i, name)| argus_protocol::WorkspaceInfo {
                id: argus_protocol::WorkspaceId(i as u64 + 1),
                name: name.to_string(),
                projects: i + 1,
                panes: i,
                open: *name == open,
            })
            .collect()
    }

    #[test]
    fn the_open_workspace_is_remembered_from_the_daemons_list() {
        let mut h = Harness::new();
        h.app
            .on_server_msg(ServerMsg::Workspaces(workspaces("work")));
        assert_eq!(h.app.open_workspace, "work");
        assert_eq!(h.app.workspaces.len(), 3);
    }

    #[test]
    fn w_opens_a_picker_positioned_on_the_workspace_already_open() {
        // "Look at where I am, then move" is the reason to press it, so
        // starting at the top of the list would be the wrong default.
        let mut h = Harness::new();
        h.app
            .on_server_msg(ServerMsg::Workspaces(workspaces("work")));
        h.key(KeyCode::Char('w'));

        let picker = h.app.picker.as_ref().expect("w should open the picker");
        assert_eq!(picker.sel, 1, "starts on the open one");
        assert!(picker.items[1].starts_with("work"));
    }

    #[test]
    fn choosing_a_workspace_asks_the_daemon_to_switch() {
        let mut h = Harness::new();
        h.app
            .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        h.key(KeyCode::Down);
        h.key(KeyCode::Enter);

        match &h.sent()[0] {
            ClientMsg::OpenWorkspace { workspace } => {
                assert_eq!(*workspace, argus_protocol::WorkspaceId(2), "the 'work' row");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(h.app.picker.is_none());
    }

    #[test]
    fn switching_workspace_resets_navigation_to_the_top() {
        // The incoming tree is a different set of projects; keeping an index
        // that meant something else would land the user somewhere arbitrary.
        let mut h = Harness::new();
        h.app
            .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.keys("lllj"); // wander into the pane column
        h.sent();

        h.key(KeyCode::Char('w'));
        h.key(KeyCode::Down);
        h.key(KeyCode::Enter);

        assert_eq!(h.app.focus, Focus::Projects);
        assert_eq!(
            (h.app.sel_project, h.app.sel_checkout, h.app.sel_pane),
            (0, 0, 0)
        );
    }

    #[test]
    fn escaping_the_workspace_picker_switches_nothing() {
        let mut h = Harness::new();
        h.app
            .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        h.key(KeyCode::Down);
        h.key(KeyCode::Esc);
        assert!(h.app.picker.is_none());
        assert!(h.sent().is_empty());
    }

    fn only_default() -> Vec<argus_protocol::WorkspaceInfo> {
        vec![argus_protocol::WorkspaceInfo {
            id: argus_protocol::WorkspaceId(1),
            name: "default".to_string(),
            projects: 1,
            panes: 0,
            open: true,
        }]
    }

    #[test]
    fn w_still_opens_on_a_lone_workspace_because_that_is_where_a_second_comes_from() {
        // The zero-config case, and the one that used to be a dead end:
        // with no way to name a workspace here, an install stayed at one
        // forever unless the user hand-edited projects.toml.
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Workspaces(only_default()));
        h.key(KeyCode::Char('w'));
        assert!(h.app.picker.is_some());
    }

    #[test]
    fn a_query_naming_no_workspace_offers_to_create_it() {
        let mut h = Harness::new();
        h.app
            .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        h.app.picker.as_mut().unwrap().type_query("weekday");

        let p = h.app.picker.as_ref().unwrap();
        assert_eq!(p.create.as_deref(), Some("weekday"));
    }

    #[test]
    fn a_query_naming_a_workspace_that_exists_does_not_offer_to_create_it() {
        // Two ways to reach the same workspace, one of which would fail on
        // the daemon, is worse than one.
        let mut h = Harness::new();
        h.app
            .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        h.app.picker.as_mut().unwrap().type_query("weekend");
        assert_eq!(h.app.picker.as_ref().unwrap().create, None);
    }

    #[test]
    fn workspace_rows_are_matched_on_their_names_not_their_counts() {
        // The rows carry "2\u{25a3}"; typing a digit must not "find" a
        // workspace by how many panes it happens to be running.
        let mut h = Harness::new();
        h.app
            .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        h.app.picker.as_mut().unwrap().type_query("2");
        let p = h.app.picker.as_ref().unwrap();
        assert!(p.shown.is_empty(), "no workspace is named 2: {:?}", p.shown);
        assert_eq!(
            p.create.as_deref(),
            Some("2"),
            "it is a name to make instead"
        );
    }

    #[test]
    fn choosing_the_create_row_makes_the_workspace() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Workspaces(only_default()));
        h.key(KeyCode::Char('w'));
        h.keys("side");
        h.key(KeyCode::Down); // past the (now empty) matches, onto create
        h.key(KeyCode::Enter);

        match &h.sent()[0] {
            ClientMsg::CreateWorkspace { name } => assert_eq!(name, "side"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(h.app.picker.is_none());
    }

    #[test]
    fn a_created_workspace_arrives_empty_so_navigation_starts_over() {
        // The daemon opens what it creates, and it has no projects; leaving
        // the columns pointed into the old workspace would be a selection
        // into a tree that is gone.
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Workspaces(only_default()));
        h.keys("lllj");
        h.sent();

        h.key(KeyCode::Char('w'));
        h.keys("side");
        h.key(KeyCode::Down);
        h.key(KeyCode::Enter);

        assert_eq!(h.app.focus, Focus::Projects);
        assert_eq!(
            (h.app.sel_project, h.app.sel_checkout, h.app.sel_pane),
            (0, 0, 0)
        );
    }

    #[test]
    fn the_top_row_still_switches_rather_than_creating() {
        // The create row sits below the matches; enter on a match is a
        // switch, exactly as it was before the row existed.
        let mut h = Harness::new();
        h.app
            .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        h.keys("week");
        h.key(KeyCode::Enter);

        match &h.sent()[0] {
            ClientMsg::OpenWorkspace { workspace } => {
                assert_eq!(
                    *workspace,
                    argus_protocol::WorkspaceId(3),
                    "the 'weekend' row"
                );
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_picker_shows_how_much_is_running_in_each_workspace() {
        // The reason to surface counts at all: an agent working somewhere
        // you are not looking should still be visible.
        let mut h = Harness::new();
        h.app
            .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        let items = &h.app.picker.as_ref().unwrap().items;
        assert!(
            items[2].contains("2▣"),
            "weekend has two live panes: {items:?}"
        );
        assert!(
            !items[0].contains('▣'),
            "an idle workspace stays quiet: {items:?}"
        );
    }

    #[test]
    fn the_agent_picker_still_spawns_after_the_picker_grew_a_second_use() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('a'));
        h.key(KeyCode::Enter);
        assert!(matches!(h.sent()[0], ClientMsg::SpawnAgent { .. }));
    }
    // --- review -------------------------------------------------------------

    fn diff_of(checkout: CheckoutId) -> argus_protocol::Review {
        argus_protocol::Review {
            request_id: 1,
            checkout,
            base: argus_protocol::ReviewBase::Unstaged,
            files: vec![argus_protocol::FileDiff {
                path: "src/a.rs".to_string(),
                old_path: None,
                kind: argus_protocol::ChangeKind::Modified,
                hunks: vec![argus_protocol::Hunk {
                    header: "@@ -1,2 +1,2 @@".to_string(),
                    lines: vec![
                        argus_protocol::DiffLine {
                            kind: argus_protocol::LineKind::Context,
                            old_lineno: Some(1),
                            new_lineno: Some(1),
                            text: "keep".to_string(),
                            spans: Vec::new(),
                        },
                        argus_protocol::DiffLine {
                            kind: argus_protocol::LineKind::Added,
                            old_lineno: None,
                            new_lineno: Some(2),
                            text: "new".to_string(),
                            spans: Vec::new(),
                        },
                    ],
                }],
                note: None,
            }],
            commit: None,
        }
    }

    /// Presses `R` on the first checkout and answers with `review`.
    fn open_review(h: &mut Harness, review: argus_protocol::Review) {
        h.key(KeyCode::Char('l'));
        h.key(KeyCode::Char('R'));
        h.sent();
        h.app.on_server_msg(ServerMsg::Review(review));
    }

    #[test]
    fn r_asks_the_daemon_for_the_selected_checkouts_diff() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('l')); // into the checkouts column
        let checkout = h.app.current_checkout().unwrap().id;
        h.key(KeyCode::Char('R'));

        match &h.sent()[0] {
            ClientMsg::Review { checkout: c, .. } => assert_eq!(*c, checkout),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_arriving_diff_opens_the_viewer_and_takes_focus() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));

        assert!(h.app.review.is_some());
        assert_eq!(h.app.focus, Focus::Review);
    }

    #[test]
    fn a_diff_for_a_checkout_the_user_left_is_dropped() {
        // It was computed on a blocking thread; by the time it lands the
        // user may be looking at something else entirely.
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        h.key(KeyCode::Char('l'));
        h.key(KeyCode::Char('R'));
        h.sent();

        h.app
            .on_server_msg(ServerMsg::Review(diff_of(CheckoutId(9999))));
        assert!(
            h.app.review.is_none(),
            "not for the checkout we asked about"
        );

        h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
        assert!(h.app.review.is_some());
    }

    #[test]
    fn only_the_exact_latest_review_request_is_accepted() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('l'));
        h.key(KeyCode::Char('R'));
        h.key(KeyCode::Char('R'));
        h.sent();
        let checkout = h.app.current_checkout().unwrap().id;

        h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
        assert!(h.app.review.is_none());

        let mut latest = diff_of(checkout);
        latest.request_id = 2;
        h.app.on_server_msg(ServerMsg::Review(latest));
        assert!(h.app.review.is_some());
    }

    #[test]
    fn an_unsolicited_diff_never_hijacks_the_screen() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
        assert!(h.app.review.is_none());
        assert_eq!(h.app.focus, Focus::Projects);
    }

    #[test]
    fn a_clean_checkout_says_so_instead_of_opening_an_empty_viewer() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(
            &mut h,
            argus_protocol::Review {
                request_id: 1,
                checkout,
                base: argus_protocol::ReviewBase::Unstaged,
                files: Vec::new(),
                commit: None,
            },
        );
        assert!(h.app.review.is_none());
        assert_ne!(h.app.focus, Focus::Review);
        assert!(
            h.app.status.contains("no changes vs unstaged"),
            "{}",
            h.app.status
        );
    }

    #[test]
    fn esc_closes_the_review_and_lands_back_on_the_checkout() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));
        h.key(KeyCode::Esc);

        assert!(h.app.review.is_none());
        assert_eq!(h.app.focus, Focus::Checkouts);
    }

    #[test]
    fn navigation_keys_move_within_the_diff_not_the_tree() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));
        let before = (h.app.sel_project, h.app.sel_checkout, h.app.sel_pane);

        h.key(KeyCode::Char('j'));

        assert_eq!(
            (h.app.sel_project, h.app.sel_checkout, h.app.sel_pane),
            before,
            "j belongs to the diff while it's up"
        );
        let v = h.app.review.as_ref().unwrap();
        assert_eq!(v.anchor().unwrap().text, vec!["+new"]);
    }

    #[test]
    fn r_inside_the_review_re_requests_rather_than_reusing_a_stale_diff() {
        // An agent is very likely still editing the tree underneath it.
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));

        h.key(KeyCode::Char('r'));
        assert!(matches!(h.sent()[0], ClientMsg::Review { .. }));
    }

    #[test]
    fn v_then_j_selects_a_range_of_lines() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));

        h.key(KeyCode::Char('v'));
        h.key(KeyCode::Char('j'));

        let a = h.app.review.as_ref().unwrap().anchor().unwrap();
        assert_eq!(a.path, "src/a.rs");
        assert_eq!(a.text, vec![" keep", "+new"]);
    }

    #[test]
    fn typing_in_the_review_never_reaches_a_pane() {
        // The review shares its column with the live pane; a keystroke
        // leaking into the child would be silent and destructive.
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));

        h.keys("jkgG");
        assert!(
            !h.sent()
                .iter()
                .any(|m| matches!(m, ClientMsg::Input { .. })),
            "no input should be forwarded"
        );
    }

    /// A tree whose first checkout has an agent pane running in it.
    fn tree_with_agent() -> Vec<ProjectInfo> {
        let mut t = tree();
        t[0].repositories[0].checkouts[0].panes = vec![
            PaneInfo {
                id: PaneId(50),
                kind: PaneKind::Shell,
                title: "sh".to_string(),
                status: PaneStatus::Idle,
                note: None,
                template: None,
                children: Vec::new(),
            },
            PaneInfo {
                id: PaneId(51),
                kind: PaneKind::Agent,
                title: "claude".to_string(),
                status: PaneStatus::Idle,
                note: None,
                template: None,
                children: Vec::new(),
            },
        ];
        t
    }

    fn review_with_agent() -> Harness {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Tree(tree_with_agent()));
        h.sent();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));
        h
    }

    #[test]
    fn c_opens_a_comment_prompt_anchored_to_the_cursor() {
        let mut h = review_with_agent();
        h.key(KeyCode::Char('c'));
        match &h.app.prompt {
            Some(Prompt::Comment { anchor, .. }) => assert_eq!(anchor.path, "src/a.rs"),
            _ => panic!("no comment prompt"),
        }
    }

    #[test]
    fn a_comment_is_typed_at_the_agent_and_submitted() {
        let mut h = review_with_agent();
        let checkout = h.app.review.as_ref().unwrap().review.checkout;
        h.key(KeyCode::Char('j'));
        h.key(KeyCode::Char('c'));
        h.keys("fix this");
        h.key(KeyCode::Enter);

        match &h.sent()[0] {
            ClientMsg::ReviewComment {
                checkout: sent_checkout,
                recipient,
                anchor,
                body,
            } => {
                assert_eq!(*sent_checkout, checkout);
                assert_eq!(*recipient, PaneId(51), "the agent, not the shell");
                assert_eq!(anchor.notification(body), "src/a.rs:2 `+new`: fix this");
                assert_eq!(body, "fix this");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(h.app.prompt.is_none());
    }

    #[test]
    fn a_comment_chooses_between_multiple_live_agents() {
        let mut h = review_with_agent();
        h.app.tree[0].repositories[0].checkouts[0]
            .panes
            .push(PaneInfo {
                id: PaneId(52),
                kind: PaneKind::Agent,
                title: "fix tests".to_string(),
                status: PaneStatus::Working,
                note: None,
                template: Some("codex".to_string()),
                children: Vec::new(),
            });

        h.key(KeyCode::Char('c'));
        h.keys("route this");
        h.key(KeyCode::Enter);

        let picker = h.app.picker.as_ref().expect("recipient picker");
        assert!(matches!(
            &picker.kind,
            PickerKind::ReviewRecipient { panes, .. }
                if panes == &[PaneId(51), PaneId(52)]
        ));
        assert!(picker.items[1].contains("fix tests"));
        assert!(picker.items[1].contains("codex"));
        assert!(picker.items[1].contains("#52"));
        assert!(h.sent().is_empty(), "nothing is sent before choosing");

        h.key(KeyCode::Char('j'));
        h.key(KeyCode::Enter);
        assert!(matches!(
            &h.sent()[0],
            ClientMsg::ReviewComment { recipient: PaneId(52), body, .. }
                if body == "route this"
        ));
    }

    #[test]
    fn an_exited_agent_is_not_offered_as_a_comment_recipient() {
        let mut h = review_with_agent();
        h.app.tree[0].repositories[0].checkouts[0]
            .panes
            .push(PaneInfo {
                id: PaneId(52),
                kind: PaneKind::Agent,
                title: "old agent".to_string(),
                status: PaneStatus::Exited { code: Some(0) },
                note: None,
                template: Some("codex".to_string()),
                children: Vec::new(),
            });

        h.key(KeyCode::Char('c'));
        h.keys("only live agents");
        h.key(KeyCode::Enter);

        assert!(h.app.picker.is_none());
        assert!(matches!(
            h.sent()[0],
            ClientMsg::ReviewComment {
                recipient: PaneId(51),
                ..
            }
        ));
    }

    #[test]
    fn a_comment_with_no_agent_to_read_it_says_so_and_sends_nothing() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));
        h.key(KeyCode::Char('c'));
        h.keys("hello");
        h.key(KeyCode::Enter);

        assert!(h.sent().is_empty());
        assert!(h.app.status.contains("no agent"), "{}", h.app.status);
    }

    #[test]
    fn an_empty_comment_sends_nothing() {
        let mut h = review_with_agent();
        h.key(KeyCode::Char('c'));
        h.key(KeyCode::Enter);
        assert!(h.sent().is_empty());
        assert!(h.app.prompt.is_none());
    }

    #[test]
    fn escaping_the_comment_prompt_sends_nothing_and_leaves_the_review_up() {
        let mut h = review_with_agent();
        h.key(KeyCode::Char('c'));
        h.keys("never mind");
        h.key(KeyCode::Esc);

        assert!(h.sent().is_empty());
        assert!(h.app.prompt.is_none());
        assert!(h.app.review.is_some());
        assert_eq!(h.app.focus, Focus::Review);
    }

    #[test]
    fn a_comment_on_a_range_is_sent_as_one_message() {
        let mut h = review_with_agent();
        h.key(KeyCode::Char('v'));
        h.key(KeyCode::Char('j'));
        h.key(KeyCode::Char('c'));
        h.keys("both lines");
        h.key(KeyCode::Enter);

        let sent = h.sent();
        assert_eq!(sent.len(), 1);
        match &sent[0] {
            ClientMsg::ReviewComment { anchor, body, .. } => {
                assert_eq!(
                    anchor.notification(body),
                    "src/a.rs:1-2 (2 lines): both lines"
                )
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_saved_comment_reports_delivery_from_the_daemon() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::ReviewCommentSaved {
            id: 7,
            delivered: true,
        });
        assert_eq!(h.app.status, "comment #7 saved and sent");

        h.app.on_server_msg(ServerMsg::ReviewCommentSaved {
            id: 8,
            delivered: false,
        });
        assert_eq!(h.app.status, "comment #8 saved; agent unavailable");
    }

    #[test]
    fn review_keys_do_not_leak_into_the_comment_being_typed() {
        let mut h = review_with_agent();
        h.key(KeyCode::Char('c'));
        h.keys("jkgG");
        match &h.app.prompt {
            Some(Prompt::Comment { input, .. }) => assert_eq!(input, "jkgG"),
            _ => panic!("no comment prompt"),
        }
    }

    #[test]
    fn n_cycles_through_panes_that_need_attention() {
        let mut h = Harness::new();
        let mut updated = tree();
        updated[0].repositories[0].checkouts[0].panes[1].status = PaneStatus::Waiting;
        updated[0].repositories[0].checkouts[0].panes[1].note =
            Some("needs a password".to_string());
        let mut review_pane = pane(102, "review agent");
        review_pane.status = PaneStatus::NeedsReview;
        updated[0].repositories[0].checkouts[1]
            .panes
            .push(review_pane);
        h.app.on_server_msg(ServerMsg::Tree(updated));
        h.sent();

        h.key(KeyCode::Char('N'));
        assert_eq!(h.app.column_pane(), Some(PaneId(101)));
        assert_eq!(h.app.focus, Focus::PaneContent);
        assert!(h.app.status.contains("needs a password"));

        h.leader();
        h.key(KeyCode::Char('N'));
        assert_eq!(h.app.column_pane(), Some(PaneId(102)));

        h.leader();
        h.key(KeyCode::Char('N'));
        assert_eq!(h.app.column_pane(), Some(PaneId(101)), "cycles at the end");
        assert!(
            !h.sent()
                .iter()
                .any(|message| matches!(message, ClientMsg::Input { .. })),
            "the leader chord must not reach a child"
        );
    }

    #[test]
    fn n_reports_when_no_pane_needs_attention() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('N'));
        assert!(h.app.status.contains("no panes need attention"));
        assert_eq!(h.app.focus, Focus::Projects);
    }

    #[test]
    fn n_opens_the_parent_of_a_child_that_needs_attention() {
        let mut h = Harness::new();
        let mut updated = tree();
        updated[0].repositories[0].checkouts[0].panes[1]
            .children
            .push(argus_protocol::ChildAgentInfo {
                label: "database helper".to_string(),
                status: PaneStatus::Waiting,
                note: Some("needs credentials".to_string()),
            });
        h.app.on_server_msg(ServerMsg::Tree(updated));

        h.key(KeyCode::Char('N'));

        assert_eq!(h.app.column_pane(), Some(PaneId(101)));
        assert!(h.app.status.contains("claude / database helper"));
        assert!(h.app.status.contains("needs credentials"));
    }

    #[test]
    fn e_opens_the_file_under_the_cursor_at_its_line() {
        let mut h = review_with_agent();
        h.key(KeyCode::Char('j'));
        h.key(KeyCode::Char('e'));

        match &h.sent()[0] {
            ClientMsg::OpenInEditor { path, line, .. } => {
                assert_eq!(path, "src/a.rs");
                assert_eq!(*line, Some(2));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn opening_an_editor_gives_it_the_column_the_review_was_using() {
        let mut h = review_with_agent();
        h.key(KeyCode::Char('e'));
        assert!(h.app.review.is_none());
    }

    #[test]
    fn b_toggles_the_diff_side_and_asks_again() {
        let mut h = review_with_agent();
        h.key(KeyCode::Char('b'));

        match &h.sent()[0] {
            ClientMsg::Review { base, .. } => {
                assert_eq!(*base, argus_protocol::ReviewBase::Staged)
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_chosen_base_sticks_across_reopens() {
        // `b` is a setting, not a per-visit choice.
        let mut h = review_with_agent();
        h.key(KeyCode::Char('b'));
        h.key(KeyCode::Esc);
        h.sent();

        h.key(KeyCode::Char('R'));
        match &h.sent()[0] {
            ClientMsg::Review { base, .. } => {
                assert_eq!(*base, argus_protocol::ReviewBase::Staged)
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn b_on_a_commit_review_leaves_the_side_setting_alone() {
        // The side toggle is uncommitted-only. Flipping it here would
        // change which side the next `R` opens on, with nothing on
        // screen to show that it happened.
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        let mut review = diff_of(checkout);
        review.base = argus_protocol::ReviewBase::Commit;
        review.commit = Some(argus_protocol::CommitInfo {
            oid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            short: "aaaaaaa".into(),
            summary: "fix the thing".into(),
            author: "t".into(),
            time: 0,
        });
        open_review(&mut h, review);

        h.key(KeyCode::Char('b'));
        assert_eq!(h.app.review_base, argus_protocol::ReviewBase::Unstaged);
        assert!(h.sent().is_empty(), "b must not re-request a commit");

        h.key(KeyCode::Esc);
        h.key(KeyCode::Char('R'));
        match &h.sent()[0] {
            ClientMsg::Review { base, commit, .. } => {
                assert_eq!(*base, argus_protocol::ReviewBase::Unstaged);
                assert!(commit.is_none());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn tab_reaches_the_review_from_the_tree_and_from_inside_a_pane() {
        // §5's entry point. Inside a pane it needs the leader, since a bare
        // Tab there belongs to the child.
        let mut h = Harness::new();
        h.key(KeyCode::Char('l'));
        h.key(KeyCode::Tab);
        assert!(matches!(h.sent()[0], ClientMsg::Review { .. }));

        h.keys("l");
        h.key(KeyCode::Char('s'));
        h.sent();
        h.app.focus = Focus::PaneContent;
        h.leader();
        h.key(KeyCode::Tab);
        assert!(
            h.sent()
                .iter()
                .any(|m| matches!(m, ClientMsg::Review { .. })),
            "leader-Tab should ask for the diff"
        );
    }

    #[test]
    fn a_review_restores_the_columns_after_a_fullscreen_pane() {
        let mut h = Harness::new();
        h.keys("llll");
        let checkout = h.app.current_checkout().unwrap().id;
        h.leader();
        h.key(KeyCode::Char('f'));
        h.leader();
        h.key(KeyCode::Tab);
        h.sent();

        h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));

        assert_eq!(h.app.focus, Focus::Review);
        assert!(!h.app.pane_fullscreen);
    }

    #[test]
    fn t_opens_the_theme_picker_on_the_theme_already_in_use() {
        let mut h = Harness::new();
        h.app.theme = crate::theme::Theme::by_name("frappe");
        h.key(KeyCode::Char('t'));

        let picker = h.app.picker.as_ref().expect("t should open the picker");
        assert_eq!(picker.items[picker.sel], "frappe");
    }

    #[test]
    fn choosing_a_theme_swaps_the_palette_without_asking_the_daemon() {
        // The palette is the client's business; the daemon has no opinion.
        let mut h = Harness::new();
        h.key(KeyCode::Char('t'));
        h.key(KeyCode::Char('j'));
        h.key(KeyCode::Enter);

        assert_eq!(h.app.theme, crate::theme::Theme::by_name("macchiato"));
        assert!(h.sent().is_empty());
        assert!(h.app.picker.is_none());
    }

    #[test]
    fn escaping_the_theme_picker_leaves_the_palette_alone() {
        let mut h = Harness::new();
        let before = h.app.theme;
        h.key(KeyCode::Char('t'));
        h.key(KeyCode::Char('j'));
        h.key(KeyCode::Esc);
        assert_eq!(h.app.theme, before);
    }

    #[test]
    fn clicking_the_content_frame_returns_to_what_it_is_showing() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app.on_mouse(click(48, 0)); // the card's corner, not the live grid
        assert_eq!(h.app.focus, Focus::PaneContent);
    }

    #[test]
    fn clicking_an_empty_content_column_does_not_trap_focus_there() {
        // Focusing a pane that doesn't exist is a mode with no keys in it.
        let mut h = Harness::new();
        let mut t = tree();
        t[0].repositories[0].checkouts[0].panes.clear();
        t[0].repositories[0].checkouts[1].panes.clear();
        h.app.on_server_msg(ServerMsg::Tree(t));
        laid_out(&mut h);
        h.sent();

        h.app.on_mouse(click(36, 0));
        assert_ne!(h.app.focus, Focus::PaneContent);
    }

    #[test]
    fn clicking_a_column_leaves_the_live_pane_subscribed() {
        // "Move over there" must not tear down the session you were on.
        let mut h = Harness::new();
        laid_out(&mut h);
        h.keys("lll"); // down into the panes column, subscribing
        h.sent();
        let watching = h.app.column_pane();
        assert!(watching.is_some(), "precondition: something is being shown");

        h.app.on_mouse(click(0, 0)); // all the way back to projects

        assert_eq!(h.app.focus, Focus::Projects);
        assert_eq!(h.app.column_pane(), watching, "still showing the same pane");
    }

    // --- history -------------------------------------------------------------

    fn commit(oid: &str, summary: &str) -> argus_protocol::CommitInfo {
        argus_protocol::CommitInfo {
            oid: oid.to_string(),
            short: oid.chars().take(7).collect(),
            summary: summary.to_string(),
            author: "hunt".to_string(),
            time: 0,
        }
    }

    fn touched(path: &str) -> argus_protocol::CommitFile {
        argus_protocol::CommitFile {
            path: path.to_string(),
            old_path: None,
            kind: argus_protocol::ChangeKind::Modified,
            added: 1,
            removed: 1,
        }
    }

    /// Presses `H` on the first checkout and answers with two commits.
    fn open_history(h: &mut Harness) -> CheckoutId {
        h.key(KeyCode::Char('l'));
        let checkout = h.app.current_checkout().unwrap().id;
        h.key(KeyCode::Char('H'));
        let request_id = match &h.sent()[0] {
            ClientMsg::ListCommits { request_id, .. } => *request_id,
            other => panic!("unexpected {other:?}"),
        };
        h.app.on_server_msg(ServerMsg::Commits {
            request_id,
            checkout,
            commits: vec![commit("aaaa111", "newest"), commit("bbbb222", "older")],
        });
        checkout
    }

    fn commit_files_arrive(h: &mut Harness, checkout: CheckoutId, oid: &str) {
        h.app.on_server_msg(ServerMsg::CommitFiles {
            checkout,
            commit: oid.to_string(),
            files: vec![touched("src/a.rs")],
        });
    }

    #[test]
    fn opening_history_asks_for_commits_and_nothing_else() {
        let mut h = Harness::new();
        let checkout = open_history(&mut h);
        assert!(h.app.history.is_some());
        assert!(matches!(h.app.overlay, Some(Overlay::History)));
        assert!(
            h.sent().is_empty(),
            "no commit is summarized until it is drilled into"
        );
        assert_eq!(h.app.history.as_ref().unwrap().checkout, checkout);
        assert_eq!(
            h.app.history.as_ref().unwrap().rows.len(),
            2,
            "two headers, no file rows"
        );
    }

    #[test]
    fn drilling_into_a_commit_asks_for_that_commit_alone() {
        let mut h = Harness::new();
        let checkout = open_history(&mut h);

        h.key(KeyCode::Char('l'));
        match h.sent().as_slice() {
            [ClientMsg::ListCommitFiles { checkout: c, commit }] => {
                assert_eq!(*c, checkout);
                assert_eq!(commit, "aaaa111");
            }
            other => panic!("unexpected {other:?}"),
        }

        commit_files_arrive(&mut h, checkout, "aaaa111");
        let view = h.app.history.as_ref().unwrap();
        assert_eq!(view.rows.len(), 3);
        assert_eq!(view.commits[0].files.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn drilling_into_an_unfolded_commit_opens_it_as_a_review() {
        let mut h = Harness::new();
        let checkout = open_history(&mut h);
        h.key(KeyCode::Char('l'));
        commit_files_arrive(&mut h, checkout, "aaaa111");
        h.sent();

        h.key(KeyCode::Char('l'));
        match h.sent().as_slice() {
            [ClientMsg::Review { base, commit, .. }] => {
                assert_eq!(*base, argus_protocol::ReviewBase::Commit);
                assert_eq!(commit.as_deref(), Some("aaaa111"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn h_folds_the_commit_before_it_closes_the_overlay() {
        let mut h = Harness::new();
        let checkout = open_history(&mut h);
        h.key(KeyCode::Char('l'));
        commit_files_arrive(&mut h, checkout, "aaaa111");

        h.key(KeyCode::Char('h'));
        assert!(
            matches!(h.app.overlay, Some(Overlay::History)),
            "folded, not closed"
        );
        assert_eq!(h.app.history.as_ref().unwrap().rows.len(), 2);

        h.key(KeyCode::Char('h'));
        assert!(h.app.overlay.is_none());
    }

    #[test]
    fn a_summary_for_a_checkout_the_user_left_is_dropped() {
        let mut h = Harness::new();
        open_history(&mut h);
        h.key(KeyCode::Char('l'));

        commit_files_arrive(&mut h, CheckoutId(9999), "aaaa111");
        assert!(h.app.history.as_ref().unwrap().commits[0].files.is_none());
    }

    // --- fuzzy pickers ------------------------------------------------------

    fn branches_arrive(h: &mut Harness, list: &[&str]) {
        h.key(KeyCode::Char('l')); // into the checkouts column
        h.key(KeyCode::Char('b'));
        let checkout = match h.sent().into_iter().next() {
            Some(ClientMsg::ListBranches { checkout }) => checkout,
            other => panic!("unexpected {other:?}"),
        };
        h.app.on_server_msg(ServerMsg::Branches {
            checkout,
            branches: list.iter().map(|s| s.to_string()).collect(),
        });
    }

    #[test]
    fn b_asks_for_the_branches_and_opens_only_when_they_arrive() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('l'));
        h.key(KeyCode::Char('b'));
        assert!(
            h.app.picker.is_none(),
            "no picker until the list is in hand"
        );
        assert!(matches!(h.sent()[0], ClientMsg::ListBranches { .. }));
    }

    #[test]
    fn the_branch_you_are_on_is_a_label_not_a_row_to_switch_to() {
        // Switching to the branch you are already on does nothing, so
        // offering it is a row that can only waste a keystroke.
        let mut h = Harness::new();
        branches_arrive(&mut h, &["master", "feature/login", "hotfix"]);

        let p = h.app.picker.as_ref().unwrap();
        assert_eq!(p.items, vec!["feature/login", "hotfix"]);
        assert!(h.app.status.contains("on master"), "{}", h.app.status);
    }

    #[test]
    fn typing_filters_the_branches() {
        let mut h = Harness::new();
        branches_arrive(&mut h, &["master", "feature/login", "hotfix"]);
        h.keys("log");
        assert_eq!(
            h.app.picker.as_ref().unwrap().selected(),
            Some("feature/login")
        );
    }

    #[test]
    fn enter_switches_to_the_branch_under_the_cursor() {
        let mut h = Harness::new();
        branches_arrive(&mut h, &["master", "feature/login", "hotfix"]);
        h.keys("hot");
        h.key(KeyCode::Enter);

        match &h.sent()[0] {
            ClientMsg::SwitchBranch { branch, .. } => assert_eq!(branch, "hotfix"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_query_naming_no_branch_offers_to_create_it() {
        // Making a branch and switching to one are the same intent, so they
        // are the same gesture — but the row is explicit, never implied.
        let mut h = Harness::new();
        branches_arrive(&mut h, &["master", "hotfix"]);
        h.keys("wip");

        let p = h.app.picker.as_ref().unwrap();
        assert_eq!(p.create.as_deref(), Some("wip"));
        assert!(p.shown.is_empty(), "nothing existing matches");
    }

    #[test]
    fn a_query_that_names_an_existing_branch_does_not_offer_to_create_it() {
        let mut h = Harness::new();
        branches_arrive(&mut h, &["master", "hotfix"]);
        h.keys("hotfix");
        assert_eq!(h.app.picker.as_ref().unwrap().create, None);
    }

    #[test]
    fn choosing_the_create_row_makes_the_branch_here() {
        let mut h = Harness::new();
        branches_arrive(&mut h, &["master", "hotfix"]);
        h.keys("wip");
        h.key(KeyCode::Enter);

        match &h.sent()[0] {
            ClientMsg::CreateBranch { branch, .. } => assert_eq!(branch, "wip"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_create_row_sits_below_the_matches_rather_than_replacing_them() {
        let mut h = Harness::new();
        branches_arrive(&mut h, &["master", "hotfix", "hotfix-2"]);
        h.keys("hotfix-");

        let p = h.app.picker.as_ref().unwrap();
        assert_eq!(p.selected(), Some("hotfix-2"), "the match is still first");
        assert_eq!(p.create.as_deref(), Some("hotfix-"));

        // Enter on the top row switches; you have to move down to create.
        h.key(KeyCode::Down);
        h.key(KeyCode::Enter);
        assert!(matches!(h.sent()[0], ClientMsg::CreateBranch { .. }));
    }

    #[test]
    fn backspace_widens_the_query_again() {
        let mut h = Harness::new();
        branches_arrive(&mut h, &["master", "feature/login", "hotfix"]);
        h.keys("log");
        assert_eq!(h.app.picker.as_ref().unwrap().shown.len(), 1);

        for _ in 0..3 {
            h.key(KeyCode::Backspace);
        }
        assert_eq!(h.app.picker.as_ref().unwrap().shown.len(), 2);
    }

    #[test]
    fn j_and_k_are_text_in_a_fuzzy_picker() {
        // A branch with a j or a k in its name has to be typeable.
        let mut h = Harness::new();
        branches_arrive(&mut h, &["master", "jkl", "other"]);
        h.keys("jk");
        assert_eq!(h.app.picker.as_ref().unwrap().query, "jk");
        assert_eq!(h.app.picker.as_ref().unwrap().selected(), Some("jkl"));
    }

    #[test]
    fn j_and_k_still_move_in_the_short_pickers() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('t'));
        h.key(KeyCode::Char('j'));
        assert_eq!(h.app.picker.as_ref().unwrap().sel, 1);
    }

    #[test]
    fn esc_closes_a_fuzzy_picker_without_switching_anything() {
        let mut h = Harness::new();
        branches_arrive(&mut h, &["master", "hotfix"]);
        h.keys("hot");
        h.key(KeyCode::Esc);
        assert!(h.app.picker.is_none());
        assert!(h.sent().is_empty());
    }

    #[test]
    fn a_stale_branch_list_is_dropped() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('l'));
        h.key(KeyCode::Char('b'));
        h.sent();
        h.app.on_server_msg(ServerMsg::Branches {
            checkout: CheckoutId(9999),
            branches: vec!["whatever".to_string()],
        });
        assert!(h.app.picker.is_none());
    }

    // --- files --------------------------------------------------------------

    fn files_arrive(h: &mut Harness, list: &[&str]) {
        h.key(KeyCode::Char('l'));
        h.key(KeyCode::Char('f'));
        let checkout = match h.sent().into_iter().next() {
            Some(ClientMsg::ListFiles { checkout }) => checkout,
            other => panic!("unexpected {other:?}"),
        };
        h.app.on_server_msg(ServerMsg::Files {
            checkout,
            files: list.iter().map(|s| s.to_string()).collect(),
        });
    }

    #[test]
    fn f_opens_the_chosen_file_in_the_editor() {
        let mut h = Harness::new();
        files_arrive(&mut h, &["src/app.rs", "src/ui.rs", "README.md"]);
        h.keys("ui");
        h.key(KeyCode::Enter);

        match &h.sent()[0] {
            ClientMsg::OpenInEditor { path, line, .. } => {
                assert_eq!(path, "src/ui.rs");
                assert_eq!(*line, None, "no particular line was asked for");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_checkout_with_no_files_says_so_rather_than_opening_an_empty_picker() {
        let mut h = Harness::new();
        files_arrive(&mut h, &[]);
        assert!(h.app.picker.is_none());
        assert!(h.app.status.contains("no files"), "{}", h.app.status);
    }

    #[test]
    fn the_file_picker_never_offers_to_create_anything() {
        // That row belongs to branches; a typo here should find nothing,
        // not offer to invent a file.
        let mut h = Harness::new();
        files_arrive(&mut h, &["src/app.rs"]);
        h.keys("zzzz");
        let p = h.app.picker.as_ref().unwrap();
        assert_eq!(p.create, None);
        assert_eq!(p.len(), 0);
    }

    // --- changes ------------------------------------------------------------

    #[test]
    fn f_in_the_review_jumps_the_cursor_to_the_chosen_file() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        let mut review = diff_of(checkout);
        let mut second = review.files[0].clone();
        second.path = "src/other.rs".to_string();
        review.files.push(second);
        open_review(&mut h, review);

        h.key(KeyCode::Char('f'));
        h.keys("other");
        h.key(KeyCode::Enter);

        let a = h.app.review.as_ref().unwrap().anchor().unwrap();
        assert_eq!(a.path, "src/other.rs");
        assert!(h.app.picker.is_none());
    }

    #[test]
    fn the_change_picker_lists_the_files_with_their_markers() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));
        h.key(KeyCode::Char('f'));
        assert_eq!(h.app.picker.as_ref().unwrap().items, vec!["M src/a.rs"]);
    }

    // --- floating windows ---------------------------------------------------

    #[test]
    fn a_floating_pane_streams_alongside_the_column_not_instead_of_it() {
        // Opening a file must not cost you sight of the agent behind it.
        let mut h = Harness::new();
        h.keys("lll"); // watching the column's pane
        h.sent();
        let column = h.app.column_pane().unwrap();

        h.app
            .open_overlay_pane(PaneId(700), "vim".to_string(), false);

        assert_eq!(h.app.overlay_pane(), Some(PaneId(700)));
        assert_eq!(h.app.column_pane(), Some(column), "the column is untouched");
        assert!(h.app.grids.contains_key(&column), "and still streaming");
        assert_eq!(h.app.focus, Focus::Overlay);
        assert!(matches!(
            h.sent().last(),
            Some(ClientMsg::Subscribe { pane: PaneId(700) })
        ));
    }

    #[test]
    fn a_floating_pane_restores_the_columns_before_taking_focus() {
        let mut h = Harness::new();
        h.keys("llll");
        h.leader();
        h.key(KeyCode::Char('f'));

        h.app
            .open_overlay_pane(PaneId(700), "vim".to_string(), false);

        assert_eq!(h.app.focus, Focus::Overlay);
        assert!(!h.app.pane_fullscreen);
    }

    #[test]
    fn typing_in_a_floating_pane_reaches_its_child() {
        let mut h = Harness::new();
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.sent();

        h.keys("iabc");

        let typed: Vec<u8> = h
            .sent()
            .into_iter()
            .filter_map(|m| match m {
                ClientMsg::Input {
                    pane: PaneId(101),
                    bytes,
                } => Some(bytes),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(typed, b"iabc");
    }

    #[test]
    fn nav_keys_do_not_leak_out_of_a_floating_pane() {
        // Every key belongs to the editor while it is up — `q` especially.
        let mut h = Harness::new();
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.keys("q");
        assert!(!h.app.should_quit, "q is the editor's, not ours");
        assert!(h.app.overlay.is_some());
    }

    #[test]
    fn the_leader_closes_a_floating_pane_and_leaves_it_running() {
        let mut h = Harness::new();
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.sent();

        h.leader();
        h.key(KeyCode::Esc);

        assert!(h.app.overlay.is_none());
        assert!(
            !h.sent().iter().any(|m| matches!(m, ClientMsg::Kill { .. })),
            "closing the window must not kill the editor"
        );
    }

    #[test]
    fn the_leader_can_also_kill_the_pane_in_a_floating_window() {
        let mut h = Harness::new();
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.sent();

        h.leader();
        h.key(KeyCode::Char('x'));

        assert!(h
            .sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Kill { pane } if *pane == PaneId(101))));
        assert!(h.app.overlay.is_none());
    }

    #[test]
    fn closing_a_floating_pane_puts_the_live_view_back_on_the_column() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        let was = h.app.column_pane();

        h.app
            .open_overlay_pane(PaneId(999), "vim".to_string(), false);
        h.leader();
        h.key(KeyCode::Esc);

        assert_eq!(h.app.column_pane(), was, "back to what the columns show");
        assert!(
            !h.app.grids.contains_key(&PaneId(999)),
            "and the editor is dropped"
        );
    }

    #[test]
    fn a_floating_pane_and_the_column_are_sized_separately() {
        let mut h = Harness::new();
        laid_out(&mut h);
        assert_eq!(h.app.live_panes()[0].1, h.app.layout.content.inner);

        h.app
            .open_overlay_pane(PaneId(700), "vim".to_string(), false);
        h.app.layout.overlay = Panel {
            outer: Rect::new(2, 1, 40, 20),
            inner: Rect::new(3, 2, 38, 18),
            first: 0,
        };
        let live = h.app.live_panes();
        assert_eq!(live.len(), 2);
        assert_eq!(live[1].1, h.app.layout.overlay.inner);
    }

    // --- settings -----------------------------------------------------------

    #[test]
    fn shift_s_opens_the_settings_panel() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('S'));
        assert!(matches!(h.app.overlay, Some(Overlay::Settings { .. })));
        assert_eq!(h.app.focus, Focus::Overlay);
    }

    #[test]
    fn h_and_l_change_the_setting_under_the_cursor() {
        let mut h = Harness::new();
        h.app.open_settings();
        let before = h.app.settings.editor;

        h.key(KeyCode::Char('l'));
        assert_eq!(h.app.settings.editor, before.next());

        h.key(KeyCode::Char('h'));
        assert_eq!(h.app.settings.editor, before, "and back again");
    }

    #[test]
    fn changing_the_theme_applies_it_at_once() {
        // There is no save button, so there is nothing to forget to press.
        let mut h = Harness::new();
        h.app.open_settings();
        let theme_row = Setting::ALL
            .iter()
            .position(|setting| *setting == Setting::Theme)
            .unwrap();
        for _ in 0..theme_row {
            h.key(KeyCode::Char('j'));
        }
        h.key(KeyCode::Char('l'));

        assert_eq!(
            h.app.theme,
            crate::theme::Theme::by_name(&h.app.settings.theme)
        );
        assert_ne!(h.app.settings.theme, "mocha");
    }

    #[test]
    fn the_initial_tree_is_a_quiet_baseline() {
        let mut h = Harness::new();

        assert!(h.app.next_flash_deadline().is_none());
        assert!(!h.app.take_bell());
        assert!(h.app.status.is_empty());
    }

    #[test]
    fn every_state_has_an_accurate_notification_word() {
        let cases = [
            (PaneStatus::Idle, "idle"),
            (PaneStatus::Working, "working"),
            (PaneStatus::Waiting, "needs attention"),
            (PaneStatus::NeedsReview, "needs review"),
            (PaneStatus::Done, "done"),
            (PaneStatus::Failed, "failed"),
            (PaneStatus::Exited { code: Some(0) }, "exited"),
            (
                PaneStatus::Exited { code: Some(1) },
                "exited unsuccessfully",
            ),
            (PaneStatus::Exited { code: None }, "exited unsuccessfully"),
        ];

        for (status, word) in cases {
            assert_eq!(state_word(status), word, "for {status:?}");
        }
    }

    #[test]
    fn an_actionable_transition_flashes_and_explains_the_pane() {
        let mut h = Harness::new();
        let mut next = tree();
        let pane = &mut next[0].repositories[0].checkouts[0].panes[1];
        pane.status = PaneStatus::Waiting;
        pane.note = Some("needs the staging password".to_string());

        h.app.on_server_msg(ServerMsg::Tree(next));

        assert!(h.app.pane_is_flashing(PaneId(101)));
        assert!(h.app.status.contains("claude: needs the staging password"));
        assert!(h.app.status_alert);
        let deadline = h.app.next_flash_deadline().unwrap();
        h.app.expire_state_flashes(deadline);
        assert!(!h.app.pane_is_flashing(PaneId(101)));
    }

    #[test]
    fn the_bell_is_opt_in_and_only_consumed_once() {
        let mut h = Harness::new();
        h.app.settings.notifications = crate::settings::NotificationMode::Bell;
        let mut next = tree();
        next[0].repositories[0].checkouts[0].panes[1].status = PaneStatus::NeedsReview;

        h.app.on_server_msg(ServerMsg::Tree(next));

        assert!(h.app.take_bell());
        assert!(!h.app.take_bell());
    }

    #[test]
    fn a_child_transition_flashes_and_names_its_parent() {
        let mut h = Harness::new();
        let mut working = tree();
        let pane = &mut working[0].repositories[0].checkouts[0].panes[1];
        pane.status = PaneStatus::Working;
        pane.children.push(argus_protocol::ChildAgentInfo {
            label: "test runner".to_string(),
            status: PaneStatus::Working,
            note: None,
        });
        h.app.on_server_msg(ServerMsg::Tree(working.clone()));
        h.app
            .expire_state_flashes(std::time::Instant::now() + STATE_FLASH);
        working[0].repositories[0].checkouts[0].panes[1].children[0].status = PaneStatus::Failed;
        working[0].repositories[0].checkouts[0].panes[1].children[0].note =
            Some("unit tests failed".to_string());

        h.app.on_server_msg(ServerMsg::Tree(working));

        assert!(h.app.pane_is_flashing(PaneId(101)));
        assert!(h
            .app
            .status
            .contains("claude / test runner: unit tests failed"));
    }

    #[test]
    fn the_settings_cursor_stops_at_the_ends() {
        let mut h = Harness::new();
        h.app.open_settings();
        for _ in 0..10 {
            h.key(KeyCode::Char('j'));
        }
        let Some(Overlay::Settings { sel }) = h.app.overlay else {
            panic!("no settings panel")
        };
        assert_eq!(sel, crate::app::Setting::ALL.len() - 1);
    }

    #[test]
    fn esc_closes_the_settings_panel() {
        let mut h = Harness::new();
        h.app.open_settings();
        h.key(KeyCode::Esc);
        assert!(h.app.overlay.is_none());
    }

    #[test]
    fn settings_keys_never_reach_a_pane() {
        // The panel shares the overlay slot with a live editor; leaking a
        // keystroke into a child would be silent and destructive.
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.app.open_settings();

        h.keys("jklh");

        assert!(!h
            .sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Input { .. })));
    }

    // --- where an editor opens ----------------------------------------------

    /// Opens an editor without disturbing where the columns are pointed —
    /// `open_review` drives keys from the projects column, which would
    /// move the selection this is trying to observe.
    fn editor_arrives(h: &mut Harness) {
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        h.app.review_for_test(checkout);
        h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
        h.key(KeyCode::Char('e'));
        h.sent();
        let mut t = tree();
        t[0].repositories[0].checkouts[0].panes.push(PaneInfo {
            id: PaneId(700),
            kind: PaneKind::Editor,
            title: "a.rs".to_string(),
            status: PaneStatus::Idle,
            note: None,
            template: None,
            children: Vec::new(),
        });
        h.app.on_server_msg(ServerMsg::Tree(t));
    }

    fn open_editor_from_review(h: &mut Harness) {
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(h, diff_of(checkout));
        h.key(KeyCode::Char('e'));
        h.sent();
        // The daemon answers with a tree carrying the new editor pane.
        let mut t = tree();
        t[0].repositories[0].checkouts[0].panes.push(PaneInfo {
            id: PaneId(700),
            kind: PaneKind::Editor,
            title: "a.rs".to_string(),
            status: PaneStatus::Idle,
            note: None,
            template: None,
            children: Vec::new(),
        });
        h.app.on_server_msg(ServerMsg::Tree(t));
    }

    #[test]
    fn an_editor_opens_in_a_floating_window_by_default() {
        let mut h = Harness::new();
        open_editor_from_review(&mut h);
        assert!(matches!(h.app.overlay, Some(Overlay::Pane { pane, .. }) if pane == PaneId(700)));
    }

    #[test]
    fn the_column_setting_keeps_the_editor_in_the_column() {
        let mut h = Harness::new();
        h.app.settings.editor = crate::settings::EditorMode::Column;
        open_editor_from_review(&mut h);

        assert!(h.app.overlay.is_none());
        assert_eq!(h.app.focus, Focus::PaneContent);
    }

    #[test]
    fn an_external_editor_asks_the_daemon_not_to_make_a_pane() {
        let mut h = Harness::new();
        h.app.settings.editor = crate::settings::EditorMode::External;
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));
        h.key(KeyCode::Char('e'));

        match &h.sent()[0] {
            ClientMsg::OpenInEditor { external, .. } => assert!(*external),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn an_external_editor_does_not_steal_focus_when_the_tree_changes() {
        // It has no pane to focus, and grabbing the newest one would land
        // the user somewhere arbitrary.
        let mut h = Harness::new();
        h.app.settings.editor = crate::settings::EditorMode::External;
        open_editor_from_review(&mut h);

        assert!(h.app.overlay.is_none());
        assert_ne!(h.app.focus, Focus::PaneContent);
    }

    // --- getting out of a floating window -----------------------------------

    #[test]
    fn f12_closes_a_floating_window_from_anywhere() {
        // The leader depends on the terminal delivering Ctrl-Space, and a
        // floating pane swallows every other key on purpose. When both fail
        // there has to be something left to press.
        let mut h = Harness::new();
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.sent();

        h.key(KeyCode::F(12));

        assert!(h.app.overlay.is_none());
        assert!(
            !h.sent()
                .iter()
                .any(|m| matches!(m, ClientMsg::Input { .. })),
            "and it is never forwarded to the child"
        );
    }

    #[test]
    fn f12_is_harmless_when_no_window_is_open() {
        let mut h = Harness::new();
        h.key(KeyCode::F(12));
        assert!(!h.app.should_quit);
        assert!(h.app.overlay.is_none());
    }

    #[test]
    fn clicking_outside_a_floating_window_dismisses_it() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.app.layout.overlay = Panel {
            outer: Rect::new(10, 4, 20, 10),
            inner: Rect::new(11, 5, 18, 8),
            first: 0,
        };

        h.app.on_mouse(click(1, 1)); // out on the projects column

        assert!(h.app.overlay.is_none());
    }

    #[test]
    fn a_click_inside_the_window_belongs_to_its_pane() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.app.layout.overlay = Panel {
            outer: Rect::new(10, 4, 20, 10),
            inner: Rect::new(11, 5, 18, 8),
            first: 0,
        };
        wants_mouse(&mut h, PaneId(101));
        h.sent();

        h.app.on_mouse(click(15, 7));

        assert!(h.app.overlay.is_some(), "still open");
        assert!(h
            .sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Input { .. })));
    }

    #[test]
    fn a_click_under_a_floating_window_never_reaches_the_columns() {
        // The bug this exists for: focus moved to a column while the keys
        // still went to the overlay, leaving no way in and no way out.
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.app.layout.overlay = Panel {
            outer: Rect::new(10, 4, 20, 10),
            inner: Rect::new(11, 5, 18, 8),
            first: 0,
        };
        let before = h.app.sel_project;

        h.app.on_mouse(click(1, 3)); // a project row, underneath

        assert_eq!(
            h.app.sel_project, before,
            "the click dismissed, it did not select"
        );
    }

    #[test]
    fn a_window_whose_pane_exits_closes_itself() {
        // Otherwise it sits there showing a dead grid — the shape of a hung
        // editor, with no sign anything is wrong.
        let mut h = Harness::new();
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);

        h.app.on_server_msg(ServerMsg::PaneClosed {
            pane: PaneId(101),
            code: Some(0),
        });

        assert!(h.app.overlay.is_none());
    }

    #[test]
    fn another_panes_exit_leaves_the_window_alone() {
        let mut h = Harness::new();
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.app.on_server_msg(ServerMsg::PaneClosed {
            pane: PaneId(100),
            code: Some(0),
        });
        assert!(h.app.overlay.is_some());
    }

    #[test]
    fn a_window_whose_pane_vanishes_from_the_tree_closes_itself() {
        // Killed from another client, or reaped while we were not looking.
        let mut h = Harness::new();
        h.app
            .open_overlay_pane(PaneId(101), "vim".to_string(), false);

        let mut t = tree();
        t[0].repositories[0].checkouts[0]
            .panes
            .retain(|p| p.id != PaneId(101));
        h.app.on_server_msg(ServerMsg::Tree(t));

        assert!(h.app.overlay.is_none());
    }

    // --- choosing the editor ------------------------------------------------

    /// Moves the settings cursor onto `want`.
    fn settings_row(h: &mut Harness, want: crate::app::Setting) {
        h.app.open_settings();
        let target = crate::app::Setting::ALL
            .iter()
            .position(|s| *s == want)
            .unwrap();
        for _ in 0..target {
            h.key(KeyCode::Char('j'));
        }
    }

    #[test]
    fn the_editor_command_is_typed_rather_than_cycled() {
        let mut h = Harness::new();
        settings_row(&mut h, crate::app::Setting::EditorCmd);
        h.key(KeyCode::Char('l'));

        assert!(
            matches!(h.app.prompt, Some(Prompt::EditorCommand { .. })),
            "free text needs a field, not a carousel"
        );
    }

    #[test]
    fn typing_a_command_stores_it() {
        let mut h = Harness::new();
        settings_row(&mut h, crate::app::Setting::EditorCmd);
        h.key(KeyCode::Enter);
        h.keys("nvim -p");
        h.key(KeyCode::Enter);

        assert_eq!(h.app.settings.editor_cmd, "nvim -p");
        assert!(h.app.prompt.is_none());
    }

    #[test]
    fn the_prompt_starts_from_the_command_already_set() {
        // Retyping a long path to change one flag would be miserable.
        let mut h = Harness::new();
        h.app.settings.editor_cmd = "code -w".to_string();
        settings_row(&mut h, crate::app::Setting::EditorCmd);
        h.key(KeyCode::Enter);

        match &h.app.prompt {
            Some(Prompt::EditorCommand { input }) => assert_eq!(input, "code -w"),
            other => panic!("unexpected {other:?}", other = other.is_some()),
        }
    }

    #[test]
    fn clearing_the_command_goes_back_to_the_environment() {
        let mut h = Harness::new();
        h.app.settings.editor_cmd = "nvim".to_string();
        settings_row(&mut h, crate::app::Setting::EditorCmd);
        h.key(KeyCode::Enter);
        for _ in 0..4 {
            h.key(KeyCode::Backspace);
        }
        h.key(KeyCode::Enter);

        assert!(h.app.settings.editor_cmd.is_empty());
    }

    #[test]
    fn escaping_the_command_prompt_changes_nothing() {
        let mut h = Harness::new();
        h.app.settings.editor_cmd = "nvim".to_string();
        settings_row(&mut h, crate::app::Setting::EditorCmd);
        h.key(KeyCode::Enter);
        h.keys("zzz");
        h.key(KeyCode::Esc);

        assert_eq!(h.app.settings.editor_cmd, "nvim");
    }

    #[test]
    fn the_chosen_command_is_sent_with_the_request() {
        let mut h = Harness::new();
        h.app.settings.editor_cmd = "hx".to_string();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));
        h.key(KeyCode::Char('e'));

        match &h.sent()[0] {
            ClientMsg::OpenInEditor { command, .. } => {
                assert_eq!(command.as_deref(), Some("hx"))
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn no_command_leaves_the_choice_to_the_daemon() {
        // The daemon can see $VISUAL and what is installed; the client
        // guessing would only be worse.
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));
        h.key(KeyCode::Char('e'));

        match &h.sent()[0] {
            ClientMsg::OpenInEditor { command, .. } => assert_eq!(*command, None),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn opening_an_editor_does_not_move_the_pane_selection() {
        // The agent you were watching stays selected and stays on screen;
        // the editor is a window over the top, not a replacement for it.
        let mut h = Harness::new();
        h.keys("lll"); // sitting on the agent in the panes column
        h.sent();
        let watching = h.app.column_pane();
        let where_ = h.app.sel_pane;

        editor_arrives(&mut h);

        assert_eq!(h.app.sel_pane, where_, "selection untouched");
        assert_eq!(h.app.column_pane(), watching, "column still on the agent");
        assert_eq!(
            h.app.overlay_pane(),
            Some(PaneId(700)),
            "editor is the window"
        );
        assert!(
            h.app.grids.contains_key(&watching.unwrap()),
            "and the agent is still streaming"
        );
    }

    #[test]
    fn closing_the_editor_leaves_you_back_on_the_agent() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        let watching = h.app.column_pane();

        editor_arrives(&mut h);
        h.key(KeyCode::F(12));

        assert_eq!(h.app.column_pane(), watching);
        assert!(h.app.overlay.is_none());
    }

    // --- editors are not panes ----------------------------------------------

    /// A tree whose first checkout has a shell, an agent, and an editor.
    fn tree_with_editor() -> Vec<ProjectInfo> {
        let mut t = tree();
        t[0].repositories[0].checkouts[0].panes.push(PaneInfo {
            id: PaneId(700),
            kind: PaneKind::Editor,
            title: "a.rs".to_string(),
            status: PaneStatus::Idle,
            note: None,
            template: None,
            children: Vec::new(),
        });
        t
    }

    #[test]
    fn an_editor_is_not_listed_among_the_panes() {
        // It is a way of looking at a file, not something running here that
        // you would come back to.
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Tree(tree_with_editor()));

        let listed: Vec<PaneId> = h.app.tree[0].repositories[0].checkouts[0]
            .listed_panes()
            .map(|p| p.id)
            .collect();
        assert!(!listed.contains(&PaneId(700)), "{listed:?}");
        assert_eq!(listed.len(), 2, "the shell and the agent remain");
    }

    #[test]
    fn navigation_skips_over_editors() {
        // Otherwise j/k walks onto a row nothing draws.
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Tree(tree_with_editor()));
        h.keys("lll");
        for _ in 0..5 {
            h.key(KeyCode::Char('j'));
        }
        assert_eq!(h.app.sel_pane, 1, "clamped to the last listed pane");
        assert_ne!(h.app.column_pane(), Some(PaneId(700)));
    }

    #[test]
    fn an_editor_does_not_inflate_the_pane_counts() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Tree(tree_with_editor()));
        assert_eq!(
            h.app.tree[0].repositories[0].checkouts[0]
                .listed_panes()
                .count(),
            2
        );
    }

    #[test]
    fn closing_an_editors_window_ends_the_editor() {
        // Nothing lists it afterwards, so a survivor would be a process
        // with no window and no way back to it.
        let mut h = Harness::new();
        h.app
            .open_overlay_pane(PaneId(700), "a.rs".to_string(), true);
        h.sent();

        h.key(KeyCode::F(12));

        assert!(h
            .sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Kill { pane } if *pane == PaneId(700))));
    }

    #[test]
    fn closing_a_window_over_a_listed_pane_leaves_it_running() {
        // A shell or agent shown floating is still in the panes column, so
        // closing the window is only ever "stop looking at it".
        let mut h = Harness::new();
        h.app
            .open_overlay_pane(PaneId(101), "shell".to_string(), false);
        h.sent();

        h.key(KeyCode::F(12));

        assert!(!h.sent().iter().any(|m| matches!(m, ClientMsg::Kill { .. })));
    }

    #[test]
    fn a_diff_opens_in_a_window_and_leaves_the_column_alone() {
        // Reading a diff should not cost you sight of the agent that
        // produced it.
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        let watching = h.app.column_pane();

        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        h.app.review_for_test(checkout);
        h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));

        assert!(matches!(h.app.overlay, Some(Overlay::Review)));
        assert_eq!(h.app.column_pane(), watching, "column untouched");
        assert!(
            h.app.grids.contains_key(&watching.unwrap()),
            "still streaming"
        );
    }

    #[test]
    fn closing_a_diff_puts_you_back_on_the_checkout() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        h.app.review_for_test(checkout);
        h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));

        h.key(KeyCode::Esc);

        assert!(h.app.overlay.is_none());
        assert!(h.app.review.is_none());
        assert_eq!(h.app.focus, Focus::Checkouts);
    }

    #[test]
    fn f12_also_gets_you_out_of_a_diff() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        h.app.review_for_test(checkout);
        h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));

        h.key(KeyCode::F(12));

        assert!(h.app.overlay.is_none());
        assert!(h.app.review.is_none(), "and the diff goes with it");
    }

    // --- collapse projects pane ----------------------------------------------

    #[test]
    fn p_collapses_and_restores_the_projects_column() {
        let mut h = Harness::new();
        assert!(!h.app.projects_collapsed, "starts expanded");
        assert_eq!(h.app.focus, Focus::Projects);

        h.key(KeyCode::Char('p'));
        assert!(h.app.projects_collapsed, "p collapses");
        assert_eq!(h.app.focus, Focus::Repositories, "focus leaves the strip");
        assert!(
            h.app.status.contains("collapsed"),
            "reports collapse: {}",
            h.app.status
        );
        assert!(h.app.settings.projects_collapsed, "persisted to settings");

        h.key(KeyCode::Char('p'));
        assert!(!h.app.projects_collapsed, "p restores");
        assert_eq!(
            h.app.focus,
            Focus::Repositories,
            "focus stays put on restore"
        );
        assert!(
            h.app.status.contains("expanded"),
            "reports expand: {}",
            h.app.status
        );
        assert!(!h.app.settings.projects_collapsed, "cleared in settings");
    }

    #[test]
    fn collapsing_moves_focus_off_projects() {
        let mut h = Harness::new();
        h.app.focus = Focus::Projects;
        h.key(KeyCode::Char('p'));
        assert!(h.app.projects_collapsed);
        assert_eq!(h.app.focus, Focus::Repositories);
    }

    #[test]
    fn ascending_into_a_collapsed_projects_column_stays_put() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('p')); // collapse, focus -> Repositories
        h.key(KeyCode::Char('h')); // ascend from Repositories
        assert_eq!(
            h.app.focus,
            Focus::Repositories,
            "blocked by collapsed strip"
        );
        // Expand it; now ascend works.
        h.key(KeyCode::Char('p'));
        h.key(KeyCode::Char('h'));
        assert_eq!(h.app.focus, Focus::Projects);
    }

    #[test]
    fn starting_collapsed_lands_on_repositories() {
        let (tx, _rx) = unbounded_channel();
        let settings = crate::settings::Settings {
            projects_collapsed: true,
            ..crate::settings::Settings::default()
        };
        let app = App::build(tx, settings, false);
        assert!(app.projects_collapsed);
        assert_eq!(
            app.focus,
            Focus::Repositories,
            "never lands on the hidden column"
        );
    }

    #[test]
    fn clicking_the_collapsed_strip_expands_it() {
        let mut h = Harness::new();
        laid_out(&mut h); // set up a real layout
        h.key(KeyCode::Char('p')); // collapse
                                   // The laid_out projects column is 12 wide; clicking anywhere in it
                                   // expands because the layout hasn't been re-rendered yet.
        h.app.on_mouse(click(1, 1));
        assert!(!h.app.projects_collapsed, "click expands");
    }

    #[test]
    fn the_gutter_next_to_a_collapsed_strip_is_not_draggable() {
        let mut h = Harness::new();
        let panel = |x: u16, w: u16| Panel {
            outer: Rect::new(x, 0, w, 8),
            inner: Rect::new(x + 1, 1, w.saturating_sub(2), 6),
            first: 0,
        };
        // Strip at 0..2, gutter at 2, repositories at 3..15.
        h.app.layout = Layout {
            projects: panel(0, 2),
            repositories: panel(3, 12),
            checkouts: panel(16, 12),
            panes: panel(29, 12),
            content: panel(42, 20),
            overlay: Panel::default(),
            cursor: None,
        };
        h.app.projects_collapsed = true;

        h.app.on_mouse(click(2, 3)); // the gutter cell
        assert!(h.app.resizing_gutter.is_none(), "gutter suppressed");

        // Drag does nothing.
        h.app.on_mouse(drag(5, 3));
        h.app.on_mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 5,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(h.app.column_widths, None);
    }
}
