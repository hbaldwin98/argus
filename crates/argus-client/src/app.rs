use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use argus_protocol::{
    CheckoutId, CheckoutInfo, ClientMsg, PaneId, PaneInfo, ProjectInfo, RepositoryId,
    RepositoryInfo, ServerMsg, WorkspaceId, WorkspaceInfo,
};
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use crate::fuzzy::Fuzzy;
use crate::grid::Grid;
use crate::review::{Anchor, ReviewView};
use argus_protocol::ReviewBase;
use crate::theme::Theme;
use crate::keys::{encode_key, is_leader};
use crate::mouse::encode_mouse;

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
}

impl Overlay {
    fn pane(&self) -> Option<PaneId> {
        match self {
            Overlay::Pane { pane, .. } => Some(*pane),
            Overlay::Settings { .. } | Overlay::Review => None,
        }
    }
}

/// The rows of the settings panel, in the order they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Editor,
    EditorCmd,
    Theme,
}

impl Setting {
    pub const ALL: &'static [Setting] =
        &[Setting::Editor, Setting::EditorCmd, Setting::Theme];

    pub fn label(self) -> &'static str {
        match self {
            Setting::Editor => "editor opens",
            Setting::EditorCmd => "editor command",
            Setting::Theme => "theme",
        }
    }
}

/// A modal text/confirm prompt, mutually exclusive with `Picker`. Both new
/// worktree (free text) and remove-checkout (yes/no) go through this so
/// there's one input path and one place `on_mouse` has to know to ignore.
pub enum Prompt {
    NewWorktree { base: CheckoutId, input: String },
    ConfirmRemoveCheckout { checkout: CheckoutId, label: String },
    AddProject { input: String },
    Comment { anchor: Anchor, input: String },
    /// The editor command, typed rather than cycled — it is free text.
    EditorCommand { input: String },
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
    resizing_gutter: Option<usize>,
    pub picker: Option<Picker>,
    pub overlay: Option<Overlay>,
    pub settings: crate::settings::Settings,
    /// False for an app that must not write to the user's config — every
    /// test, and anything constructed with [`App::new`].
    persist_settings: bool,
    /// The next pane the daemon tells us about should open in an overlay.
    /// Set when an editor is spawned for one.
    pending_overlay_new: bool,
    pub review: Option<ReviewView>,
    /// What the outstanding request was for; a diff for anything else is
    /// stale and dropped.
    review_wanted: Option<(CheckoutId, u64)>,
    next_review_request: u64,
    /// Same, for a branch or file list.
    list_wanted: Option<CheckoutId>,
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
            focus: Focus::Projects,
            sel_project: 0,
            sel_repository: 0,
            sel_checkout: 0,
            sel_pane: 0,
            grids: std::collections::HashMap::new(),
            leader_pending: false,
            should_quit: false,
            // Empty, not a keymap: the bar's left half is the breadcrumb's
            // until something has actually happened to report.
            status: String::new(),
            status_alert: false,
            layout: Layout::default(),
            column_widths,
            resizing_gutter: None,
            picker: None,
            overlay: None,
            settings,
            persist_settings: persist,
            pending_overlay_new: false,
            review: None,
            review_wanted: None,
            next_review_request: 1,
            list_wanted: None,
            review_base: ReviewBase::WorkingTree,
            prompt: None,
            theme,
            pending_focus_new: false,
            pending_focus_new_checkout: None,
            pending_focus_new_project: false,
            out,
        }
    }

    pub fn current_project(&self) -> Option<&ProjectInfo> {
        self.tree.get(self.sel_project)
    }

    pub fn current_repository(&self) -> Option<&RepositoryInfo> {
        self.current_project()
            .and_then(|p| p.repositories.get(self.sel_repository))
    }

    pub fn current_checkout(&self) -> Option<&CheckoutInfo> {
        self.current_repository()
            .and_then(|r| r.checkouts.get(self.sel_checkout))
    }

    pub fn current_pane(&self) -> Option<&PaneInfo> {
        self.current_checkout()
            .and_then(|c| c.listed_panes().nth(self.sel_pane))
    }

    fn clamp(&mut self) {
        let nproj = self.tree.len();
        if nproj == 0 {
            self.sel_project = 0;
        } else if self.sel_project >= nproj {
            self.sel_project = nproj - 1;
        }
        let nrepo = self.current_project().map(|p| p.repositories.len()).unwrap_or(0);
        if nrepo == 0 {
            self.sel_repository = 0;
        } else if self.sel_repository >= nrepo {
            self.sel_repository = nrepo - 1;
        }
        let ncheck = self.current_repository().map(|r| r.checkouts.len()).unwrap_or(0);
        if ncheck == 0 {
            self.sel_checkout = 0;
        } else if self.sel_checkout >= ncheck {
            self.sel_checkout = ncheck - 1;
        }
        let npane = self.visible_pane_count();
        if npane == 0 {
            self.sel_pane = 0;
        } else if self.sel_pane >= npane {
            self.sel_pane = npane - 1;
        }
        self.sync_subscription();
    }

    /// Keeps the live view subscribed to whatever pane current navigation
    /// state implies, independent of `focus` — the rightmost column always
    /// shows this pane's content alongside the project/checkout columns,
    /// it never takes over the whole screen.
    fn visible_pane_count(&self) -> usize {
        self.current_checkout()
            .map(|c| c.listed_panes().count())
            .unwrap_or(0)
    }

    /// The pane the rightmost column draws: whatever the tree selection
    /// implies, untouched by anything floating above it.
    pub fn column_pane(&self) -> Option<PaneId> {
        self.current_pane().map(|p| p.id)
    }

    pub fn overlay_pane(&self) -> Option<PaneId> {
        self.overlay.as_ref().and_then(Overlay::pane)
    }

    /// Subscribes to everything currently on screen and drops the rest.
    fn sync_subscription(&mut self) {
        let want: Vec<PaneId> = [self.column_pane(), self.overlay_pane()]
            .into_iter()
            .flatten()
            .collect();

        let stale: Vec<PaneId> = self
            .grids
            .keys()
            .copied()
            .filter(|id| !want.contains(id))
            .collect();
        for id in stale {
            self.grids.remove(&id);
            let _ = self.out.send(ClientMsg::Unsubscribe { pane: id });
        }
        for id in want {
            // The entry doubles as the record that this pane has been
            // asked for; the snapshot replaces the placeholder.
            if let std::collections::hash_map::Entry::Vacant(slot) = self.grids.entry(id) {
                slot.insert(Grid::new(Vec::new()));
                let _ = self.out.send(ClientMsg::Subscribe { pane: id });
            }
        }
    }

    pub fn on_server_msg(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Tree(tree) => self.receive_tree(tree),
            ServerMsg::Templates(names) => {
                self.templates = names;
            }
            ServerMsg::Workspaces(list) => {
                self.open_workspace = list
                    .iter()
                    .find(|w| w.open)
                    .map(|w| w.name.clone())
                    .unwrap_or_default();
                self.workspaces = list;
            }
            ServerMsg::PaneSnapshot {
                pane, cells, cursor, ..
            } => {
                if self.grids.contains_key(&pane) {
                    self.grids.insert(pane, Grid::with_cursor(cells, cursor));
                }
            }
            ServerMsg::Damage { pane, spans, cursor } => {
                {
                    if let Some(grid) = self.grids.get_mut(&pane) {
                        grid.apply(&spans);
                        grid.move_cursor(cursor);
                    }
                }
            }
            ServerMsg::PaneClosed { pane, code } => {
                self.receive_pane_closed(pane, code);
            }
            ServerMsg::Review(review) => {
                self.receive_review(review);
            }
            ServerMsg::ReviewFailed {
                request_id,
                checkout,
                message,
            } => {
                self.receive_review_failure(request_id, checkout, message);
            }
            ServerMsg::ReviewAcknowledged {
                checkout,
                target_snapshot,
            } => {
                self.receive_review_acknowledgement(checkout, &target_snapshot, None);
            }
            ServerMsg::ReviewAcknowledgeFailed {
                checkout,
                target_snapshot,
                message,
            } => {
                self.receive_review_acknowledgement(checkout, &target_snapshot, Some(message));
            }
            ServerMsg::Branches { checkout, branches } => {
                if self.list_wanted != Some(checkout) {
                    return;
                }
                self.list_wanted = None;
                // The head of the list is the branch we are already on, and
                // switching to it is a no-op — it stays as a label only.
                let current = branches.first().cloned().unwrap_or_default();
                let rest: Vec<String> = branches.into_iter().skip(1).collect();
                self.picker = Some(Picker::new(
                    PickerKind::Branch { checkout },
                    "switch branch",
                    rest,
                    0,
                ));
                self.report(format!("on {current}"));
            }
            ServerMsg::Files { checkout, files } => {
                if self.list_wanted != Some(checkout) {
                    return;
                }
                self.list_wanted = None;
                if files.is_empty() {
                    self.report("no files here");
                    return;
                }
                self.picker = Some(Picker::new(
                    PickerKind::File { checkout },
                    "open file",
                    files,
                    0,
                ));
            }
            ServerMsg::Error { message } => {
                self.alert(format!("error: {message}"));
            }
        }
    }

    fn receive_tree(&mut self, tree: Vec<ProjectInfo>) {
        let selected_pane = matches!(self.focus, Focus::Panes | Focus::PaneContent)
            .then(|| self.current_pane().map(|pane| pane.id))
            .flatten();
        self.tree = tree;
        if let Some(selected_pane) = selected_pane {
            if let Some((project, repository, checkout, pane)) = self.tree.iter().enumerate().find_map(
                |(project_index, project)| {
                    project.repositories.iter().enumerate().find_map(|(repository_index, repository)| {
                        repository.checkouts.iter().enumerate().find_map(
                            |(checkout_index, checkout)| {
                                checkout
                                    .listed_panes()
                                    .position(|candidate| candidate.id == selected_pane)
                                    .map(|pane_index| {
                                        (project_index, repository_index, checkout_index, pane_index)
                                    })
                            },
                        )
                    })
                },
            ) {
                self.sel_project = project;
                self.sel_repository = repository;
                self.sel_checkout = checkout;
                self.sel_pane = pane;
            }
        }
        self.clamp();
        if self.pending_focus_new_project {
            self.pending_focus_new_project = false;
            let n = self.tree.len();
            if n > 0 {
                self.sel_project = n - 1;
                self.clamp();
            }
        }
        if let Some(repository_id) = self.pending_focus_new_checkout.take() {
            if let Some((project, repository)) = self.tree.iter().enumerate().find_map(
                |(project_index, project)| {
                    project.repositories.iter().enumerate().find_map(
                        |(repository_index, repository)| {
                            (repository.id == repository_id)
                                .then_some((project_index, repository_index))
                        },
                    )
                },
            ) {
                self.sel_project = project;
                self.sel_repository = repository;
                self.sel_checkout = self
                    .current_repository()
                    .map(|r| r.checkouts.len().saturating_sub(1))
                    .unwrap_or(0);
                self.clamp();
            }
        }
        // A pane killed from elsewhere leaves its window orphaned.
        if let Some(pane) = self.overlay.as_ref().and_then(Overlay::pane) {
            let alive = self
                .tree
                .iter()
                .flat_map(|p| p.repositories.iter())
                .flat_map(|r| r.checkouts.iter())
                .flat_map(|c| c.panes.iter())
                .any(|p| p.id == pane);
            if !alive {
                self.close_overlay();
            }
        }
        if self.pending_focus_new {
            self.pending_focus_new = false;
            let newest = self
                .current_checkout()
                .and_then(|c| c.panes.last())
                .map(|p| (p.id, p.title.clone()));
            if let Some((id, title)) = newest {
                if std::mem::take(&mut self.pending_overlay_new) {
                    // Deliberately leaves `sel_pane` alone: the columns keep
                    // showing whatever you were watching, and closing the
                    // window puts you back there rather than on the editor.
                    self.open_overlay_pane(id, title, true);
                } else {
                    self.sel_pane = self.visible_pane_count().saturating_sub(1);
                    self.sync_subscription();
                    self.focus = Focus::PaneContent;
                }
            }
        }
    }

    fn receive_pane_closed(&mut self, pane: PaneId, code: Option<i32>) {
        // Otherwise the window sits there showing a dead grid, which is
        // exactly what a hung editor looks like.
        if self.overlay_pane() == Some(pane) {
            self.close_overlay();
        }
        self.grids.remove(&pane);
        if self.column_pane() == Some(pane) {
            // Ranked the way the pane rows rank an exit (§8b): a clean one is
            // news — the column just emptied — while a failure or a kill is
            // the thing on the bar you have to read.
            match code {
                Some(0) => self.report("pane exited"),
                Some(c) => self.alert(format!("pane exited with code {c}")),
                None => self.alert("pane was killed"),
            }
        }
    }

    fn receive_review(&mut self, review: argus_protocol::Review) {
        if self.review_wanted != Some((review.checkout, review.request_id)) {
            return;
        }
        self.review_wanted = None;
        let files = review.files.len();
        let base = review.base;
        let view = ReviewView::new(review);
        if view.is_empty() && base != ReviewBase::SinceLastLooked {
            self.review = None;
            self.report(format!("no changes vs {}", base.label()));
            return;
        }
        self.review = Some(view);
        self.overlay = Some(Overlay::Review);
        self.focus = Focus::Review;
        self.report(format!("{files} changed vs {}", base.label()));
    }

    fn receive_review_failure(&mut self, request_id: u64, checkout: CheckoutId, message: String) {
        if self.review_wanted == Some((checkout, request_id)) {
            self.review_wanted = None;
            self.alert(format!("error: {message}"));
        }
    }

    fn receive_review_acknowledgement(
        &mut self,
        checkout: CheckoutId,
        target_snapshot: &str,
        error: Option<String>,
    ) {
        let Some(view) = self.review.as_mut().filter(|view| {
            view.review.checkout == checkout && view.review.target_snapshot == target_snapshot
        }) else {
            return;
        };
        if let Some(message) = error {
            self.alert(format!("error: {message}"));
        } else {
            view.review.baseline_snapshot = Some(target_snapshot.to_string());
            self.report("review baseline accepted");
        }
    }

    /// Ordinary news — what a keypress did, or what it could not do. Drawn
    /// in plain text, and it yields the bar to the keymap when both will not
    /// fit.
    pub fn report(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_alert = false;
    }

    /// Something the user must read: a daemon error, a pane that died. Drawn
    /// as an alarm, and it keeps the bar even when that costs the keymap.
    pub fn alert(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_alert = true;
    }

    fn clear_status(&mut self) {
        self.status.clear();
        self.status_alert = false;
    }

    /// Shuts any floating window, from anywhere, whatever has focus.
    ///
    /// The leader is the *nice* way out, but it depends on the terminal
    /// delivering Ctrl-Space, and a floating pane consumes every other key
    /// on purpose. When that combination fails there is nothing left to
    /// press, so this one is checked before any handler runs and is never
    /// forwarded to a child. F-keys are reliably delivered and no terminal
    /// editor binds F12 by default.
    fn is_panic_key(key: &KeyEvent) -> bool {
        key.code == KeyCode::F(12)
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        // The left of the bar is the breadcrumb's seat and a message only
        // borrows it. Pressing anything is the acknowledgement that hands it
        // back; without that, the last error or exit hides where you are for
        // the rest of the session. Cleared before dispatch, so a handler is
        // still free to set its own.
        self.clear_status();
        if Self::is_panic_key(&key) {
            if self.overlay.is_some() {
                self.close_overlay();
                self.report("closed the floating window");
            }
            return;
        }
        if self.prompt.is_some() {
            self.on_key_prompt(key);
        } else if self.picker.is_some() {
            self.on_key_picker(key);
        } else if self.overlay.is_some() {
            self.on_key_overlay(key);
        } else if self.focus == Focus::Review {
            self.on_key_review(key);
        } else if self.focus == Focus::PaneContent {
            self.on_key_pane_content(key);
        } else {
            self.on_key_nav(key);
        }
    }

    pub fn on_paste(&mut self, text: String) {
        self.clear_status();
        if let Some(prompt) = &mut self.prompt {
            let input = match prompt {
                Prompt::NewWorktree { input, .. }
                | Prompt::Comment { input, .. }
                | Prompt::EditorCommand { input }
                | Prompt::AddProject { input } => Some(input),
                Prompt::ConfirmRemoveCheckout { .. } => None,
            };
            if let Some(input) = input {
                input.extend(text.chars().filter(|c| !c.is_control()));
            }
            return;
        }
        if let Some(picker) = &mut self.picker {
            if picker.kind.is_fuzzy() {
                picker.query.extend(text.chars().filter(|c| !c.is_control()));
                picker.refilter();
            }
            return;
        }
        let pane = self
            .overlay
            .as_ref()
            .and_then(Overlay::pane)
            .or_else(|| (self.focus == Focus::PaneContent).then(|| self.column_pane()).flatten());
        if let Some(pane) = pane {
            let _ = self.out.send(ClientMsg::Paste { pane, text });
        }
    }

    fn on_key_prompt(&mut self, key: KeyEvent) {
        let Some(prompt) = &mut self.prompt else { return };
        match prompt {
            Prompt::NewWorktree { base, input } => match key.code {
                KeyCode::Enter => {
                    let branch = input.trim().to_string();
                    let base = *base;
                    self.prompt = None;
                    if !branch.is_empty() {
                        let _ = self.out.send(ClientMsg::CreateWorktree { checkout: base, branch });
                        self.pending_focus_new_checkout = self.current_repository().map(|r| r.id);
                    }
                }
                KeyCode::Esc => self.prompt = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            },
            Prompt::ConfirmRemoveCheckout { checkout, .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let _ = self.out.send(ClientMsg::RemoveCheckout { checkout: *checkout });
                    self.prompt = None;
                }
                KeyCode::Esc | KeyCode::Char('n') => self.prompt = None,
                _ => {}
            },
            Prompt::Comment { anchor, input } => match key.code {
                KeyCode::Enter => {
                    let message = anchor.message(input);
                    let empty = input.trim().is_empty();
                    self.prompt = None;
                    if !empty {
                        self.send_to_agent(message);
                    }
                }
                KeyCode::Esc => self.prompt = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            },
            Prompt::EditorCommand { input } => match key.code {
                KeyCode::Enter => {
                    let cmd = input.trim().to_string();
                    self.prompt = None;
                    self.settings.editor_cmd = cmd;
                    if self.persist_settings {
                        crate::settings::save(&self.settings);
                    }
                }
                KeyCode::Esc => self.prompt = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            },
            Prompt::AddProject { input } => match key.code {
                KeyCode::Enter => {
                    let path = input.trim().to_string();
                    self.prompt = None;
                    if !path.is_empty() {
                        let _ = self.out.send(ClientMsg::AddProject { path });
                        self.pending_focus_new_project = true;
                    }
                }
                KeyCode::Esc => self.prompt = None,
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => input.push(c),
                _ => {}
            },
        }
    }

    fn on_key_picker(&mut self, key: KeyEvent) {
        let fuzzy = self.picker.as_ref().is_some_and(|p| p.kind.is_fuzzy());
        // On a fuzzy picker every printable key is query text, so movement
        // moves to the arrows and ctrl-n/p. On a plain one j/k still work.
        match key.code {
            KeyCode::Down => self.move_picker(1),
            KeyCode::Up => self.move_picker(-1),
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_picker(1)
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_picker(-1)
            }
            KeyCode::Char('j') if !fuzzy => self.move_picker(1),
            KeyCode::Char('k') if !fuzzy => self.move_picker(-1),
            KeyCode::Enter => self.confirm_picker(),
            KeyCode::Esc => self.picker = None,
            KeyCode::Char('q') if !fuzzy => self.picker = None,
            KeyCode::Backspace if fuzzy => {
                if let Some(p) = &mut self.picker {
                    p.query.pop();
                    p.refilter();
                }
            }
            KeyCode::Char(c) if fuzzy => {
                if let Some(p) = &mut self.picker {
                    p.query.push(c);
                    p.refilter();
                }
            }
            _ => {}
        }
    }

    fn move_picker(&mut self, delta: isize) {
        let Some(p) = &mut self.picker else { return };
        let last = p.len().saturating_sub(1);
        p.sel = (p.sel as isize + delta).clamp(0, last as isize) as usize;
    }

    /// An overlay holding a pane is a typing surface like the content
    /// column, so the same leader gets you out of it.
    fn on_key_overlay(&mut self, key: KeyEvent) {
        if matches!(self.overlay, Some(Overlay::Review)) {
            self.on_key_review(key);
            return;
        }
        if let Some(Overlay::Settings { sel }) = &mut self.overlay {
            let sel = *sel;
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => self.move_setting(1),
                KeyCode::Char('k') | KeyCode::Up => self.move_setting(-1),
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => self.cycle_setting(sel, 1),
                KeyCode::Char('h') | KeyCode::Left => self.cycle_setting(sel, -1),
                KeyCode::Esc | KeyCode::Char('q') => self.close_overlay(),
                _ => {}
            }
            return;
        }
        if self.leader_pending {
            self.leader_pending = false;
            match key.code {
                KeyCode::Esc => self.close_overlay(),
                KeyCode::Char('x') => {
                    if let Some(pane) = self.overlay.as_ref().and_then(Overlay::pane) {
                        let _ = self.out.send(ClientMsg::Kill { pane });
                    }
                    self.close_overlay();
                }
                _ => {}
            }
            return;
        }
        if is_leader(&key) {
            self.leader_pending = true;
            return;
        }
        let Some(pane) = self.overlay.as_ref().and_then(Overlay::pane) else {
            return;
        };
        let bytes = encode_key(&key);
        if !bytes.is_empty() {
            let _ = self.out.send(ClientMsg::Input { pane, bytes });
        }
    }

    /// Puts a review up directly, for tests that must not disturb the
    /// column selection by navigating to it.
    #[cfg(test)]
    pub fn review_for_test(&mut self, checkout: CheckoutId) {
        self.review_wanted = Some((checkout, 1));
    }

    pub fn open_settings(&mut self) {
        self.overlay = Some(Overlay::Settings { sel: 0 });
        self.focus = Focus::Overlay;
    }

    fn move_setting(&mut self, delta: isize) {
        if let Some(Overlay::Settings { sel }) = &mut self.overlay {
            let last = Setting::ALL.len() as isize - 1;
            *sel = (*sel as isize + delta).clamp(0, last) as usize;
        }
    }

    /// Changing a setting applies it at once and writes it out — there is
    /// no separate save, so there is nothing to forget to press.
    fn cycle_setting(&mut self, sel: usize, delta: isize) {
        match Setting::ALL.get(sel) {
            Some(Setting::EditorCmd) => {
                self.prompt = Some(Prompt::EditorCommand {
                    input: self.settings.editor_cmd.clone(),
                });
                return;
            }
            Some(Setting::Editor) => {
                self.settings.editor = if delta > 0 {
                    self.settings.editor.next()
                } else {
                    self.settings.editor.prev()
                };
            }
            Some(Setting::Theme) => {
                let themes = crate::theme::THEMES;
                let here = themes
                    .iter()
                    .position(|t| *t == self.settings.theme)
                    .unwrap_or(0) as isize;
                let n = themes.len() as isize;
                let next = ((here + delta) % n + n) % n;
                self.settings.theme = themes[next as usize].to_string();
                self.theme = Theme::by_name(&self.settings.theme);
            }
            None => return,
        }
        if self.persist_settings {
            crate::settings::save(&self.settings);
        }
    }

    /// Opens `pane` in a floating window alongside whatever the columns
    /// are showing. `ephemeral` panes are killed when the window closes.
    pub fn open_overlay_pane(&mut self, pane: PaneId, title: String, ephemeral: bool) {
        self.overlay = Some(Overlay::Pane {
            pane,
            title,
            ephemeral,
        });
        self.focus = Focus::Overlay;
        self.sync_subscription();
    }

    pub fn close_overlay(&mut self) {
        // An editor is its window. Nothing lists it once the window is
        // gone, so leaving it running would strand the process.
        if let Some(Overlay::Pane {
            pane,
            ephemeral: true,
            ..
        }) = self.overlay
        {
            let _ = self.out.send(ClientMsg::Kill { pane });
        }
        self.review = None;
        self.review_wanted = None;
        self.overlay = None;
        self.leader_pending = false;
        self.focus = match self.focus {
            Focus::Overlay => Focus::Panes,
            // A diff was opened from a checkout, so that is where closing
            // it puts you back.
            Focus::Review => Focus::Checkouts,
            other => other,
        };
        self.sync_subscription();
    }

    fn on_key_pane_content(&mut self, key: KeyEvent) {
        if self.leader_pending {
            self.leader_pending = false;
            match key.code {
                KeyCode::Esc => self.ascend(),
                KeyCode::Tab => self.open_review(),
                KeyCode::Char('x') => self.close_current(),
                _ => {}
            }
            return;
        }
        if is_leader(&key) {
            self.leader_pending = true;
            return;
        }
        let Some(pane) = self.column_pane() else { return };
        let bytes = encode_key(&key);
        if !bytes.is_empty() {
            let _ = self.out.send(ClientMsg::Input { pane, bytes });
        }
    }

    fn on_key_nav(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('l') | KeyCode::Enter | KeyCode::Right => self.descend(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Esc => self.ascend(),
            KeyCode::Char('s') => self.spawn_shell(),
            KeyCode::Char('a') => self.open_picker(),
            KeyCode::Char('n') => self.new_prompt(),
            KeyCode::Char('D') => self.remove_checkout_prompt(),
            KeyCode::Char('w') => self.open_workspace_picker(),
            KeyCode::Char('t') => self.open_theme_picker(),
            KeyCode::Char('S') => self.open_settings(),
            KeyCode::Char('b') => self.open_branch_picker(),
            KeyCode::Char('f') => self.open_file_picker(),
            KeyCode::Char('R') | KeyCode::Tab => self.open_review(),
            KeyCode::Char('x') => self.kill_selected(),
            _ => {}
        }
    }

    fn open_theme_picker(&mut self) {
        let here = self.theme.name();
        self.picker = Some(Picker::new(
            PickerKind::Theme,
            "theme",
            crate::theme::THEMES.iter().map(|t| t.to_string()).collect(),
            crate::theme::THEMES.iter().position(|t| *t == here).unwrap_or(0),
        ));
    }

    /// Works from any column that still implies a checkout.
    fn open_review(&mut self) {
        let Some(id) = self.current_checkout().map(|c| c.id) else {
            self.report("nothing to review");
            return;
        };
        let request_id = self.next_review_request;
        self.next_review_request = self.next_review_request.wrapping_add(1).max(1);
        self.review_wanted = Some((id, request_id));
        self.report("loading diff…");
        let _ = self.out.send(ClientMsg::Review {
            request_id,
            checkout: id,
            base: self.review_base,
        });
    }

    /// Typed at the agent as if by hand, so it works with any harness
    /// rather than needing one to know about Argus.
    fn send_to_agent(&mut self, message: String) {
        let Some(pane) = self.agent_in_current_checkout() else {
            self.report("no agent running in this checkout");
            return;
        };
        let mut bytes = message.into_bytes();
        // What a terminal actually sends for Enter.
        bytes.push(b'\r');
        let _ = self.out.send(ClientMsg::Input { pane, bytes });
        self.report("comment sent");
    }

    /// Shells are skipped — a comment at a shell prompt is a failed command.
    fn agent_in_current_checkout(&self) -> Option<PaneId> {
        let checkout = self.review.as_ref().map(|v| v.review.checkout)?;
        self.tree
            .iter()
            .flat_map(|p| p.repositories.iter())
            .flat_map(|r| r.checkouts.iter())
            .find(|c| c.id == checkout)?
            .panes
            .iter()
            .find(|p| p.kind == argus_protocol::PaneKind::Agent)
            .map(|p| p.id)
    }

    /// The configured editor command, or `None` to leave it to the daemon.
    fn editor_command(&self) -> Option<String> {
        let cmd = self.settings.editor_cmd.trim();
        (!cmd.is_empty()).then(|| cmd.to_string())
    }

    /// Where the editor about to be spawned should land. Nothing at all
    /// for an external one: it has no pane to focus.
    fn want_editor(&mut self) {
        match self.settings.editor {
            crate::settings::EditorMode::External => {}
            crate::settings::EditorMode::Column => self.pending_focus_new = true,
            crate::settings::EditorMode::Overlay => {
                self.pending_focus_new = true;
                self.pending_overlay_new = true;
            }
        }
    }

    fn close_review(&mut self) {
        self.review = None;
        self.review_wanted = None;
        self.overlay = None;
        self.focus = Focus::Checkouts;
    }

    fn on_key_review(&mut self, key: KeyEvent) {
        // Taken first so they don't sit inside the view borrow.
        match key.code {
            KeyCode::Char('R') | KeyCode::Char('r') => return self.open_review(),
            KeyCode::Char('b') => {
                self.review_base = self.review_base.next();
                return self.open_review();
            }
            KeyCode::Char('A') => {
                if let Some(view) = &self.review {
                    let _ = self.out.send(ClientMsg::AcknowledgeReview {
                        checkout: view.review.checkout,
                        target_snapshot: view.review.target_snapshot.clone(),
                        expected_baseline: view.review.baseline_snapshot.clone(),
                    });
                    self.report("accepting review baseline…");
                }
                return;
            }
            KeyCode::Char('f') => return self.open_change_picker(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Esc | KeyCode::Char('q') => {
                return self.close_review()
            }
            _ => {}
        }
        let Some(v) = &mut self.review else {
            // Focus without a view would trap every keystroke; the only
            // honest thing is to leave.
            self.focus = Focus::Checkouts;
            return;
        };
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => v.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => v.move_by(-1),
            KeyCode::Char('d') | KeyCode::PageDown => v.move_by(10),
            KeyCode::Char('u') | KeyCode::PageUp => v.move_by(-10),
            KeyCode::Char(']') => v.jump_file(true),
            KeyCode::Char('[') => v.jump_file(false),
            KeyCode::Char('g') | KeyCode::Home => v.top_of_diff(),
            KeyCode::Char('G') | KeyCode::End => v.bottom_of_diff(),
            KeyCode::Char('V') | KeyCode::Char('v') => v.toggle_mark(),
            KeyCode::Char('e') => {
                let checkout = v.review.checkout;
                if let Some(a) = v.anchor() {
                    let _ = self.out.send(ClientMsg::OpenInEditor {
                        checkout,
                        path: a.path,
                        line: a.start,
                        external: self.settings.editor.is_external(),
                        command: self.editor_command(),
                    });
                    self.want_editor();
                    self.close_review();
                }
            }
            KeyCode::Char('c') => {
                let anchor = v.anchor();
                if let Some(anchor) = anchor {
                    self.prompt = Some(Prompt::Comment {
                        anchor,
                        input: String::new(),
                    });
                }
            }
            // The tree has likely moved on under an agent still editing it.
            _ => {}
        }
    }

    /// `n` is contextual on which column has focus: a new project (any
    /// directory, not just preconfigured ones) from the projects column, or
    /// a new worktree branched off the selected checkout from the
    /// checkouts column. No-op elsewhere — there's no "current checkout"
    /// to branch from once you're inside the panes/content columns.
    fn new_prompt(&mut self) {
        match self.focus {
            Focus::Projects => {
                self.prompt = Some(Prompt::AddProject { input: String::new() });
            }
            Focus::Checkouts => {
                if let Some(c) = self.current_checkout() {
                    self.prompt = Some(Prompt::NewWorktree {
                        base: c.id,
                        input: String::new(),
                    });
                }
            }
            _ => {}
        }
    }

    /// Opens a confirmation to remove the selected checkout. Refused
    /// client-side for the primary checkout — the repo the user already
    /// had, not Argus's to delete — so there's no round-trip just to be
    /// told no (the daemon refuses it too, as defense in depth).
    fn remove_checkout_prompt(&mut self) {
        if self.focus != Focus::Checkouts {
            return;
        }
        let Some(c) = self.current_checkout() else { return };
        if c.primary {
            self.report("can't remove the primary checkout");
            return;
        }
        self.prompt = Some(Prompt::ConfirmRemoveCheckout {
            checkout: c.id,
            label: c.name.clone(),
        });
    }

    pub fn on_mouse(&mut self, ev: MouseEvent) {
        if self.picker.is_some() || self.prompt.is_some() {
            return;
        }
        // Same acknowledgement as a keypress, but only for a deliberate one:
        // a mouse crossing the terminal is not the user reading anything.
        if matches!(ev.kind, MouseEventKind::Down(_)) {
            self.clear_status();
        }
        // A floating window is modal: clicks inside it are its own, and a
        // click outside dismisses it. Without this a click would fall
        // through to the columns underneath, moving focus while the keys
        // still went to the overlay — no way in and no way out.
        if self.overlay.is_some() {
            let inside = in_rect(self.layout.overlay.outer, ev.column, ev.row);
            if !inside {
                if matches!(ev.kind, MouseEventKind::Down(_)) {
                    self.close_overlay();
                }
                return;
            }
            if let Some(bytes) = encode_mouse(&ev, self.layout.overlay.inner) {
                if let Some(pane) = self.overlay_pane() {
                    let _ = self.out.send(ClientMsg::Input { pane, bytes });
                }
            }
            return;
        }
        if matches!(ev.kind, MouseEventKind::Up(MouseButton::Left))
            && self.resizing_gutter.take().is_some()
        {
            self.settings.column_widths = self.column_widths.clone();
            if self.persist_settings {
                crate::settings::save(&self.settings);
            }
            return;
        }
        if let MouseEventKind::Drag(MouseButton::Left) = ev.kind {
            if let Some(gutter) = self.resizing_gutter {
                self.resize_columns_at(gutter, ev.column);
                return;
            }
        }
        if let MouseEventKind::Down(MouseButton::Left) = ev.kind {
            if let Some(gutter) = self.gutter_at(ev.column, ev.row) {
                self.column_widths = Some(self.rendered_column_widths());
                self.resizing_gutter = Some(gutter);
                return;
            }
        }
        // The live view is always visible in the rightmost column, so a
        // click landing on it both forwards to the child and (for presses)
        // switches into typing mode, regardless of what was focused before.
        if let Some(bytes) = encode_mouse(&ev, self.layout.content.inner) {
            if matches!(ev.kind, MouseEventKind::Down(_)) {
                self.focus = Focus::PaneContent;
            }
            if let Some(pane) = self.column_pane() {
                let _ = self.out.send(ClientMsg::Input { pane, bytes });
            }
            return;
        }
        // Anything outside the live view always navigates, even while
        // "inside" a pane for typing — a click on another column should
        // switch to it, not get swallowed by the pane that currently has
        // keyboard focus.
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => self.click_nav(ev.column, ev.row),
            MouseEventKind::ScrollUp => self.scroll_at(ev.column, ev.row, -1),
            MouseEventKind::ScrollDown => self.scroll_at(ev.column, ev.row, 1),
            _ => {}
        }
    }

    fn panels(&self) -> [Panel; 5] {
        [
            self.layout.projects,
            self.layout.repositories,
            self.layout.checkouts,
            self.layout.panes,
            self.layout.content,
        ]
    }

    fn rendered_column_widths(&self) -> Vec<u16> {
        self.panels().iter().map(|panel| panel.outer.width).collect()
    }

    /// Returns the blank separator under the pointer. Separators remain one
    /// cell wide, which gives dragging an unambiguous target without taking
    /// clicks away from either panel's border.
    fn gutter_at(&self, x: u16, y: u16) -> Option<usize> {
        let panels = self.panels();
        panels.windows(2).position(|pair| {
            let left = pair[0].outer;
            let right = pair[1].outer;
            let left_edge = left.x.saturating_add(left.width);
            x >= left_edge
                && x < right.x
                && y >= left.y.max(right.y)
                && y
                    < left
                        .y
                        .saturating_add(left.height)
                        .min(right.y.saturating_add(right.height))
        })
    }

    fn resize_columns_at(&mut self, gutter: usize, x: u16) {
        let panels = self.panels();
        let left = panels[gutter].outer;
        let right = panels[gutter + 1].outer;
        let pair_width = left.width.saturating_add(right.width);
        if pair_width < 2 {
            return;
        }

        // On very small terminals the effective floor scales down, but a
        // column always retains at least one cell instead of disappearing.
        let floor = crate::ui::MIN_COLUMN_WIDTH.min(pair_width / 2).max(1);
        let left_width = x
            .saturating_sub(left.x)
            .clamp(floor, pair_width.saturating_sub(floor));
        let rendered = self.rendered_column_widths();
        let widths = self.column_widths.get_or_insert(rendered);
        widths[gutter] = left_width;
        widths[gutter + 1] = pair_width - left_width;
    }

    /// Scroll-wheel selection change for whichever list column the cursor is
    /// over, independent of `focus` — so scrolling a background column
    /// doesn't steal focus away from a pane you're typing into.
    fn scroll_at(&mut self, x: u16, y: u16, delta: i32) {
        let Some((target, _)) = self.column_at(x, y) else {
            return;
        };
        self.adjust_selection(target, delta);
    }

    /// Which list column a point falls in, anywhere on its card, and that
    /// card's row area.
    fn column_at(&self, x: u16, y: u16) -> Option<(Focus, Rect)> {
        for (focus, panel) in [
            (Focus::Projects, self.layout.projects),
            (Focus::Repositories, self.layout.repositories),
            (Focus::Checkouts, self.layout.checkouts),
            (Focus::Panes, self.layout.panes),
        ] {
            if in_rect(panel.outer, x, y) {
                return Some((focus, panel.inner));
            }
        }
        None
    }

    /// A click on a card moves focus to it and leaves the selection alone;
    /// a click that lands on a row selects that row as well. Clicking the
    /// already-selected row a second time descends, the way `l` would.
    fn click_nav(&mut self, x: u16, y: u16) {
        // The content column has no rows to hit — clicking its frame just
        // puts keyboard focus back on whatever it is showing.
        if in_rect(self.layout.content.outer, x, y) {
            if self.review.is_some() {
                self.focus = Focus::Review;
            } else if self.current_pane().is_some() {
                self.focus = Focus::PaneContent;
            }
            // With nothing running there, focus would be a mode with no
            // keys and no way out but the leader.
            return;
        }

        let Some((target, inner)) = self.column_at(x, y) else {
            return;
        };
        let count = match target {
            Focus::Projects => self.tree.len(),
            Focus::Repositories => self.current_project().map(|p| p.repositories.len()).unwrap_or(0),
            Focus::Checkouts => self.current_repository().map(|r| r.checkouts.len()).unwrap_or(0),
            _ => self.visible_pane_count(),
        };

        let hit = row_in(inner, x, y).filter(|idx| *idx < count);
        let already = self.focus == target && hit == Some(self.selection_in(target));
        if let Some(idx) = hit {
            *self.selection_mut(target) = idx;
        }
        self.focus = target;
        self.clamp();
        if already {
            self.descend();
        }
    }

    fn selection_in(&self, target: Focus) -> usize {
        match target {
            Focus::Projects => self.sel_project,
            Focus::Repositories => self.sel_repository,
            Focus::Checkouts => self.sel_checkout,
            _ => self.sel_pane,
        }
    }

    fn selection_mut(&mut self, target: Focus) -> &mut usize {
        match target {
            Focus::Projects => &mut self.sel_project,
            Focus::Repositories => &mut self.sel_repository,
            Focus::Checkouts => &mut self.sel_checkout,
            _ => &mut self.sel_pane,
        }
    }

    fn move_selection(&mut self, delta: i32) {
        self.adjust_selection(self.focus, delta);
    }

    fn adjust_selection(&mut self, target: Focus, delta: i32) {
        let sel = match target {
            Focus::Projects => &mut self.sel_project,
            Focus::Repositories => &mut self.sel_repository,
            Focus::Checkouts => &mut self.sel_checkout,
            Focus::Panes => &mut self.sel_pane,
            Focus::PaneContent | Focus::Review | Focus::Overlay => return,
        };
        let new = *sel as i32 + delta;
        if new >= 0 {
            *sel = new as usize;
        }
        self.clamp();
    }

    fn descend(&mut self) {
        match self.focus {
            Focus::Projects => {
                if self.current_project().is_some() {
                    self.sel_repository = 0;
                    self.focus = Focus::Repositories;
                }
            }
            Focus::Repositories => {
                if self.current_repository().is_some() {
                    self.sel_checkout = 0;
                    self.focus = Focus::Checkouts;
                }
            }
            Focus::Checkouts => {
                if self.current_checkout().is_some() {
                    self.sel_pane = 0;
                    self.focus = Focus::Panes;
                }
            }
            Focus::Panes => {
                if self.current_pane().is_some() {
                    self.focus = Focus::PaneContent;
                }
            }
            Focus::PaneContent | Focus::Review | Focus::Overlay => {}
        }
    }

    fn ascend(&mut self) {
        match self.focus {
            Focus::PaneContent => {
                // Deliberately does not unsubscribe: the live view keeps
                // showing this pane in the rightmost column while browsing.
                self.leader_pending = false;
                self.focus = Focus::Panes;
            }
            Focus::Panes => self.focus = Focus::Checkouts,
            Focus::Checkouts => self.focus = Focus::Repositories,
            Focus::Repositories => self.focus = Focus::Projects,
            Focus::Projects => {}
            Focus::Review | Focus::Overlay => self.focus = Focus::Checkouts,
        }
    }

    fn spawn_shell(&mut self) {
        if let Some(checkout) = self.current_checkout() {
            let _ = self.out.send(ClientMsg::SpawnShell { checkout: checkout.id });
            self.pending_focus_new = true;
        }
    }

    fn open_picker(&mut self) {
        if self.templates.is_empty() || self.current_checkout().is_none() {
            return;
        }
        self.picker = Some(Picker::new(
            PickerKind::Agent,
            "spawn agent",
            self.templates.clone(),
            0,
        ));
    }

    /// `w` switches workspace — the scope of the whole project column, and
    /// daemon-global, so every attached client follows. Opens on the one
    /// that's already open rather than the top of the list, since "look at
    /// where I am, then move" is the usual reason to press it.
    ///
    /// It opens on a single workspace too, because typing a name here is
    /// how a second one comes to exist: the alternative was hand-editing
    /// `projects.toml`, which meant the zero-config install stayed at one
    /// workspace forever.
    fn open_workspace_picker(&mut self) {
        if self.workspaces.is_empty() {
            // Only before the daemon's first message; there is always at
            // least `default`.
            self.report("no workspaces yet");
            return;
        }
        let sel = self.workspaces.iter().position(|w| w.open).unwrap_or(0);
        let items = self
            .workspaces
            .iter()
            .map(|w| {
                let panes = if w.panes > 0 {
                    format!("  {}▣", w.panes)
                } else {
                    String::new()
                };
                format!("{}  {}⑂{}", w.name, w.projects, panes)
            })
            .collect();
        self.picker = Some(Picker::new(
            PickerKind::Workspace {
                ids: self.workspaces.iter().map(|w| w.id).collect(),
                names: self.workspaces.iter().map(|w| w.name.clone()).collect(),
            },
            "open workspace",
            items,
            sel,
        ));
    }

    /// Back to the top of the tree. Anything that swaps the whole project
    /// column out from under the columns needs it: the old indices refer
    /// to rows that are no longer there.
    fn reset_navigation(&mut self) {
        self.sel_project = 0;
        self.sel_repository = 0;
        self.sel_checkout = 0;
        self.sel_pane = 0;
        self.focus = Focus::Projects;
    }

    /// `b` asks the daemon for this checkout's branches; the picker opens
    /// when they arrive, so it never shows a stale list.
    fn open_branch_picker(&mut self) {
        let Some(id) = self.current_checkout().map(|c| c.id) else {
            self.report("no checkout selected");
            return;
        };
        self.list_wanted = Some(id);
        let _ = self.out.send(ClientMsg::ListBranches { checkout: id });
    }

    fn open_file_picker(&mut self) {
        let Some(id) = self.current_checkout().map(|c| c.id) else {
            self.report("no checkout selected");
            return;
        };
        self.list_wanted = Some(id);
        let _ = self.out.send(ClientMsg::ListFiles { checkout: id });
    }

    /// The changed files of the review that is already open — no round
    /// trip, since the diff is in hand.
    fn open_change_picker(&mut self) {
        let Some(view) = &self.review else { return };
        let items: Vec<String> = view
            .review
            .files
            .iter()
            .map(|f| format!("{} {}", f.kind.marker(), f.path))
            .collect();
        if items.is_empty() {
            return;
        }
        self.picker = Some(Picker::new(PickerKind::Change, "jump to change", items, 0));
    }

    fn confirm_picker(&mut self) {
        let Some(picker) = self.picker.take() else { return };
        // A create row carries the typed name rather than a list entry, so
        // it is handled before anything reads the selection.
        if let (Some(name), true) = (picker.create.clone(), picker.on_create_row()) {
            match &picker.kind {
                PickerKind::Branch { checkout } => {
                    let _ = self.out.send(ClientMsg::CreateBranch {
                        checkout: *checkout,
                        branch: name,
                    });
                }
                PickerKind::Workspace { .. } => {
                    let _ = self.out.send(ClientMsg::CreateWorkspace { name });
                    // The daemon opens what it creates, and it arrives
                    // empty, so the columns must not keep pointing into
                    // the workspace that was open a moment ago.
                    self.reset_navigation();
                }
                _ => {}
            }
            return;
        }
        match &picker.kind {
            PickerKind::Branch { checkout } => {
                let Some(branch) = picker.selected() else { return };
                let _ = self.out.send(ClientMsg::SwitchBranch {
                    checkout: *checkout,
                    branch: branch.to_string(),
                });
            }
            PickerKind::File { checkout } => {
                let Some(path) = picker.selected() else { return };
                let _ = self.out.send(ClientMsg::OpenInEditor {
                    checkout: *checkout,
                    path: path.to_string(),
                    line: None,
                    external: self.settings.editor.is_external(),
                    command: self.editor_command(),
                });
                self.want_editor();
            }
            PickerKind::Change => {
                let Some(idx) = picker.shown.get(picker.sel).copied() else { return };
                if let Some(view) = &mut self.review {
                    view.jump_to_file(idx);
                }
            }
            PickerKind::Agent => {
                let Some(name) = picker.selected() else { return };
                if let Some(checkout) = self.current_checkout() {
                    let _ = self.out.send(ClientMsg::SpawnAgent {
                        checkout: checkout.id,
                        template: name.to_string(),
                    });
                    self.pending_focus_new = true;
                }
            }
            PickerKind::Workspace { ids, .. } => {
                let Some(id) = picker.shown.get(picker.sel).and_then(|i| ids.get(*i)) else {
                    return;
                };
                let _ = self.out.send(ClientMsg::OpenWorkspace { workspace: *id });
                // The incoming tree is a different set of projects, so
                // start at the top rather than keeping an index that meant
                // something else.
                self.reset_navigation();
            }
            PickerKind::Theme => {
                let Some(name) = picker.selected() else { return };
                self.theme = crate::theme::Theme::by_name(name);
                self.report(format!("theme: {name}"));
            }
        }
    }

    fn kill_selected(&mut self) {
        if self.focus == Focus::Panes {
            self.close_current();
        }
    }

    /// Closes whatever pane is currently shown in the live view — reachable
    /// both from the open-agents list (`x`) and, via the leader chord, from
    /// inside the pane itself (`<leader>x`), since a bare `x` there is just
    /// a character typed at the child.
    fn close_current(&mut self) {
        if let Some(pane) = self.current_pane() {
            let _ = self.out.send(ClientMsg::Kill { pane: pane.id });
            // Land back in the open-agents list rather than staying "in"
            // PaneContent — the pane at this index may now be a different
            // one once the removal lands, and typing should never go to a
            // pane the user didn't choose.
            self.focus = Focus::Panes;
        }
    }

    pub fn resize_pane(&mut self, pane: PaneId, rows: u16, cols: u16) {
        let _ = self.out.send(ClientMsg::Resize { pane, rows, cols });
    }

    /// Every pane on screen with the area it is drawn in. Each pty is sized
    /// from its own, so a floating editor and the column behind it do not
    /// have to agree on a width.
    pub fn live_panes(&self) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        if let Some(id) = self.column_pane() {
            out.push((id, self.layout.content.inner));
        }
        if let Some(id) = self.overlay_pane() {
            out.push((id, self.layout.overlay.inner));
        }
        out
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
    use crossterm::event::KeyModifiers;
    use argus_protocol::{
        Cell, CellSpan, CheckoutId, GitStatus, PaneKind, PaneStatus, ProjectId, RepositoryId,
        RepositoryInfo,
    };
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    fn pane(id: u64, title: &str) -> PaneInfo {
        PaneInfo {
            id: PaneId(id),
            kind: PaneKind::Shell,
            title: title.to_string(),
            status: PaneStatus::Idle,
            note: None,
            template: None,
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
        }
    }

    /// Two projects; the first has a primary checkout with two panes and a
    /// linked worktree with none, the second has a single empty checkout.
    fn tree() -> Vec<ProjectInfo> {
        vec![
            ProjectInfo {
                id: ProjectId(1),
                name: "argus".to_string(),
                repositories: vec![repository(5, "orion", vec![
                    checkout(10, "master", true, vec![pane(100, "shell"), pane(101, "claude")]),
                    checkout(11, "feat", false, vec![]),
                ])],
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
        for expected in [Focus::Repositories, Focus::Checkouts, Focus::Panes, Focus::PaneContent] {
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

    // --- live-view subscription -------------------------------------------

    #[test]
    fn the_live_view_subscribes_to_the_selected_pane_without_descending() {
        // The rightmost column always shows a pane; it never has to take
        // over the screen for content to be visible.
        let mut h = Harness::new();
        assert_eq!(h.app.column_pane(), Some(PaneId(100)), "first pane, from Projects focus");
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
        assert!(matches!(msgs[1], ClientMsg::Subscribe { pane: PaneId(101) }), "{msgs:?}");
        assert_eq!(h.app.column_pane(), Some(PaneId(101)));
        assert!(!h.app.grids.contains_key(&PaneId(100)), "the old grid is dropped");
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
        assert_eq!(h.app.column_pane(), Some(PaneId(100)), "live view keeps showing it");
        assert!(h.sent().is_empty(), "no resubscribe churn");
    }

    #[test]
    fn damage_for_an_unsubscribed_pane_is_ignored() {
        let mut h = Harness::new();
        h.app.grids
            .insert(PaneId(100), crate::grid::Grid::new(vec![vec![Cell::default()]]));
        h.app.on_server_msg(ServerMsg::Damage {
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
            pane: PaneId(100),
            rows: 1,
            cols: 1,
            cells: vec![vec![Cell::default()]],
            cursor: argus_protocol::Cursor {
                row: 0,
                col: 0,
                visible: true,
            },
        });
        assert!(h.app.grids.contains_key(&PaneId(100)));
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

        h.key(KeyCode::Esc);
        assert_eq!(h.app.focus, Focus::Panes);
        assert!(!h.app.leader_pending);
        assert!(h.sent().is_empty());
    }

    #[test]
    fn leader_then_x_closes_the_pane() {
        let mut h = Harness::new();
        h.keys("llll");
        h.sent();
        h.leader();
        h.key(KeyCode::Char('x'));
        assert!(matches!(h.sent()[0], ClientMsg::Kill { pane: PaneId(100) }));
        assert_eq!(h.app.focus, Focus::Panes, "land back in the list, not on another pane");
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

    // --- spawning ----------------------------------------------------------

    #[test]
    fn s_spawns_a_shell_in_the_selected_checkout_and_focuses_it() {
        let mut h = Harness::new();
        h.keys("llj"); // the linked worktree, which has no panes
        h.sent();
        h.key(KeyCode::Char('s'));
        assert!(
            matches!(h.sent()[0], ClientMsg::SpawnShell { checkout: CheckoutId(11) }),
            "spawns into the selected checkout"
        );

        // The daemon's next tree carries the new pane.
        let mut t = tree();
        t[0].repositories[0].checkouts[1].panes.push(pane(102, "shell"));
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.sel_pane, 0);
        assert_eq!(h.app.focus, Focus::PaneContent, "drops you straight into it");
        assert_eq!(h.app.column_pane(), Some(PaneId(102)));
    }

    #[test]
    fn a_spawn_focuses_the_newest_pane_not_the_first() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('s'));
        h.sent();
        let mut t = tree();
        t[0].repositories[0].checkouts[0].panes.push(pane(102, "shell"));
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
        assert_eq!(h.app.focus, Focus::Projects, "column focus must not move behind the modal");
    }

    // --- prompts -----------------------------------------------------------

    #[test]
    fn n_in_the_projects_column_prompts_for_a_directory() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        assert!(matches!(h.app.prompt, Some(Prompt::AddProject { .. })));

        h.keys("/some/dir");
        h.key(KeyCode::Enter);
        match &h.sent()[0] {
            ClientMsg::AddProject { path } => assert_eq!(path, "/some/dir"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(h.app.prompt.is_none());
    }

    #[test]
    fn a_new_project_becomes_the_selected_one() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.keys("/d");
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
                assert_eq!(*checkout, CheckoutId(10), "branched off the selected checkout");
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
        assert!(
            h.sent().is_empty(),
            "cancelling must not create a worktree"
        );
    }

    #[test]
    fn a_new_worktree_becomes_the_selected_checkout() {
        let mut h = Harness::new();
        h.keys("lln");
        h.keys("x");
        h.key(KeyCode::Enter);
        h.sent();

        let mut t = tree();
        t[0].repositories[0].checkouts.push(checkout(12, "x", false, vec![]));
        h.app.on_server_msg(ServerMsg::Tree(t));
        assert_eq!(h.app.current_checkout().unwrap().name, "x");
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
        t[0].repositories[0].checkouts.push(checkout(12, "x", false, vec![]));
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
            [ClientMsg::SpawnShell { checkout: CheckoutId(10) }]
        ));
    }

    #[test]
    fn n_does_nothing_in_the_pane_columns() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        h.key(KeyCode::Char('n'));
        assert!(h.app.prompt.is_none(), "no checkout context to branch from");
    }

    #[test]
    fn an_empty_prompt_sends_nothing() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.keys("   ");
        h.key(KeyCode::Enter);
        assert!(h.app.prompt.is_none());
        assert!(h.sent().is_empty(), "whitespace is not a path");
    }

    #[test]
    fn esc_cancels_a_prompt_and_backspace_edits_it() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.keys("abc");
        h.key(KeyCode::Backspace);
        match &h.app.prompt {
            Some(Prompt::AddProject { input }) => assert_eq!(input, "ab"),
            _ => panic!("expected the add-project prompt to still be open"),
        }
        h.key(KeyCode::Esc);
        assert!(h.app.prompt.is_none());
        assert!(h.sent().is_empty());
    }

    #[test]
    fn a_prompt_swallows_navigation_keys() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('n'));
        h.keys("jl");
        assert_eq!(h.app.sel_project, 0, "j typed into the prompt, not a move");
        assert_eq!(h.app.focus, Focus::Projects);
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
            Some(Prompt::ConfirmRemoveCheckout { checkout, label }) => {
                assert_eq!(*checkout, CheckoutId(11));
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
    fn d_only_applies_to_the_checkouts_column() {
        let mut h = Harness::new();
        h.key(KeyCode::Char('D'));
        assert!(h.app.prompt.is_none(), "not from the projects column");
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
        assert!(h.app.status_alert, "a failed exit is the thing you have to read");
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
        assert!(!h.app.status.is_empty(), "drifting across the terminal is not reading it");

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
        };
        h.app.layout = Layout {
            projects: panel(0, 12),
            repositories: panel(12, 12),
            checkouts: panel(24, 12),
            panes: panel(36, 12),
            content: panel(48, 20),
            overlay: Panel::default(),
        };
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
        };
        h.app.layout = Layout {
            projects: panel(0, 12),
            repositories: panel(13, 12),
            checkouts: panel(26, 12),
            panes: panel(39, 12),
            content: panel(52, 20),
            overlay: Panel::default(),
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
        };
        h.app.layout = Layout {
            projects: panel(0, 12),
            repositories: panel(13, 12),
            checkouts: panel(26, 12),
            panes: panel(39, 12),
            content: panel(52, 20),
            overlay: Panel::default(),
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
        h.app.on_mouse(click(54, 3));
        assert_eq!(h.app.focus, Focus::PaneContent);
        assert!(
            h.sent().iter().any(|m| matches!(m, ClientMsg::Input { .. })),
            "the child gets the click too"
        );
    }

    #[test]
    fn releasing_in_the_live_view_is_forwarded_when_not_resizing() {
        let mut h = Harness::new();
        laid_out(&mut h);
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
    fn the_mouse_is_ignored_while_a_modal_is_open() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.key(KeyCode::Char('n'));
        h.app.on_mouse(click(14, 3));
        assert_eq!(h.app.sel_checkout, 0, "click must not navigate behind the prompt");
        assert!(h.app.prompt.is_some());
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
        h.app.open_overlay_pane(PaneId(700), "vim".to_string(), false);
        h.app.layout.overlay = Panel {
            outer: Rect::new(2, 1, 60, 20),
            inner: Rect::new(3, 2, 58, 18),
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
        h.app.on_server_msg(ServerMsg::Workspaces(workspaces("work")));
        assert_eq!(h.app.open_workspace, "work");
        assert_eq!(h.app.workspaces.len(), 3);
    }

    #[test]
    fn w_opens_a_picker_positioned_on_the_workspace_already_open() {
        // "Look at where I am, then move" is the reason to press it, so
        // starting at the top of the list would be the wrong default.
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Workspaces(workspaces("work")));
        h.key(KeyCode::Char('w'));

        let picker = h.app.picker.as_ref().expect("w should open the picker");
        assert_eq!(picker.sel, 1, "starts on the open one");
        assert!(picker.items[1].starts_with("work"));
    }

    #[test]
    fn choosing_a_workspace_asks_the_daemon_to_switch() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Workspaces(workspaces("default")));
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
        h.app.on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.keys("lllj"); // wander into the pane column
        h.sent();

        h.key(KeyCode::Char('w'));
        h.key(KeyCode::Down);
        h.key(KeyCode::Enter);

        assert_eq!(h.app.focus, Focus::Projects);
        assert_eq!((h.app.sel_project, h.app.sel_checkout, h.app.sel_pane), (0, 0, 0));
    }

    #[test]
    fn escaping_the_workspace_picker_switches_nothing() {
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Workspaces(workspaces("default")));
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
        h.app.on_server_msg(ServerMsg::Workspaces(workspaces("default")));
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
        h.app.on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        h.app.picker.as_mut().unwrap().type_query("weekend");
        assert_eq!(h.app.picker.as_ref().unwrap().create, None);
    }

    #[test]
    fn workspace_rows_are_matched_on_their_names_not_their_counts() {
        // The rows carry "2\u{25a3}"; typing a digit must not "find" a
        // workspace by how many panes it happens to be running.
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        h.app.picker.as_mut().unwrap().type_query("2");
        let p = h.app.picker.as_ref().unwrap();
        assert!(p.shown.is_empty(), "no workspace is named 2: {:?}", p.shown);
        assert_eq!(p.create.as_deref(), Some("2"), "it is a name to make instead");
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
        assert_eq!((h.app.sel_project, h.app.sel_checkout, h.app.sel_pane), (0, 0, 0));
    }

    #[test]
    fn the_top_row_still_switches_rather_than_creating() {
        // The create row sits below the matches; enter on a match is a
        // switch, exactly as it was before the row existed.
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        h.keys("week");
        h.key(KeyCode::Enter);

        match &h.sent()[0] {
            ClientMsg::OpenWorkspace { workspace } => {
                assert_eq!(*workspace, argus_protocol::WorkspaceId(3), "the 'weekend' row");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn the_picker_shows_how_much_is_running_in_each_workspace() {
        // The reason to surface counts at all: an agent working somewhere
        // you are not looking should still be visible.
        let mut h = Harness::new();
        h.app.on_server_msg(ServerMsg::Workspaces(workspaces("default")));
        h.key(KeyCode::Char('w'));
        let items = &h.app.picker.as_ref().unwrap().items;
        assert!(items[2].contains("2▣"), "weekend has two live panes: {items:?}");
        assert!(!items[0].contains('▣'), "an idle workspace stays quiet: {items:?}");
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
            base: argus_protocol::ReviewBase::WorkingTree,
            target_snapshot: "target-1".to_string(),
            baseline_snapshot: None,
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
                        },
                        argus_protocol::DiffLine {
                            kind: argus_protocol::LineKind::Added,
                            old_lineno: None,
                            new_lineno: Some(2),
                            text: "new".to_string(),
                        },
                    ],
                }],
                note: None,
            }],
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

        h.app.on_server_msg(ServerMsg::Review(diff_of(CheckoutId(9999))));
        assert!(h.app.review.is_none(), "not for the checkout we asked about");

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
                base: argus_protocol::ReviewBase::WorkingTree,
                target_snapshot: "target-1".to_string(),
                baseline_snapshot: None,
                files: Vec::new(),
            },
        );
        assert!(h.app.review.is_none());
        assert_ne!(h.app.focus, Focus::Review);
        assert!(h.app.status.contains("no changes vs uncommitted"), "{}", h.app.status);
    }

    #[test]
    fn an_empty_first_since_last_looked_review_can_be_acknowledged() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        let mut review = diff_of(checkout);
        review.base = ReviewBase::SinceLastLooked;
        review.files.clear();
        open_review(&mut h, review);
        assert!(h.app.review.is_some(), "empty view keeps the explicit action reachable");

        h.key(KeyCode::Char('A'));
        assert!(matches!(
            &h.sent()[0],
            ClientMsg::AcknowledgeReview {
                checkout: id,
                target_snapshot,
                expected_baseline: None,
            } if *id == checkout && target_snapshot == "target-1"
        ));
    }

    #[test]
    fn acknowledgement_updates_only_the_review_snapshot_that_was_displayed() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));

        h.app.on_server_msg(ServerMsg::ReviewAcknowledged {
            checkout,
            target_snapshot: "stale-target".to_string(),
        });
        assert_eq!(
            h.app.review.as_ref().unwrap().review.baseline_snapshot,
            None,
            "a delayed acknowledgement must not mutate a newer review"
        );

        h.app.on_server_msg(ServerMsg::ReviewAcknowledged {
            checkout,
            target_snapshot: "target-1".to_string(),
        });
        assert_eq!(
            h.app.review.as_ref().unwrap().review.baseline_snapshot.as_deref(),
            Some("target-1")
        );
        assert_eq!(h.app.status, "review baseline accepted");
    }

    #[test]
    fn acknowledgement_failure_is_shown_only_for_the_displayed_snapshot() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));
        let before = h.app.status.clone();

        h.app.on_server_msg(ServerMsg::ReviewAcknowledgeFailed {
            checkout,
            target_snapshot: "stale-target".to_string(),
            message: "conflict".to_string(),
        });
        assert_eq!(h.app.status, before);

        h.app.on_server_msg(ServerMsg::ReviewAcknowledgeFailed {
            checkout,
            target_snapshot: "target-1".to_string(),
            message: "conflict".to_string(),
        });
        assert_eq!(h.app.status, "error: conflict");
    }

    #[test]
    fn closing_and_refreshing_never_acknowledge_a_review() {
        let mut h = Harness::new();
        let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
        open_review(&mut h, diff_of(checkout));
        h.key(KeyCode::Char('r'));
        h.key(KeyCode::Esc);
        assert!(h.sent().iter().all(|message| !matches!(
            message,
            ClientMsg::AcknowledgeReview { .. }
        )));
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
            !h.sent().iter().any(|m| matches!(m, ClientMsg::Input { .. })),
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
            },
            PaneInfo {
                id: PaneId(51),
                kind: PaneKind::Agent,
                title: "claude".to_string(),
                status: PaneStatus::Idle,
                note: None,
                template: None,
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
        h.key(KeyCode::Char('j'));
        h.key(KeyCode::Char('c'));
        h.keys("fix this");
        h.key(KeyCode::Enter);

        match &h.sent()[0] {
            ClientMsg::Input { pane, bytes } => {
                assert_eq!(*pane, PaneId(51), "the agent, not the shell");
                assert_eq!(
                    String::from_utf8_lossy(bytes),
                    "src/a.rs:2 `+new`: fix this\r"
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(h.app.prompt.is_none());
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
            ClientMsg::Input { bytes, .. } => assert_eq!(
                String::from_utf8_lossy(bytes),
                "src/a.rs:1-2 (2 lines): both lines\r"
            ),
            other => panic!("unexpected {other:?}"),
        }
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
    fn b_cycles_the_diff_base_and_asks_again() {
        let mut h = review_with_agent();
        h.key(KeyCode::Char('b'));

        match &h.sent()[0] {
            ClientMsg::Review { base, .. } => {
                assert_eq!(*base, argus_protocol::ReviewBase::BranchPoint)
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
                assert_eq!(*base, argus_protocol::ReviewBase::BranchPoint)
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
            h.sent().iter().any(|m| matches!(m, ClientMsg::Review { .. })),
            "leader-Tab should ask for the diff"
        );
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
        assert!(h.app.picker.is_none(), "no picker until the list is in hand");
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
        assert_eq!(h.app.picker.as_ref().unwrap().selected(), Some("feature/login"));
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

        h.app.open_overlay_pane(PaneId(700), "vim".to_string(), false);

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
    fn typing_in_a_floating_pane_reaches_its_child() {
        let mut h = Harness::new();
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.sent();

        h.keys("iabc");

        let typed: Vec<u8> = h
            .sent()
            .into_iter()
            .filter_map(|m| match m {
                ClientMsg::Input { pane: PaneId(101), bytes } => Some(bytes),
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
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.keys("q");
        assert!(!h.app.should_quit, "q is the editor's, not ours");
        assert!(h.app.overlay.is_some());
    }

    #[test]
    fn the_leader_closes_a_floating_pane_and_leaves_it_running() {
        let mut h = Harness::new();
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);
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
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.sent();

        h.leader();
        h.key(KeyCode::Char('x'));

        assert!(h.sent().iter().any(|m| matches!(m, ClientMsg::Kill { pane } if *pane == PaneId(101))));
        assert!(h.app.overlay.is_none());
    }

    #[test]
    fn closing_a_floating_pane_puts_the_live_view_back_on_the_column() {
        let mut h = Harness::new();
        h.keys("lll");
        h.sent();
        let was = h.app.column_pane();

        h.app.open_overlay_pane(PaneId(999), "vim".to_string(), false);
        h.leader();
        h.key(KeyCode::Esc);

        assert_eq!(h.app.column_pane(), was, "back to what the columns show");
        assert!(!h.app.grids.contains_key(&PaneId(999)), "and the editor is dropped");
    }

    #[test]
    fn a_floating_pane_and_the_column_are_sized_separately() {
        let mut h = Harness::new();
        laid_out(&mut h);
        assert_eq!(h.app.live_panes()[0].1, h.app.layout.content.inner);

        h.app.open_overlay_pane(PaneId(700), "vim".to_string(), false);
        h.app.layout.overlay = Panel {
            outer: Rect::new(2, 1, 40, 20),
            inner: Rect::new(3, 2, 38, 18),
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
        for _ in 0..crate::app::Setting::ALL.len() {
            h.key(KeyCode::Char('j')); // down to the theme row, wherever it is
        }
        h.key(KeyCode::Char('l'));

        assert_eq!(h.app.theme, crate::theme::Theme::by_name(&h.app.settings.theme));
        assert_ne!(h.app.settings.theme, "mocha");
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

        assert!(!h.sent().iter().any(|m| matches!(m, ClientMsg::Input { .. })));
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
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.sent();

        h.key(KeyCode::F(12));

        assert!(h.app.overlay.is_none());
        assert!(
            !h.sent().iter().any(|m| matches!(m, ClientMsg::Input { .. })),
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
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.app.layout.overlay = Panel {
            outer: Rect::new(10, 4, 20, 10),
            inner: Rect::new(11, 5, 18, 8),
        };

        h.app.on_mouse(click(1, 1)); // out on the projects column

        assert!(h.app.overlay.is_none());
    }

    #[test]
    fn a_click_inside_the_window_belongs_to_its_pane() {
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.app.layout.overlay = Panel {
            outer: Rect::new(10, 4, 20, 10),
            inner: Rect::new(11, 5, 18, 8),
        };
        h.sent();

        h.app.on_mouse(click(15, 7));

        assert!(h.app.overlay.is_some(), "still open");
        assert!(h.sent().iter().any(|m| matches!(m, ClientMsg::Input { .. })));
    }

    #[test]
    fn a_click_under_a_floating_window_never_reaches_the_columns() {
        // The bug this exists for: focus moved to a column while the keys
        // still went to the overlay, leaving no way in and no way out.
        let mut h = Harness::new();
        laid_out(&mut h);
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);
        h.app.layout.overlay = Panel {
            outer: Rect::new(10, 4, 20, 10),
            inner: Rect::new(11, 5, 18, 8),
        };
        let before = h.app.sel_project;

        h.app.on_mouse(click(1, 3)); // a project row, underneath

        assert_eq!(h.app.sel_project, before, "the click dismissed, it did not select");
    }

    #[test]
    fn a_window_whose_pane_exits_closes_itself() {
        // Otherwise it sits there showing a dead grid — the shape of a hung
        // editor, with no sign anything is wrong.
        let mut h = Harness::new();
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);

        h.app.on_server_msg(ServerMsg::PaneClosed {
            pane: PaneId(101),
            code: Some(0),
        });

        assert!(h.app.overlay.is_none());
    }

    #[test]
    fn another_panes_exit_leaves_the_window_alone() {
        let mut h = Harness::new();
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);
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
        h.app.open_overlay_pane(PaneId(101), "vim".to_string(), false);

        let mut t = tree();
        t[0].repositories[0].checkouts[0].panes.retain(|p| p.id != PaneId(101));
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
        assert_eq!(h.app.overlay_pane(), Some(PaneId(700)), "editor is the window");
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
        assert_eq!(h.app.tree[0].repositories[0].checkouts[0].listed_panes().count(), 2);
    }

    #[test]
    fn closing_an_editors_window_ends_the_editor() {
        // Nothing lists it afterwards, so a survivor would be a process
        // with no window and no way back to it.
        let mut h = Harness::new();
        h.app.open_overlay_pane(PaneId(700), "a.rs".to_string(), true);
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
        h.app.open_overlay_pane(PaneId(101), "shell".to_string(), false);
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
        assert!(h.app.grids.contains_key(&watching.unwrap()), "still streaming");
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

}
