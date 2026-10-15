//! Everything the daemon says, and what the client does about it.
//!
//! One rule shapes the whole module: a message is applied to the model and
//! nothing else. Rendering reads the model afterwards, so a tree arriving
//! mid-keystroke can never half-apply — and the selection fixups here run
//! before anyone can observe the new tree.

use super::*;
use argus_protocol::review_session::Answered;
use argus_protocol::{Hello, Received};

impl App {
    /// Subscribes to everything currently on screen and drops the rest.
    pub(super) fn sync_subscription(&mut self) {
        let want: Vec<PaneId> = if self.on_screen {
            [self.column_pane(), self.overlay_pane()]
                .into_iter()
                .flatten()
                .collect()
        } else {
            Vec::new()
        };

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
                slot.insert(PaneView::new(id));
                let _ = self.out.send(ClientMsg::Subscribe { pane: id });
            }
        }
    }

    /// Puts this app's host on screen or takes it off. Off screen it lets
    /// go of every pane; back on, it asks again for the ones it shows.
    pub fn set_on_screen(&mut self, on_screen: bool) {
        self.on_screen = on_screen;
        self.sync_subscription();
        self.report_view();
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
        // Pushed while the daemon was away means pushed to nobody: the
        // next tree asks for the board again.
        self.board_follower.reconnected();
        self.sync_subscription();
    }

    /// Says so when the daemon this client reached is not from its own
    /// build. `None` is a daemon that gave no greeting: one from before the
    /// handshake.
    pub fn greeted(&mut self, daemon: Option<&Hello>) {
        let ours = Hello::this_build();
        let (argusd, fix) = self.daemon_and_its_fix();
        self.pastes_files = daemon.is_some_and(|d| d.can(argus_protocol::PASTE_FILE));
        // A daemon newly reached has been told nothing.
        self.reports_view = daemon.is_some_and(|d| d.can(argus_protocol::FOCUS));
        self.view_sent = None;
        let Some(daemon) = daemon else {
            self.report(format!("{argusd} predates this client; {fix} to update it"));
            return;
        };
        if !daemon.speaks_ours() {
            self.alert(format!(
                "{argusd} {} cannot fully talk to this client {}; {fix} to match",
                daemon.version, ours.version
            ));
        } else if daemon.version != ours.version {
            self.report(format!(
                "{argusd} is {}, this client {}; {fix} to match",
                daemon.version, ours.version
            ));
        }
    }

    /// What to call the daemon, and what to do about one that is behind:
    /// here, `server restart` starts the daemon installed beside this
    /// client; on another machine, argus there has to be updated.
    pub(super) fn daemon_and_its_fix(&self) -> (String, String) {
        match &self.host {
            None => ("argusd".to_string(), "`argus server restart`".to_string()),
            Some(host) => (format!("argusd on {host}"), format!("update argus on {host}")),
        }
    }

    pub fn on_server_msg(&mut self, msg: ServerMsg) {
        match msg {
            // A greeting answered after the client stopped waiting for it.
            ServerMsg::Hello(hello) => self.greeted(Some(&hello)),
            ServerMsg::Tree(tree) => self.receive_tree(tree),
            ServerMsg::PaneTelemetry { pane, telemetry } => {
                self.tree.apply_telemetry(pane, telemetry);
            }
            ServerMsg::Templates(names) => {
                self.templates = names;
            }
            // The desktop window's for now; the terminal client lists no
            // consoles yet.
            ServerMsg::Consoles(_) => {}
            ServerMsg::Workspaces(list) => {
                self.open_workspace = list
                    .iter()
                    .find(|w| w.open)
                    .map(|w| w.name.clone())
                    .unwrap_or_default();
                self.workspaces = list;
            }
            // Only for a pane this client holds a view of: one it let go of
            // meanwhile stays gone.
            msg @ (ServerMsg::PaneSnapshot { .. }
            | ServerMsg::PaneRows { .. }
            | ServerMsg::Damage { .. }
            | ServerMsg::RowDamage { .. }
            | ServerMsg::ScrollbackRows { .. }
            | ServerMsg::ScrollbackRuns { .. }) => {
                let effect = PaneView::about(&msg)
                    .and_then(|pane| self.grids.get_mut(&pane))
                    .and_then(|view| view.receive(&msg));
                if let Some(effect) = effect {
                    self.take_view_effect(effect);
                }
            }
            ServerMsg::Clipboard { text, .. } => {
                if (self.clipboard_write)(&text) {
                    self.report("a pane copied to the clipboard");
                } else {
                    self.alert("could not write to the clipboard");
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
                if let Some(session) = &mut self.review {
                    session.commented();
                }
                if delivered {
                    self.report(format!("comment #{id} saved and sent"));
                } else {
                    self.report(format!("comment #{id} saved; agent unavailable"));
                }
            }
            ServerMsg::Decisions(board) => self.receive_decisions(*board),
            ServerMsg::Tasks(list) => self.receive_tasks(*list),
            ServerMsg::SequenceDiagrams(list) => self.receive_diagrams(*list),
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
                if self.history_request.take_if_wanted(checkout, request_id) {
                    self.alert(format!("history: {message}"));
                }
            }
            // The terminal client asks for no sessions yet.
            ServerMsg::Sessions { .. } | ServerMsg::SessionTranscript { .. } | ServerMsg::Resumes { .. } => {}
            // Nor a preview: its browser adds the directory it lands on.
            ServerMsg::ProjectPreview(_) => {}
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
            ServerMsg::BranchNotMerged {
                checkout,
                branch,
                commits,
            } => {
                let forced = argus_protocol::column_verbs::force_delete(checkout, branch.clone());
                let Some(target) = RemoveTarget::of(forced) else {
                    return;
                };
                self.prompt = Some(Prompt::ConfirmRemove {
                    target,
                    label: branch,
                    lost: commits,
                });
            }
            ServerMsg::Error { message } => {
                self.alert(format!("error: {message}"));
            }
            ServerMsg::Created {
                request_id,
                created,
            } => {
                self.awaited.answer(request_id, created);
                self.settle_awaited();
            }
            ServerMsg::Restarting => {}
            // Sent only to a client watching the focus, which this is not.
            ServerMsg::Focus(_) => {}
            ServerMsg::Stopping => self.should_quit = true,
        }
    }

    /// Adopted only when it is the board on screen: every client is told
    /// about every project's board, because the daemon does not track which
    /// view anyone has open. The follower decides, as it does for the
    /// desktop window, and settles the feature by slug, so a board arriving
    /// while an agent writes does not move the reader to another feature.
    fn receive_decisions(&mut self, board: argus_protocol::DecisionBoard) {
        let row = self.selected_work();
        let mut out = Vec::new();
        if !self.board_follower.receive_board(board, &self.tree, self.show_folded_features, &mut out) {
            return;
        }
        self.send_board_requests(out);
        self.land_on_feature();
        self.keep_selection(row);
    }

    /// Held by id across the swap, so a task reordered or moved under the
    /// cursor is still the task under the cursor: a card you have to go
    /// looking for reads as having been lost.
    fn receive_tasks(&mut self, list: argus_protocol::TaskList) {
        let on = self.selected_work();
        if let Received::Adopted { lands } = self.board_follower.receive_tasks(list, on) {
            self.keep_selection(lands);
        }
    }

    fn receive_diagrams(&mut self, list: argus_protocol::DiagramList) {
        let on = self.selected_diagram().map(|d| d.id);
        if let Received::Adopted { lands } = self.board_follower.receive_diagrams(list, on) {
            match lands.and_then(|id| self.feature_diagrams().iter().position(|d| d.id == id)) {
                Some(at) => self.diagram_sel = at,
                None => self.clamp_feature_cursors(),
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
            match &checkout_anchor {
                Some((repository, anchor)) => self.restore_checkout_anchor(*repository, anchor),
                None => self.sel_checkout = self.home_checkout_row(),
            }
        }
        self.clamp();
        // A pane killed from elsewhere leaves its window orphaned.
        if let Some(pane) = self.overlay.as_ref().and_then(Overlay::pane) {
            let alive = self.tree.panes().any(|p| p.id == pane);
            if !alive {
                self.close_overlay();
            }
        }
        // What a request of ours made may be in this tree.
        self.settle_awaited();
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
        // landed on rather than the one it started the tree on. A tree
        // regroups the index — an agent arriving makes a feature active —
        // so the cursor is settled on its feature by slug, not left on a
        // place another feature has moved into.
        self.follow_board();
        if self.view == View::Feature {
            let mut out = Vec::new();
            self.board_follower.settle(&self.tree, self.show_folded_features, &mut out);
            self.send_board_requests(out);
            self.land_on_feature();
        }
    }

    /// Alerts, and rings the bell if asked to, for each pane that came to
    /// need you. The tree held across a reconnect is the one compared
    /// with, so a pane that started waiting while the daemon was away still
    /// rings.
    fn record_state_transitions(&mut self, next: &[ProjectInfo]) {
        let alerts: Vec<(PaneId, String)> = argus_protocol::transitions(self.tree.as_slice(), next)
            .into_iter()
            .filter(|t| t.after.status.needs_you())
            .map(|t| {
                let label = effective_label(t.pane, t.after);
                let message = match t.after.note {
                    Some(note) if !note.is_empty() => format!("{label}: {note}"),
                    _ => format!("{label}: {}", state_word(t.after.status)),
                };
                (t.pane.id, message)
            })
            .collect();
        for (pane, message) in alerts {
            self.alert(message);
            if self.settings.notifications == crate::settings::NotificationMode::Bell
                && self.input_pane() != Some(pane)
            {
                self.bell_pending = true;
            }
        }
    }

    /// When the next frame is owed to something moving: a spinner about to
    /// change glyph, or focus still travelling. `None` when the screen is
    /// settled and the loop can sleep until something happens.
    pub fn next_motion_deadline(&self) -> Option<std::time::Instant> {
        let spinning = self
            .any_pane_working()
            .then(|| crate::motion::spinner_deadline(self.frame_now, self.epoch));
        let travelling = self
            .focus_from
            .filter(|(_, anim)| anim.progress(self.frame_now).is_some())
            .map(|(_, anim)| anim.deadline());
        [spinning, travelling].into_iter().flatten().min()
    }

    /// Whether anything on screen is mid-turn, and so whether the spinner
    /// is asking for frames at all.
    fn any_pane_working(&self) -> bool {
        self.tree.panes().any(|p| p.status == PaneStatus::Working)
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
    /// half that makes several cards read as one place you are moving
    /// through rather than several that take turns lighting up.
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
        if let Some(session) = &mut self.review {
            let answered = session.receive(review);
            self.review_answered(answered);
        }
    }

    /// What an answer came to, said and drawn. The review opens only once
    /// there is something in it: an empty side gives way to the other
    /// inside the session, and nothing on either says so instead.
    fn review_answered(&mut self, answered: Answered) {
        let label = |view: Option<&ReviewView>, base: ReviewBase| match view.and_then(|v| v.review.commit.as_ref()) {
            Some(c) => format!("{} {}", c.short, c.summary),
            None => format!("vs {}", base.label()),
        };
        match answered {
            Answered::Stale | Answered::Waiting | Answered::Updated => {}
            Answered::Shown { base, gave_way } => {
                let view = self.review_view();
                let files = view.map_or(0, |v| v.review.files.len());
                let label = label(view, base);
                self.overlay = Some(Overlay::Review);
                self.pane_fullscreen = false;
                self.focus = Focus::Review;
                match gave_way {
                    Some(side) => self.report(format!(
                        "nothing {}; {files} changed {label}",
                        side.label()
                    )),
                    None => self.report(format!("{files} changed {label}")),
                }
            }
            Answered::Nothing => {
                let commit = self.review.as_ref().is_some_and(|s| s.commit().is_some());
                let label = label(self.review_view(), ReviewBase::Commit);
                self.review = None;
                if commit {
                    self.report(format!("no changes {label}"));
                } else {
                    self.report("no changes, staged or unstaged");
                }
            }
            Answered::Failed(message) => self.alert(format!("error: {message}")),
        }
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
        if !self.history_request.take_if_wanted(checkout, request_id) {
            return;
        }
        if commits.is_empty() {
            self.history = None;
            self.report("no commits yet");
            return;
        }
        let n = commits.len();
        self.review = None;
        self.history = Some(argus_protocol::history_view::HistoryView::new(checkout, commits));
        self.overlay = Some(Overlay::History);
        self.pane_fullscreen = false;
        self.focus = Focus::Review;
        self.report(format!("{n} commits"));
    }

    fn receive_review_failure(&mut self, request_id: u64, checkout: CheckoutId, message: String) {
        if let Some(session) = &mut self.review {
            let answered = session.failed(checkout, request_id, message);
            self.review_answered(answered);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_intentional_server_stop_quits_the_connected_client() {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(tx);

        app.on_server_msg(ServerMsg::Stopping);

        assert!(app.should_quit);
    }
}
