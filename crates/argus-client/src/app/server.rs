//! Everything the daemon says, and what the client does about it.
//!
//! One rule shapes the whole module: a message is applied to the model and
//! nothing else. Rendering reads the model afterwards, so a tree arriving
//! mid-keystroke can never half-apply — and the selection fixups here run
//! before anyone can observe the new tree.

use super::*;

impl App {
    /// Subscribes to everything currently on screen and drops the rest.
    pub(super) fn sync_subscription(&mut self) {
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
                pane,
                cells,
                cursor,
                mouse,
                alternate_screen,
                ..
            } => {
                if self.grids.contains_key(&pane) {
                    let mut grid = Grid::with_cursor(cells, cursor, mouse);
                    grid.alternate_screen = alternate_screen;
                    self.grids.insert(pane, grid);
                }
            }
            ServerMsg::Damage {
                pane,
                spans,
                cursor,
                mouse,
                alternate_screen,
            } => {
                if let Some(grid) = self.grids.get_mut(&pane) {
                    grid.apply(&spans);
                    grid.move_cursor(cursor);
                    grid.mouse = mouse;
                    grid.alternate_screen = alternate_screen;
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
            ServerMsg::ReviewCommentSaved { id, delivered } => {
                if delivered {
                    self.report(format!("comment #{id} saved and sent"));
                } else {
                    self.report(format!("comment #{id} saved; agent unavailable"));
                }
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
            ServerMsg::Commits {
                request_id,
                checkout,
                commits,
            } => {
                self.receive_commits(request_id, checkout, commits);
            }
            ServerMsg::CommitsFailed {
                request_id,
                checkout,
                message,
            } => {
                // A list we have already navigated away from must not
                // raise an alert for a view that is no longer open.
                if self.history_wanted == Some((checkout, request_id)) {
                    self.history_wanted = None;
                    self.alert(format!("history: {message}"));
                }
            }
            ServerMsg::Directories(listing) => {
                // A listing for a directory we have already navigated away
                // from would yank the browser backwards.
                let Some(picker) = &mut self.dir_picker else {
                    return;
                };
                if picker.pending != Some(listing.request_id) {
                    return;
                }
                picker.show(listing);
            }
            ServerMsg::Error { message } => {
                self.alert(format!("error: {message}"));
            }
        }
    }

    fn receive_tree(&mut self, tree: Vec<ProjectInfo>) {
        self.record_state_transitions(&tree);
        let selected_pane = matches!(self.focus, Focus::Panes | Focus::PaneContent)
            .then(|| self.current_pane().map(|pane| pane.id))
            .flatten();
        // Columns select by index, so any row appearing above the cursor
        // moves it onto a different checkout without the user touching
        // anything. Remember what was selected, not where it sat.
        let checkout_anchor = self.checkout_anchor();
        self.tree = tree;
        let mut followed_pane = false;
        if let Some(selected_pane) = selected_pane {
            if let Some((project, repository, checkout, pane)) = self
                .tree
                .iter()
                .enumerate()
                .find_map(|(project_index, project)| {
                    project.repositories.iter().enumerate().find_map(
                        |(repository_index, repository)| {
                            repository.checkouts.iter().enumerate().find_map(
                                |(checkout_index, checkout)| {
                                    checkout
                                        .listed_panes()
                                        .position(|candidate| candidate.id == selected_pane)
                                        .map(|pane_index| {
                                            (
                                                project_index,
                                                repository_index,
                                                checkout_index,
                                                pane_index,
                                            )
                                        })
                                },
                            )
                        },
                    )
                })
            {
                self.sel_project = project;
                self.sel_repository = repository;
                if let Some(row) = self.checkout_row_of(checkout) {
                    self.sel_checkout = row;
                    self.sel_pane = pane;
                    followed_pane = true;
                }
            }
        }
        // Following a pane already moved the cursor deliberately; the
        // anchor is only for the columns nothing else re-aimed.
        if !followed_pane {
            if let Some((repository, anchor)) = &checkout_anchor {
                self.restore_checkout_anchor(*repository, anchor);
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
        if let Some(project_id) = self.pending_focus_new_repository.take() {
            if let Some((index, project)) = self
                .tree
                .iter()
                .enumerate()
                .find(|(_, p)| p.id == project_id)
            {
                if !project.repositories.is_empty() {
                    self.sel_project = index;
                    self.sel_repository = project.repositories.len() - 1;
                    self.clamp();
                }
            }
        }
        if let Some(repository_id) = self.pending_focus_new_checkout.take() {
            if let Some((project, repository)) =
                self.tree
                    .iter()
                    .enumerate()
                    .find_map(|(project_index, project)| {
                        project.repositories.iter().enumerate().find_map(
                            |(repository_index, repository)| {
                                (repository.id == repository_id)
                                    .then_some((project_index, repository_index))
                            },
                        )
                    })
            {
                self.sel_project = project;
                self.sel_repository = repository;
                let newest = self
                    .current_repository()
                    .map(|r| r.checkouts.len().saturating_sub(1))
                    .unwrap_or(0);
                self.sel_checkout = self.checkout_row_of(newest).unwrap_or(0);
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
        if self.pane_fullscreen
            && (self.focus != Focus::PaneContent
                || selected_pane.is_none()
                || self.column_pane() != selected_pane)
        {
            self.leader_pending = false;
            self.pane_fullscreen = false;
            if self.focus == Focus::PaneContent {
                self.focus = Focus::Panes;
            }
        }
    }

    fn record_state_transitions(&mut self, next: &[ProjectInfo]) {
        if self.tree.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let mut transitions = Vec::new();
        for pane in panes_in(next) {
            let Some(previous) = panes_in(&self.tree).find(|old| old.id == pane.id) else {
                continue;
            };
            let before = effective_state(previous);
            let after = effective_state(pane);
            if before.0 != after.0 {
                transitions.push((
                    pane.id,
                    before.0,
                    after.0,
                    effective_label(pane, after.1),
                    after.2.map(str::to_string),
                ));
            }
        }
        for (pane, before, after, label, note) in transitions {
            self.state_flashes.insert(pane, now + STATE_FLASH);
            if after.needs_you() && (!before.needs_you() || before != after) {
                let message = note
                    .filter(|note| !note.is_empty())
                    .map(|note| format!("{label}: {note}"))
                    .unwrap_or_else(|| format!("{label}: {}", state_word(after)));
                self.alert(message);
                if self.settings.notifications == crate::settings::NotificationMode::Bell
                    && self.input_pane() != Some(pane)
                {
                    self.bell_pending = true;
                }
            }
        }
    }

    pub fn pane_is_flashing(&self, pane: PaneId) -> bool {
        self.state_flashes
            .get(&pane)
            .is_some_and(|deadline| *deadline > std::time::Instant::now())
    }

    pub fn next_flash_deadline(&self) -> Option<std::time::Instant> {
        self.state_flashes.values().copied().min()
    }

    pub fn expire_state_flashes(&mut self, now: std::time::Instant) {
        self.state_flashes.retain(|_, deadline| *deadline > now);
    }

    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.bell_pending)
    }

    fn receive_pane_closed(&mut self, pane: PaneId, code: Option<i32>) {
        // Otherwise the window sits there showing a dead grid, which is
        // exactly what a hung editor looks like.
        if self.overlay_pane() == Some(pane) {
            self.close_overlay();
        }
        if self.pane_fullscreen && self.column_pane() == Some(pane) {
            self.leader_pending = false;
            self.pane_fullscreen = false;
            self.focus = Focus::Panes;
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
        let label = match &review.commit {
            Some(c) => format!("{} {}", c.short, c.summary),
            None => format!("vs {}", review.base.label()),
        };
        let mut view = ReviewView::new(review);
        if view.is_empty() {
            self.review = None;
            self.pending_history_file = None;
            self.report(format!("no changes {label}"));
            return;
        }
        if let Some(path) = self.pending_history_file.take() {
            if let Some(i) = view.review.files.iter().position(|f| f.path == path) {
                view.jump_to_file(i);
            }
        }
        self.review = Some(view);
        self.overlay = Some(Overlay::Review);
        self.pane_fullscreen = false;
        self.focus = Focus::Review;
        self.report(format!("{files} changed {label}"));
    }

    fn receive_commits(
        &mut self,
        request_id: u64,
        checkout: CheckoutId,
        commits: Vec<argus_protocol::HistoryCommit>,
    ) {
        if self.history_wanted != Some((checkout, request_id)) {
            return;
        }
        self.history_wanted = None;
        if commits.is_empty() {
            self.history = None;
            self.report("no commits yet");
            return;
        }
        let n = commits.len();
        self.review = None;
        self.history = Some(crate::history::HistoryView::new(checkout, commits));
        self.overlay = Some(Overlay::History);
        self.pane_fullscreen = false;
        self.focus = Focus::Review;
        self.report(format!("{n} commits"));
    }

    fn receive_review_failure(&mut self, request_id: u64, checkout: CheckoutId, message: String) {
        if self.review_wanted == Some((checkout, request_id)) {
            self.review_wanted = None;
            self.alert(format!("error: {message}"));
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

    pub(super) fn clear_status(&mut self) {
        self.status.clear();
        self.status_alert = false;
    }
}
