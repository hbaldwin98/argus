//! What the operator asks for: panes started and stopped, git fetched and
//! pulled, reviews opened, messages typed at agents.
//!
//! Everything here is a request to the daemon, not a change to the model.
//! The row does not move until the tree comes back saying it did, so a
//! refused action leaves the panel showing what is actually true.

use crate::app::*;
use argus_protocol::tree::column::verbs::Verb;

impl App {
    /// Shows or hides the branches that have no checkout. The main branch
    /// keeps its row either way, so the toggle is about the rest of them.
    pub(in crate::app) fn toggle_branches(&mut self) {
        self.show_branches = !self.show_branches;
        self.sync_checkout_filter_selection();
        self.clamp();
        self.report(if self.show_branches {
            "showing every branch"
        } else {
            "showing checkouts only"
        });
    }

    /// `F` brings the remotes up to date. Nothing about the working tree
    /// changes, so it is safe to press from any row; what changes is which
    /// branches the column can show you.
    pub(in crate::app) fn fetch(&mut self) {
        if self.send_row_verb(Verb::Fetch) {
            self.report("fetching…");
        }
    }

    /// `P` moves the selected checkout up to its upstream, fast-forward
    /// only. On a branch row that is the primary checkout, which is the
    /// same checkout every other repository-wide action uses.
    pub(in crate::app) fn pull(&mut self) {
        if self.send_row_verb(Verb::Pull) {
            self.report("pulling…");
        }
    }

    /// `verb` on the selected checkouts-column row, as `tree::column::verbs`
    /// decides it: sent, or its refusal reported. False when nothing went.
    pub(in crate::app) fn send_row_verb(&mut self, verb: Verb) -> bool {
        match self.row_verb(verb) {
            Some(Ok(msg)) => {
                let _ = self.out.send(msg);
                true
            }
            Some(Err(refusal)) => {
                self.report(refusal);
                false
            }
            None => false,
        }
    }

    /// The checkout git runs in for the selected row. A console is a row
    /// with no checkout behind it.
    fn git_checkout(&self) -> Option<CheckoutId> {
        if self.current_console().is_some() {
            return None;
        }
        self.current_repository()?
            .git_runs_in(self.selected_checkout_row()?)
    }

    /// What `verb` on the selected row comes to; `None` with no row, or
    /// with a console's.
    pub(in crate::app) fn row_verb(&self, verb: Verb) -> Option<Result<ClientMsg, String>> {
        if self.current_console().is_some() {
            return None;
        }
        let repo = self.current_repository()?;
        Some(repo.act(self.selected_checkout_row()?, verb))
    }

    /// Sends the edited brief if it has changed since the last write.
    pub(in crate::app) fn save_brief(&mut self) {
        let checkout = self.current_checkout().map(|checkout| checkout.id);
        let Some(view) = &mut self.brief else {
            return;
        };
        if !view.dirty {
            return;
        }
        let body = view.body();
        view.saved();
        let _ = self.out.send(match view.target.clone() {
            BriefTarget::Task {
                project,
                feature,
                id,
            } => ClientMsg::Task {
                project,
                checkout: checkout.expect("a task brief belongs to the selected checkout"),
                feature,
                action: argus_protocol::TaskAction::SetBody { id, body },
            },
            BriefTarget::Feature { project, slug } => ClientMsg::Feature {
                project,
                checkout: checkout.expect("a feature brief belongs to the selected checkout"),
                action: argus_protocol::FeatureAction::Rewrite { slug, body },
                request_id: 0,
            },
            BriefTarget::Decision {
                project,
                feature,
                id,
            } => ClientMsg::ChangeDecision {
                project,
                checkout: checkout.expect("a decision belongs to the selected checkout"),
                feature,
                change: argus_protocol::DecisionChange::Rewrite {
                    id,
                    text: argus_protocol::DecisionText::from_markdown(&body),
                },
            },
            BriefTarget::Diagram {
                project,
                feature,
                id,
            } => ClientMsg::SequenceDiagram {
                project,
                checkout: checkout.expect("a diagram belongs to the selected checkout"),
                feature,
                action: argus_protocol::DiagramAction::Rewrite {
                    id,
                    write: argus_protocol::DiagramWrite {
                        title: view.title.clone(),
                        body,
                    },
                },
            },
            BriefTarget::TrackerMap { project, draft: false } => ClientMsg::TrackerMap {
                project,
                checkout: checkout.expect("a tracker map belongs to the selected checkout's board"),
                change: argus_protocol::MapChange::Write(body),
            },
            // A note is sent whole by its own key, and an agent's draft is
            // approved by one; neither is the brief overlay's to save.
            BriefTarget::Capture { .. } | BriefTarget::TrackerMap { draft: true, .. } => return,
        });
    }

    /// Works from any column that still implies a checkout. `R` with a
    /// review up reads it again — the commit it is of, or the working
    /// changes of its checkout — and otherwise opens the selected one's.
    pub(in crate::app) fn open_review(&mut self) {
        self.note_review_return();
        let current = self.current_checkout().map(|c| c.id);
        let next = &mut self.next_review_request;
        if let Some(session) = self
            .review
            .as_mut()
            .filter(|s| s.commit().is_some() || current.is_none_or(|c| c == s.checkout()))
        {
            let commit = session.commit().is_some();
            let msgs = session.refresh(|| next_id(next));
            self.ask_review(msgs, if commit { "loading commit…" } else { "loading diff…" });
            return;
        }
        match current {
            Some(checkout) => self.start_review(checkout),
            None => self.report("nothing to review"),
        }
    }

    /// Asks for both sides of `checkout`'s working changes, opening on the
    /// side last chosen.
    pub(in crate::app) fn start_review(&mut self, checkout: CheckoutId) {
        let next = &mut self.next_review_request;
        let (session, msgs) =
            ReviewSession::working(checkout, self.review_base, self.review_split, false, || next_id(next));
        self.review = Some(session);
        self.history = None;
        self.history_request.clear();
        self.ask_review(msgs, "loading diff…");
    }

    /// An overlay already up keeps the place it was opened from.
    fn note_review_return(&mut self) {
        if self.focus == Focus::Review {
            return;
        }
        self.review_return = if self.focus == Focus::View {
            Focus::View
        } else {
            Focus::Checkouts
        };
    }

    fn ask_review(&mut self, msgs: Vec<ClientMsg>, loading: &str) {
        self.report(loading);
        for msg in msgs {
            let _ = self.out.send(msg);
        }
    }

    /// `H` from the tree, or from a review that is already up.
    pub(in crate::app) fn open_history(&mut self) {
        self.note_review_return();
        let Some(id) = self
            .current_checkout()
            .map(|c| c.id)
            .or_else(|| self.history.as_ref().map(|h| h.checkout))
            .or_else(|| self.review.as_ref().map(|s| s.checkout()))
            .or_else(|| self.git_checkout())
        else {
            self.report("nothing to show history for");
            return;
        };
        let request_id = self.history_request.request(id);
        self.report("loading history…");
        let _ = self.out.send(ClientMsg::ListCommits {
            request_id,
            checkout: id,
        });
    }

    /// `l`/Enter in the history overlay. A folded commit unfolds to the
    /// files it touched — fetched then, not when the list was built — and
    /// anything already unfolded opens as a review.
    pub(in crate::app) fn drill_into_history(&mut self) {
        let Some(view) = &mut self.history else {
            return;
        };
        let checkout = view.checkout;
        match view.drill() {
            Drill::Open => self.open_selected_commit(),
            Drill::Shown => {}
            Drill::Fetch(commit) => {
                self.report("loading files…");
                let _ = self
                    .out
                    .send(ClientMsg::ListCommitFiles { checkout, commit });
            }
        }
    }

    pub(in crate::app) fn open_selected_commit(&mut self) {
        let Some(view) = &self.history else {
            return;
        };
        let Some(oid) = view.selected_oid().map(str::to_string) else {
            return;
        };
        let file = view.selected_file().map(|f| f.path.clone());
        self.open_commit_review(oid, file);
    }

    pub(in crate::app) fn open_commit_review(&mut self, oid: String, file: Option<String>) {
        let Some(checkout) = self
            .history
            .as_ref()
            .map(|h| h.checkout)
            .or_else(|| self.review.as_ref().map(|s| s.checkout()))
        else {
            return;
        };
        let id = next_id(&mut self.next_review_request);
        let (session, msg) = ReviewSession::of_commit(checkout, oid, file, self.review_split, id);
        self.review = Some(session);
        self.ask_review(vec![msg], "loading commit…");
    }

    /// `h`/Left: back to history when a commit review was opened from it.
    /// Escape/`q` use [`Self::close_overlay`] and drop both.
    pub(in crate::app) fn close_review(&mut self) {
        let from_history = self.review.as_ref().is_some_and(|s| s.commit().is_some())
            && self.history.is_some();
        self.review = None;
        if from_history {
            self.overlay = Some(Overlay::History);
            self.focus = Focus::Review;
            return;
        }
        self.history = None;
        self.history_request.clear();
        self.overlay = None;
        self.pane_fullscreen = false;
        self.focus = self.review_return;
    }

    /// The comment typed at the open review, to `to` or — with nobody
    /// chosen — to the one live agent, or a choice between several.
    pub(in crate::app) fn send_comment(&mut self, body: String, to: Option<PaneId>) {
        let Some(session) = &self.review else {
            self.report("review is no longer open");
            return;
        };
        let checkout = self.tree.checkout(session.checkout());
        // Named nothing until its reviews and its makes share one count
        // (`connection::launch::greet`): an id from either could name the other.
        match agent_comment::comment(session.anchor(), checkout, to, &body, 0) {
            Ok(Comment::Send(msg)) => {
                let _ = self.out.send(*msg);
                self.report("saving comment…");
            }
            Ok(Comment::Choose(panes)) => {
                let labels = panes
                    .iter()
                    .filter_map(|id| self.tree.panes().find(|p| p.id == *id))
                    .map(|p| {
                        let template = p.template.as_deref().unwrap_or("agent");
                        format!("{}  {}  #{}", p.title, template, p.id.0)
                    })
                    .collect();
                self.picker = Some(Picker::new(
                    PickerKind::ReviewRecipient { panes, body },
                    "send comment to",
                    labels,
                    0,
                ));
            }
            Err(refused) => self.report(refused),
        }
    }

    /// Where an editor opens for this app's host. An external editor is
    /// one the daemon launches on its own machine, so for a host elsewhere
    /// it would open where nobody is looking; there it comes up in a
    /// floating pane instead, running on that machine and drawn here.
    pub(in crate::app) fn editor_mode(&self) -> crate::settings::EditorMode {
        use crate::settings::EditorMode;
        match (self.settings.editor, &self.host) {
            (EditorMode::External, Some(_)) => EditorMode::Overlay,
            (mode, _) => mode,
        }
    }

    /// The configured editor command, or `None` to leave it to the daemon —
    /// always on another machine, where this one's editor may not exist and
    /// the daemon there knows its own.
    pub(in crate::app) fn editor_command(&self) -> Option<String> {
        if self.host.is_some() {
            return None;
        }
        let cmd = self.settings.editor_cmd.trim();
        (!cmd.is_empty()).then(|| cmd.to_string())
    }

    /// Names an editor request, and says where the editor it spawns should
    /// land. Zero — no answer wanted — for an external one: it has no pane
    /// to take the keys to.
    pub(in crate::app) fn editor_request(&mut self) -> u64 {
        match self.editor_mode() {
            crate::settings::EditorMode::External => 0,
            crate::settings::EditorMode::Column => {
                self.awaited.ask(Then::FocusPane { floating: false })
            }
            crate::settings::EditorMode::Overlay => {
                self.awaited.ask(Then::FocusPane { floating: true })
            }
        }
    }

    /// Moves the repository's primary checkout onto the selected branch row.
    /// The daemon refuses this when that checkout is dirty and says so; the
    /// answer there is `n`, which gives the branch a worktree instead.
    pub(in crate::app) fn switch_primary_to_selected_branch(&mut self) {
        if self.current_branch_row().is_some() {
            self.send_row_verb(Verb::Switch);
        }
    }

    /// Starts a shell in the selected checkout. On a branch row, which has
    /// no directory of its own, the branch is given a worktree first and
    /// the shell starts there once the daemon says which checkout it made —
    /// as `a` does for an agent.
    /// A shell where the cursor is: in its checkout, or, on a console's
    /// row, another console.
    pub(in crate::app) fn spawn_shell(&mut self) {
        if self.current_console().is_some() {
            self.spawn_console();
            return;
        }
        if let Some(checkout) = self.current_checkout().map(|c| c.id) {
            let request_id = self.awaited.ask(Then::FocusPane { floating: false });
            let _ = self.out.send(ClientMsg::SpawnShell {
                checkout,
                request_id,
            });
            return;
        }
        self.branch_worktree_then(Then::SpawnShell);
    }

    /// Gives the selected branch row a worktree, and does `then` in it once
    /// the daemon says which checkout it made.
    pub(in crate::app) fn branch_worktree_then(&mut self, then: Then) {
        if self.current_branch_row().is_none() {
            return;
        }
        // Refused before it is named, so a request nobody sends waits for
        // no answer.
        if let Some(Err(refusal)) = self.row_verb(Verb::NewWorktree { name: None, request_id: 0 }) {
            self.report(refusal);
            return;
        }
        let request_id = self.awaited.ask(then);
        if self.send_row_verb(Verb::NewWorktree { name: None, request_id }) {
            self.report("creating worktree…");
        }
    }

    pub(in crate::app) fn kill_selected(&mut self) {
        if self.focus == Focus::Panes {
            self.close_current();
        }
    }

    /// Closes whatever pane is currently shown in the live view — reachable
    /// both from the open-agents list (`x`) and, via the leader chord, from
    /// inside the pane itself (`<leader>x`), since a bare `x` there is just
    /// a character typed at the child.
    pub(in crate::app) fn close_current(&mut self) {
        if let Some(pane) = self.current_pane() {
            let _ = self.out.send(ClientMsg::Kill { pane: pane.id });
            // Land back in the open-agents list rather than staying "in"
            // PaneContent — the pane at this index may now be a different
            // one once the removal lands, and typing should never go to a
            // pane the user didn't choose.
            self.pane_fullscreen = false;
            self.focus = Focus::Panes;
        }
    }
}

/// Hands out `next` and moves it on, never to zero.
pub(in crate::app) fn next_id(next: &mut u64) -> u64 {
    let id = *next;
    *next = id.wrapping_add(1).max(1);
    id
}
