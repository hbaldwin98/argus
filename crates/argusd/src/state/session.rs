//! Bringing a workspace back: what was running when the daemon stopped,
//! and asking each agent CLI to reopen the conversation it had.
//!
//! This is relaunch, not process reattachment — no PID outlives a run — so
//! everything here is about identity rather than continuity: which template
//! to start, which harness wrote the conversation being claimed, and which
//! of several panes in one checkout is allowed to claim it. Recording is
//! the same question backwards, which is why both live here.

use super::*;

/// The arguments an agent pane starts with, and whether that command asks
/// the CLI to continue a conversation rather than open a new one.
///
/// Restoring a pane means restoring what was in it, so the harness's resume
/// arguments go on the end of the template's own command — the user's flags
/// still apply to the conversation being continued. A harness Argus cannot
/// ask to resume leaves the command exactly as it was, and the pane is not
/// treated as resumed: there is nothing for a failure to fall back from.
pub(super) fn agent_args(
    configured: &[String],
    resume: &[String],
    resume_id: &[String],
    start: Start,
    session_id: Option<&str>,
) -> (Vec<String>, bool) {
    let mut args = configured.to_vec();
    if start == Start::Fresh {
        return (args, false);
    }
    if let Some(session_id) = session_id {
        if resume_id.is_empty() {
            return (args, false);
        }
        args.extend(
            resume_id
                .iter()
                .map(|arg| arg.replace("{session_id}", session_id)),
        );
    } else {
        if resume.is_empty() {
            return (args, false);
        }
        args.extend(resume.iter().cloned());
    }
    (args, true)
}
/// Whether a resumed agent's exit reads as "there was no conversation to
/// continue" rather than as an agent the user is done with.
///
/// A clean exit is always the user's: every one of these CLIs exits 0 when
/// you leave it, and refuses with a status when it cannot start.
pub(super) fn nothing_to_resume(code: Option<i32>, ran_for: Duration) -> bool {
    code != Some(0) && ran_for < RESUME_GRACE
}
/// A pane started with its harness's resume arguments, and what to start
/// instead if that turns out to have been a lie.
///
/// The CLIs answer "there is nothing to continue" by refusing to start:
/// `claude --continue` in a checkout that has never held a conversation
/// prints a line and exits. Restoring a pane must not leave a dead row
/// where an agent should be, so an immediate failure is taken as that
/// answer and the pane comes back as a plain new agent.
pub(super) struct Resumed {
    pub(super) checkout: CheckoutId,
    pub(super) template: String,
    pub(super) at: std::time::Instant,
}
/// How long after a resumed spawn a failure still reads as "there was
/// nothing to resume" rather than as the user quitting.
///
/// Long enough for a node CLI to start and give up, short enough that
/// quitting an agent you did not want back — which restore has just put in
/// front of you — is not misread as one. A false positive costs a fresh
/// agent pane, which is exactly what restore did before it could resume at
/// all; a false negative costs a dead row.
pub(super) const RESUME_GRACE: Duration = Duration::from_secs(5);

impl Daemon {
    /// What is running, in a form that survives ids being reissued.
    pub(super) fn session_panes(&self) -> Vec<crate::store::SessionPane> {
        let inner = self.inner.lock().unwrap();
        checkouts(&inner.projects)
            .flat_map(|c| {
                c.panes
                    .iter()
                    .filter(|pane| !matches!(pane.status, PaneStatus::Exited { .. }))
                    .map(|pane| crate::store::SessionPane {
                        checkout_path: c.path.clone(),
                        kind: pane.kind,
                        title: pane.title.clone(),
                        template: pane.template.clone(),
                        status: pane.status,
                        note: pane.note.clone(),
                        harness_session_id: pane.harness_session_id.clone(),
                        harness: pane.harness.clone(),
                    })
            })
            .collect()
    }

    pub(super) fn record_session(&self) {
        // Half a restore is not a session worth remembering.
        if self.restoring.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if let Err(e) = self.store.save_panes(&self.session_panes()) {
            tracing::warn!("could not record the session: {e}");
        }
    }

    /// Starts again whatever was running when the daemon last stopped, and
    /// asks each agent CLI to reopen the conversation it had.
    ///
    /// Failures are per pane and never fatal: a template that has since
    /// stopped working should cost you that pane, not the whole session.
    pub fn restore_session(self: &Arc<Self>) {
        let saved = match self.store.panes() {
            Ok(panes) => panes,
            Err(e) => {
                tracing::warn!("could not read what was running: {e}");
                return;
            }
        };
        if saved.is_empty() {
            return;
        }
        // Only primary checkouts come from the config; a worktree is
        // discovered from git by a poll that has not run yet. Without this,
        // every pane in a worktree looks like a pane whose checkout is
        // gone, and is dropped.
        self.reconcile_worktrees();
        let known = self.checkout_paths();
        let wanted: Vec<crate::store::SessionPane> =
            crate::store::restorable(&saved, &known).cloned().collect();
        if wanted.is_empty() {
            return;
        }

        self.restoring
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let mut restored = 0usize;
        let mut claimed: Vec<(PathBuf, String)> = Vec::new();
        for pane in &wanted {
            let Some(checkout) = self.checkout_at(&pane.checkout_path) else {
                continue;
            };
            let result = match pane.kind {
                PaneKind::Agent => {
                    let session_id = pane
                        .harness_session_id
                        .as_deref()
                        .and_then(valid_session_id);
                    // Exact IDs are independent. Only old records need to
                    // claim a checkout's broad "last conversation" resume.
                    let start = if session_id.is_some() {
                        Start::Resuming
                    } else {
                        // The harness the pane recorded, since that is who
                        // wrote the conversation being claimed. Only a file
                        // old enough not to have it falls back to asking
                        // what the template names now.
                        let harness = pane.harness.clone().or_else(|| {
                            let template = self
                                .templates
                                .lock()
                                .unwrap()
                                .iter()
                                .find(|template| template.name == pane.template())
                                .cloned();
                            template
                                .as_ref()
                                .map(|template| self.harness_for(template).name.to_string())
                        });
                        let key = harness.map(|harness| (pane.checkout_path.clone(), harness));
                        if key.as_ref().is_some_and(|key| claimed.contains(key)) {
                            Start::Fresh
                        } else {
                            if let Some(key) = key {
                                claimed.push(key);
                            }
                            Start::Resuming
                        }
                    };
                    self.start_agent(checkout, pane.template(), start, session_id)
                }
                _ => self.spawn_shell(checkout),
            };
            match result {
                Ok(id) => {
                    self.restore_pane_metadata(id, pane);
                    restored += 1;
                }
                Err(e) => tracing::warn!(
                    "could not restore {} in {}: {e}",
                    pane.title,
                    pane.checkout_path.display()
                ),
            }
        }
        self.restoring
            .store(false, std::sync::atomic::Ordering::Relaxed);

        tracing::info!("restored {restored} of {} panes", wanted.len());
        self.broadcast_tree();
    }

    pub(super) fn restore_pane_metadata(&self, id: PaneId, saved: &crate::store::SessionPane) {
        let mut inner = self.inner.lock().unwrap();
        let Some(pane) = find_pane(&mut inner.projects, id) else {
            return;
        };
        if !pane.restore_status_reported && !matches!(pane.status, PaneStatus::Exited { .. }) {
            pane.status = saved.status;
            pane.note = saved.note.clone();
        }
        if saved.kind == PaneKind::Agent
            && saved.title != saved.template()
            && !pane.restore_title_reported
        {
            pane.title = saved.title.clone();
        }
        pane.restore_status_reported = false;
        pane.restore_title_reported = false;
    }
}
