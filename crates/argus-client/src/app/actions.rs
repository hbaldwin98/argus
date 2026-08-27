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


    /// Works from any column that still implies a checkout.
    pub(super) fn open_review(&mut self) {
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
    pub(super) fn send_to_agent(&mut self, message: String) {
        let agents = self.review_agents();
        match agents.as_slice() {
            [] => self.report("no agent running in this checkout"),
            [(pane, _)] => self.send_to_pane(*pane, message),
            _ => {
                self.picker = Some(Picker::new(
                    PickerKind::ReviewRecipient {
                        panes: agents.iter().map(|(pane, _)| *pane).collect(),
                        message,
                    },
                    "send comment to",
                    agents.into_iter().map(|(_, label)| label).collect(),
                    0,
                ));
            }
        }
    }


    pub(super) fn send_to_pane(&mut self, pane: PaneId, message: String) {
        let mut bytes = message.into_bytes();
        // What a terminal actually sends for Enter.
        bytes.push(b'\r');
        let _ = self.out.send(ClientMsg::Input { pane, bytes });
        self.report("comment sent");
    }


    /// Shells and exited agents are skipped: neither can receive a comment.
    fn review_agents(&self) -> Vec<(PaneId, String)> {
        let Some(checkout) = self.review.as_ref().map(|v| v.review.checkout) else {
            return Vec::new();
        };
        self.tree
            .iter()
            .flat_map(|p| p.repositories.iter())
            .flat_map(|r| r.checkouts.iter())
            .find(|c| c.id == checkout)
            .into_iter()
            .flat_map(|c| c.panes.iter())
            .filter(|p| p.kind == PaneKind::Agent && !matches!(p.status, PaneStatus::Exited { .. }))
            .map(|p| {
                let template = p.template.as_deref().unwrap_or("agent");
                (p.id, format!("{}  {}  #{}", p.title, template, p.id.0))
            })
            .collect()
    }


    pub(super) fn is_live_agent(&self, pane: PaneId) -> bool {
        self.tree
            .iter()
            .flat_map(|p| p.repositories.iter())
            .flat_map(|r| r.checkouts.iter())
            .flat_map(|c| c.panes.iter())
            .any(|p| {
                p.id == pane
                    && p.kind == PaneKind::Agent
                    && !matches!(p.status, PaneStatus::Exited { .. })
            })
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


    pub(super) fn close_review(&mut self) {
        self.review = None;
        self.review_wanted = None;
        self.overlay = None;
        self.focus = Focus::Checkouts;
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
            let _ = self.out.send(ClientMsg::SpawnShell { checkout: checkout.id });
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
            self.focus = Focus::Panes;
        }
    }
}
