//! What the operator asks for: panes started and stopped, git fetched and
//! pulled, reviews opened, messages typed at agents.
//!
//! Everything here is a request to the daemon, not a change to the model.
//! The row does not move until the tree comes back saying it did, so a
//! refused action leaves the panel showing what is actually true.

use super::*;

impl App {
    /// Shows or hides the branches that have no checkout. The main branch
    /// keeps its row either way, so the toggle is about the rest of them.
    pub(super) fn toggle_branches(&mut self) {
        self.show_branches = !self.show_branches;
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
    pub(super) fn fetch(&mut self) {
        let Some(checkout) = self.git_checkout() else {
            self.report("no checkout to fetch in");
            return;
        };
        let _ = self.out.send(ClientMsg::Fetch { checkout });
        self.report("fetching…");
    }

    /// `P` moves the selected checkout up to its upstream, fast-forward
    /// only. On a branch row that is the primary checkout, which is the
    /// same checkout every other repository-wide action uses.
    pub(super) fn pull(&mut self) {
        let Some(checkout) = self.git_checkout() else {
            self.report("no checkout to pull into");
            return;
        };
        let _ = self.out.send(ClientMsg::Pull { checkout });
        self.report("pulling…");
    }

    /// The checkout a git command runs in: the selected one, or the
    /// repository's primary when the selection is a branch with no
    /// directory of its own.
    fn git_checkout(&self) -> Option<CheckoutId> {
        self.current_checkout()
            .or_else(|| self.primary_checkout())
            .map(|c| c.id)
    }

    /// Works from any column that still implies a checkout. `R` on a commit
    /// refreshes that commit; otherwise this is the uncommitted sides.
    /// Opens the note for whatever the spine currently has selected: the
    /// checkout when one is in hand, the project otherwise.
    ///
    /// The column you are in decides what the note is about, rather than a
    /// second choice on top of the one you already made by navigating
    /// here. A checkout row with no directory — a bare branch — has no
    /// note, so the project's stands in.
    pub(super) fn open_notes(&mut self) {
        let target = match self.focus {
            Focus::Projects => self.current_project().map(|p| (NoteTarget::Project(p.id), p.name.clone())),
            _ => self
                .current_checkout()
                .map(|c| (NoteTarget::Checkout(c.id), c.name.clone()))
                .or_else(|| {
                    self.current_project()
                        .map(|p| (NoteTarget::Project(p.id), p.name.clone()))
                }),
        };
        let Some((target, title)) = target else {
            self.report("nothing to take notes on");
            return;
        };
        // Opened empty and filled in when the daemon answers, so the
        // window is up on the keypress rather than a round trip later.
        let placeholder = argus_protocol::Note::new(target, String::new());
        self.notes = Some(NoteView::new(&placeholder, title));
        self.overlay = Some(Overlay::Notes);
        let _ = self.out.send(ClientMsg::GetNote { target });
    }

    /// Sends the edited body if it has changed since the last write.
    pub(super) fn save_notes(&mut self) {
        let Some(view) = &mut self.notes else {
            return;
        };
        if !view.dirty {
            return;
        }
        let target = view.target;
        let body = view.body();
        // Marked sent before the answer arrives: the daemon echoes every
        // write back, and a view still flagged dirty would refuse its own
        // echo forever.
        view.saved();
        let _ = self.out.send(ClientMsg::SetNote { target, body });
    }

    pub(super) fn open_review(&mut self) {
        if let Some(oid) = self
            .review
            .as_ref()
            .and_then(|view| view.review.commit.as_ref())
            .map(|commit| commit.oid.clone())
        {
            self.open_commit_review(oid, None);
            return;
        }
        let Some(id) = self
            .current_checkout()
            .map(|c| c.id)
            .or_else(|| self.review.as_ref().map(|v| v.review.checkout))
        else {
            self.report("nothing to review");
            return;
        };
        self.history = None;
        self.history_wanted = None;
        self.request_uncommitted(id);
    }

    pub(super) fn request_uncommitted(&mut self, id: CheckoutId) {
        let request_id = self.next_review_request;
        self.next_review_request = self.next_review_request.wrapping_add(1).max(1);
        self.review_wanted = Some((id, request_id));
        self.report("loading diff…");
        let _ = self.out.send(ClientMsg::Review {
            request_id,
            checkout: id,
            base: self.review_base,
            commit: None,
        });
    }

    /// `H` from the tree, or from a review that is already up.
    pub(super) fn open_history(&mut self) {
        let Some(id) = self
            .current_checkout()
            .map(|c| c.id)
            .or_else(|| self.history.as_ref().map(|h| h.checkout))
            .or_else(|| self.review.as_ref().map(|v| v.review.checkout))
            .or_else(|| self.git_checkout())
        else {
            self.report("nothing to show history for");
            return;
        };
        let request_id = self.next_history_request;
        self.next_history_request = self.next_history_request.wrapping_add(1).max(1);
        self.history_wanted = Some((id, request_id));
        self.report("loading history…");
        let _ = self.out.send(ClientMsg::ListCommits {
            request_id,
            checkout: id,
        });
    }

    /// `l`/Enter in the history overlay. A folded commit unfolds to the
    /// files it touched — fetched then, not when the list was built — and
    /// anything already unfolded opens as a review.
    pub(super) fn drill_into_history(&mut self) {
        let Some(view) = &mut self.history else {
            return;
        };
        let checkout = view.checkout;
        match view.drill() {
            Drill::Open => self.open_selected_commit(),
            Drill::Shown => {}
            Drill::Fetch(commit) => {
                self.report("loading files…");
                let _ = self.out.send(ClientMsg::ListCommitFiles { checkout, commit });
            }
        }
    }

    pub(super) fn open_selected_commit(&mut self) {
        let Some(view) = &self.history else {
            return;
        };
        let Some(oid) = view.selected_oid().map(str::to_string) else {
            return;
        };
        let file = view.selected_file().map(|f| f.path.clone());
        self.open_commit_review(oid, file);
    }

    pub(super) fn open_commit_review(&mut self, oid: String, file: Option<String>) {
        let Some(id) = self
            .history
            .as_ref()
            .map(|h| h.checkout)
            .or_else(|| self.review.as_ref().map(|v| v.review.checkout))
        else {
            return;
        };
        let request_id = self.next_review_request;
        self.next_review_request = self.next_review_request.wrapping_add(1).max(1);
        self.review_wanted = Some((id, request_id));
        self.pending_history_file = file;
        self.report("loading commit…");
        let _ = self.out.send(ClientMsg::Review {
            request_id,
            checkout: id,
            base: ReviewBase::Commit,
            commit: Some(oid),
        });
    }

    /// `h`/Left: back to history when a commit review was opened from it.
    /// Escape/`q` use [`Self::close_overlay`] and drop both.
    pub(super) fn close_review(&mut self) {
        let from_history = self
            .review
            .as_ref()
            .is_some_and(|v| v.review.commit.is_some())
            && self.history.is_some();
        self.review = None;
        self.review_wanted = None;
        self.pending_history_file = None;
        if from_history {
            self.overlay = Some(Overlay::History);
            self.focus = Focus::Review;
            return;
        }
        self.history = None;
        self.history_wanted = None;
        self.overlay = None;
        self.pane_fullscreen = false;
        self.focus = Focus::Checkouts;
    }

    pub(super) fn send_to_agent(&mut self, anchor: ReviewAnchor, body: String) {
        let Some(checkout) = self.review.as_ref().map(|view| view.review.checkout) else {
            self.report("review is no longer open");
            return;
        };
        let agents = self.review_agents();
        match agents.as_slice() {
            [] => self.report("no agent running in this checkout"),
            [(pane, _)] => self.send_review_comment(checkout, *pane, anchor, body),
            _ => {
                self.picker = Some(Picker::new(
                    PickerKind::ReviewRecipient {
                        panes: agents.iter().map(|(pane, _)| *pane).collect(),
                        checkout,
                        anchor,
                        body,
                    },
                    "send comment to",
                    agents.into_iter().map(|(_, label)| label).collect(),
                    0,
                ));
            }
        }
    }

    pub(super) fn send_review_comment(
        &mut self,
        checkout: CheckoutId,
        recipient: PaneId,
        anchor: ReviewAnchor,
        body: String,
    ) {
        let _ = self.out.send(ClientMsg::ReviewComment {
            checkout,
            recipient,
            anchor: Box::new(anchor),
            body,
        });
        self.report("saving comment…");
    }

    /// Shells and exited agents are skipped: neither can receive a comment.
    fn review_agents(&self) -> Vec<(PaneId, String)> {
        let Some(checkout) = self.review.as_ref().map(|v| v.review.checkout) else {
            return Vec::new();
        };
        checkouts_in(&self.tree)
            .find(|c| c.id == checkout)
            .into_iter()
            .flat_map(|c| c.panes.iter())
            .filter(|p| is_live_agent_pane(p))
            .map(|p| {
                let template = p.template.as_deref().unwrap_or("agent");
                (p.id, format!("{}  {}  #{}", p.title, template, p.id.0))
            })
            .collect()
    }

    pub(super) fn is_live_agent(&self, pane: PaneId) -> bool {
        panes_in(&self.tree).any(|p| p.id == pane && is_live_agent_pane(p))
    }

    /// The configured editor command, or `None` to leave it to the daemon.
    pub(super) fn editor_command(&self) -> Option<String> {
        let cmd = self.settings.editor_cmd.trim();
        (!cmd.is_empty()).then(|| cmd.to_string())
    }

    /// Where the editor about to be spawned should land. Nothing at all
    /// for an external one: it has no pane to focus.
    pub(super) fn want_editor(&mut self) {
        match self.settings.editor {
            crate::settings::EditorMode::External => {}
            crate::settings::EditorMode::Column => self.pending_focus_new = true,
            crate::settings::EditorMode::Overlay => {
                self.pending_focus_new = true;
                self.pending_overlay_new = true;
            }
        }
    }

    /// Moves the repository's primary checkout onto the selected branch row.
    /// The daemon refuses this when that checkout is dirty and says so; the
    /// answer there is `n`, which gives the branch a worktree instead.
    pub(super) fn switch_primary_to_selected_branch(&mut self) {
        let Some(branch) = self.current_branch_row().map(str::to_string) else {
            return;
        };
        let Some(checkout) = self.primary_checkout().map(|c| c.id) else {
            self.report("this repository has no primary checkout");
            return;
        };
        let _ = self.out.send(ClientMsg::SwitchBranch { checkout, branch });
    }

    pub(super) fn spawn_shell(&mut self) {
        if let Some(checkout) = self.current_checkout() {
            let _ = self.out.send(ClientMsg::SpawnShell {
                checkout: checkout.id,
            });
            self.pending_focus_new = true;
        }
    }

    pub(super) fn kill_selected(&mut self) {
        if self.focus == Focus::Panes {
            self.close_current();
        }
    }

    /// Closes whatever pane is currently shown in the live view — reachable
    /// both from the open-agents list (`x`) and, via the leader chord, from
    /// inside the pane itself (`<leader>x`), since a bare `x` there is just
    /// a character typed at the child.
    pub(super) fn close_current(&mut self) {
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
