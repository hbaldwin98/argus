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

    /// Points the app at a new connection after the old one died.
    ///
    /// Subscriptions belong to a connection, and so do the grids they fill:
    /// the new daemon has never heard of this client, and the cells in hand
    /// are from a pty that may have moved on or been restored from disk
    /// since. Dropping them is what makes [`Self::sync_subscription`] ask
    /// for a fresh snapshot of everything on screen rather than leaving the
    /// columns showing a screen nobody is updating any more.
    pub fn reconnect(&mut self, out: UnboundedSender<ClientMsg>) {
        self.out = out;
        self.grids.clear();
        self.sync_subscription();
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
                if let Some(previous) = self.grids.get(&pane) {
                    // A snapshot is how a resize reaches the client, so a
                    // parked view has to be re-read: its rows are the old
                    // width and nothing else will replace them.
                    let parked = previous.scrollback.as_ref().map(|sb| sb.offset);
                    let mut grid = Grid::with_cursor(cells, cursor, mouse);
                    grid.alternate_screen = alternate_screen;
                    self.grids.insert(pane, grid);
                    if let Some(offset) = parked {
                        self.park_pane(pane, offset);
                    }
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
            ServerMsg::ScrollbackRows {
                pane,
                offset,
                depth,
                cells,
            } => {
                self.receive_scrollback(pane, offset, depth, cells);
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
            // Adopted only when it is the board on screen: every client
            // is told about every project's board, because the daemon does
            // not track which view anyone has open.
            ServerMsg::Decisions(board) => {
                let ours = self
                    .current_project()
                    .map(|p| p.name == board.name)
                    .unwrap_or(false);
                if ours {
                    // Held by slug across the swap: a board arriving while
                    // an agent writes must not move the reader to another
                    // feature's tree.
                    let was = self.current_feature_row().and_then(|row| row.slug);
                    self.board = Some(*board);
                    if let Some(slug) = was {
                        if let Some(at) = self
                            .feature_rows()
                            .iter()
                            .position(|row| row.slug.as_deref() == Some(slug.as_str()))
                        {
                            self.feature_sel = at;
                        }
                    }
                    self.rescope_feature();
                }
            }
            ServerMsg::Tasks(list) => {
                // A push for a feature the view is not on is another
                // client's business, exactly as a board for another
                // project is.
                let ours = self.feature_slug() == list.feature;
                if ours {
                    // Held by id across the swap, so a task reordered or
                    // moved under the cursor is still the task under the
                    // cursor: a card you have to go looking for reads as
                    // having been lost.
                    let was = self.selected_task().map(|task| task.id);
                    self.tasks = Some(*list);
                    match was
                        .and_then(|id| self.feature_tasks().iter().position(|task| task.id == id))
                    {
                        Some(at) => self.task_sel = at,
                        None => self.clamp_task_selection(),
                    }
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
            ServerMsg::CommitFiles {
                checkout,
                commit,
                files,
            } => {
                self.receive_commit_files(checkout, &commit, files);
            }
            ServerMsg::CommitFilesFailed {
                checkout,
                commit,
                message,
            } => {
                // Only worth an alert while the row it belongs to is still
                // on screen, unfolded and waiting for it.
                if self
                    .history
                    .as_mut()
                    .is_some_and(|v| v.checkout == checkout && v.fail_files(&commit))
                {
                    self.alert(format!("commit files: {message}"));
                }
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
            // Straight back into the same popup with the harder question,
            // rather than an alert the user would have to answer by
            // finding the row again.
            ServerMsg::BranchNotMerged { checkout, branch } => {
                self.prompt = Some(Prompt::ConfirmRemove {
                    target: RemoveTarget::Branch {
                        checkout,
                        branch: branch.clone(),
                        force: true,
                    },
                    label: branch,
                });
            }
            ServerMsg::Error { message } => {
                self.alert(format!("error: {message}"));
            }
            ServerMsg::Restarting => {}
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
            let alive = panes_in(&self.tree).any(|p| p.id == pane);
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
        // Last, because it asks about the project the selection has just
        // landed on rather than the one it started the tree on.
        self.refresh_board_if_stale();
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
            self.state_flashes
                .insert(pane, crate::motion::Animation::starting(now, STATE_FLASH));
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

    /// How much of the pane's state flash is left to draw, `1.0` at its
    /// brightest and falling to nothing. `None` once it is over.
    ///
    /// Eased, so the highlight leaves the way a thing settles rather than
    /// at a constant rate — and inverted from raw progress, because what
    /// the renderer wants is how much wash to mix in, not how much of the
    /// animation has gone by.
    pub fn flash_strength(&self, pane: PaneId) -> Option<f32> {
        let progress = self.state_flashes.get(&pane)?.progress(self.frame_now)?;
        Some(1.0 - crate::motion::ease_out(progress))
    }

    /// When the next frame is owed to something moving: a flash still
    /// fading, or a spinner about to change glyph. `None` when the screen
    /// is settled and the loop can sleep until something happens.
    pub fn next_motion_deadline(&self) -> Option<std::time::Instant> {
        let fading = self
            .state_flashes
            .values()
            .map(|anim| anim.deadline())
            .min();
        let spinning = self
            .any_pane_working()
            .then(|| crate::motion::spinner_deadline(self.frame_now, self.epoch));
        let travelling = self
            .focus_from
            .filter(|(_, anim)| anim.progress(self.frame_now).is_some())
            .map(|(_, anim)| anim.deadline());
        [fading, spinning, travelling].into_iter().flatten().min()
    }

    /// Whether anything on screen is mid-turn, and so whether the spinner
    /// is asking for frames at all.
    fn any_pane_working(&self) -> bool {
        crate::app::panes_in(&self.tree).any(|p| p.status == PaneStatus::Working)
    }

    pub fn expire_state_flashes(&mut self, now: std::time::Instant) {
        self.state_flashes
            .retain(|_, anim| anim.progress(now).is_some());
    }

    /// The clock the frame about to be drawn reads. Set once per frame so
    /// every animation in it agrees on the time.
    ///
    /// Also where a focus move is noticed. Focus is assigned from nav,
    /// mouse, actions and input, so starting the fade at each of those
    /// would be a rule to remember in four files; comparing what is about
    /// to be drawn against what was drawn last cannot be missed.
    pub fn set_frame_now(&mut self, now: std::time::Instant) {
        self.frame_now = now;
        if self.focus != self.focus_shown {
            self.focus_from = Some((
                self.focus_shown,
                crate::motion::Animation::starting(now, crate::motion::FOCUS_FADE),
            ));
            self.focus_shown = self.focus;
        }
    }

    /// How lit a panel should be drawn: `1.0` for the focused card, `0.0`
    /// for a receded one, and the two of them trading places while focus
    /// travels between them.
    ///
    /// Focus used to snap, and a card that is simply *replaced* by another
    /// tells you where focus ended up but not that it moved — which is the
    /// half that makes a spine of five cards read as one place you are
    /// moving through rather than five that take turns lighting up.
    pub fn focus_lit(&self, panel: Focus) -> crate::motion::Lit {
        let moving = self
            .focus_from
            .and_then(|(from, anim)| Some((from, anim.progress(self.frame_now)?)));
        let Some((from, progress)) = moving else {
            return (self.focus == panel).into();
        };
        let t = crate::motion::ease_out(progress);
        if self.focus == panel {
            t.into()
        } else if from == panel {
            (1.0 - t).into()
        } else {
            crate::motion::Lit::OFF
        }
    }

    pub fn frame_now(&self) -> std::time::Instant {
        self.frame_now
    }

    pub fn epoch(&self) -> std::time::Instant {
        self.epoch
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
        let mut view = ReviewView::new(review, self.review_split);
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

    fn receive_commit_files(
        &mut self,
        checkout: CheckoutId,
        commit: &str,
        files: Vec<argus_protocol::CommitFile>,
    ) {
        let Some(view) = self.history.as_mut() else {
            return;
        };
        if view.checkout != checkout {
            return;
        }
        let n = files.len();
        if view.receive_files(commit, files) {
            let s = if n == 1 { "" } else { "s" };
            self.report(format!("{n} file{s} changed"));
        }
    }

    fn receive_commits(
        &mut self,
        request_id: u64,
        checkout: CheckoutId,
        commits: Vec<argus_protocol::CommitInfo>,
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
