//! What an agent says about itself, and what it is told.
//!
//! Every report carries the session it came from, because a CLI spawned
//! inside a pane inherits that pane's hook URL and token and cannot be
//! stopped from calling home. The agent Argus started stays the authority
//! on what the row says; everything else is listed underneath it.

use argus_protocol::{ReviewAnchor, ReviewComment, MAX_REVIEW_COMMENT_BYTES};

use super::*;

/// The durable identity of the place an agent is running.
pub(super) struct AgentScope {
    pub project_name: String,
    pub checkout_path: std::path::PathBuf,
    /// Whether this project lets an agent write to its checkout's note.
    /// Resolved here with the rest of the scope because the policy belongs
    /// to the project the pane was found under, and this walk is what finds
    /// it.
    pub todos_allowed: bool,
}

/// Whether a hook's report should be dropped rather than applied, for
/// either of two reasons. The pane has already exited, and nothing said
/// afterwards — a `Stop` racing a crash, say — should resurrect its row. Or
/// `Idle` is arriving over a state that is still holding something for the
/// operator, where "my turn ended" is not news that clears "blocked on the
/// db password".
pub(super) fn is_stale_report(current: PaneStatus, reported: PaneStatus) -> bool {
    matches!(current, PaneStatus::Exited { .. })
        || reported == PaneStatus::Idle
            && matches!(
                current,
                PaneStatus::Waiting
                    | PaneStatus::NeedsReview
                    | PaneStatus::Done
                    | PaneStatus::Failed
            )
}

/// A title has to survive being drawn in a one-line row, and arrives from
/// a model, so it is flattened to one line and cut to a column's width.
pub(super) fn clean_title(raw: &str) -> String {
    const MAX: usize = 48;
    let flat = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(MAX) {
        Some((i, _)) => format!("{}…", flat[..i].trim_end()),
        None => flat,
    }
}

impl Daemon {
    /// Persists first, then best-effort notifies the selected agent through
    /// its terminal. A failed notification does not erase durable feedback.
    pub fn submit_review_comment(
        &self,
        checkout_id: CheckoutId,
        recipient: PaneId,
        anchor: ReviewAnchor,
        body: String,
    ) -> anyhow::Result<(u64, bool)> {
        let body = body.trim().to_string();
        if body.is_empty() {
            anyhow::bail!("review comment is empty");
        }
        if body.len() > MAX_REVIEW_COMMENT_BYTES {
            anyhow::bail!("review comment exceeds {MAX_REVIEW_COMMENT_BYTES} bytes");
        }

        let (checkout_path, input) = {
            let inner = self.inner.lock().unwrap();
            let checkout = find_checkout_ref(&inner.projects, checkout_id)
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?;
            let pane = checkout
                .panes
                .iter()
                .find(|pane| pane.id == recipient)
                .ok_or_else(|| anyhow::anyhow!("recipient is not in that checkout"))?;
            if pane.kind != PaneKind::Agent || matches!(pane.status, PaneStatus::Exited { .. }) {
                anyhow::bail!("recipient must be a live agent pane");
            }
            (checkout.path.clone(), pane.runtime.input())
        };

        let comment = self
            .store
            .add_review_comment(&checkout_path, anchor, body)?;
        let mut notification = comment.anchor.notification(&comment.body).into_bytes();
        notification.push(b'\r');
        let delivered = input.write(&notification).is_ok();
        Ok((comment.id, delivered))
    }

    /// Comments visible to the checkout containing this live agent. Pane ids
    /// are runtime-only, so durable access is deliberately checkout-scoped.
    pub fn review_comments_for_agent(&self, pane_id: PaneId) -> anyhow::Result<Vec<ReviewComment>> {
        let scope = self.agent_scope(pane_id)?;
        self.store.review_comments(&scope.checkout_path)
    }

    /// Where a live agent pane sits: the project that owns it and the
    /// checkout it is working in, both named the way the store files them
    /// rather than by the ids a restart throws away.
    ///
    /// Every durable read an agent is allowed to make resolves through
    /// here, so the rule that a caller must be a live agent pane is written
    /// once. A shell pane, an exited one, or an id from another Argus gets
    /// the same refusal whatever it asked for.
    pub(super) fn agent_scope(&self, pane_id: PaneId) -> anyhow::Result<AgentScope> {
        let inner = self.inner.lock().unwrap();
        for project in &inner.projects {
            for repository in &project.repositories {
                for checkout in &repository.checkouts {
                    let Some(pane) = checkout.panes.iter().find(|pane| pane.id == pane_id) else {
                        continue;
                    };
                    if pane.kind != PaneKind::Agent
                        || matches!(pane.status, PaneStatus::Exited { .. })
                    {
                        anyhow::bail!("source must be a live agent pane");
                    }
                    return Ok(AgentScope {
                        project_name: project.name.clone(),
                        checkout_path: checkout.path.clone(),
                        todos_allowed: project.agent_todos,
                    });
                }
            }
        }
        anyhow::bail!("no such source pane")
    }

    /// Applies a hook report to whichever agent sent it.
    ///
    /// Every report a harness makes carries the session it came from, so a
    /// CLI spawned inside a pane — which inherits the pane's hook URL and
    /// token and cannot be stopped from calling home — lands in that pane's
    /// child list instead of overwriting the row. The agent Argus started
    /// stays the authority on what the pane says.
    pub(super) fn report_pane_status(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
        status: PaneStatus,
        note: Option<String>,
    ) {
        match self.child_of(pane, reporter) {
            Some(session) => self.set_child_status(pane, &session, status, note),
            None => self.set_pane_hook_status(pane, status, note),
        }
    }

    pub(super) fn report_pane_title(&self, pane: PaneId, reporter: Option<&str>, title: &str) {
        match self.child_of(pane, reporter) {
            Some(session) => self.set_child_label(pane, &session, title),
            None => self.set_pane_title(pane, title),
        }
    }

    /// The reporting session, when it is not the one that owns the pane.
    /// A report with no session at all is the pane's own: only a harness
    /// event carries one, and `argus-hook status` typed by hand has none.
    pub(super) fn child_of(&self, pane: PaneId, reporter: Option<&str>) -> Option<String> {
        let reporter = reporter?;
        // Match insertion's lock order so ownership cannot change between
        // checking the pre-spawn mailbox and checking the pane tree.
        let starting = self.starting_agents.lock().unwrap();
        let inner = self.inner.lock().unwrap();
        let owner = find_pane_ref(&inner.projects, pane)
            .and_then(|pane| pane.harness_session_id.as_deref())
            .or_else(|| {
                starting
                    .get(&pane)
                    .and_then(|pending| pending.harness_session_id.as_deref())
            })?;
        (owner != reporter).then(|| reporter.to_string())
    }

    fn with_child(&self, pane: PaneId, session: &str, edit: impl FnOnce(&mut ChildAgent)) {
        {
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            let children = match find_pane(&mut inner.projects, pane) {
                Some(p) if matches!(p.status, PaneStatus::Exited { .. }) => return,
                Some(p) => &mut p.children,
                None => match starting.get_mut(&pane) {
                    Some(pending) => &mut pending.children,
                    None => return,
                },
            };
            match children.iter_mut().find(|c| c.session_id == session) {
                Some(child) => {
                    child.at = std::time::Instant::now();
                    edit(child)
                }
                None => {
                    let mut child = ChildAgent {
                        session_id: session.to_string(),
                        label: None,
                        status: PaneStatus::Working,
                        note: None,
                        at: std::time::Instant::now(),
                    };
                    edit(&mut child);
                    children.push(child);
                    if children.len() > MAX_CHILDREN {
                        children.remove(0);
                    }
                }
            }
            // A child that has gone idle is no longer something running
            // under this row, so it stops being listed under it.
            children.retain(|c| !matches!(c.status, PaneStatus::Idle | PaneStatus::Exited { .. }));
        }
        self.broadcast_tree();
    }

    fn set_child_status(
        &self,
        pane: PaneId,
        session: &str,
        status: PaneStatus,
        note: Option<String>,
    ) {
        let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
        self.with_child(pane, session, |child| {
            child.status = status;
            child.note = note;
        });
    }

    fn set_child_label(&self, pane: PaneId, session: &str, title: &str) {
        let label = clean_title(title);
        if label.is_empty() {
            return;
        }
        self.with_child(pane, session, |child| child.label = Some(label));
    }

    /// Forgets children that have gone quiet. A child says so when it
    /// finishes and is cleared with its parent's turn either way, so this
    /// only catches the one that was killed, crashed, or lost its harness
    /// mid-turn — otherwise its row would sit there claiming to be working
    /// for as long as the parent kept going.
    pub(super) fn drop_silent_children(&self) {
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let mut changed = false;
            for p in panes_mut(&mut inner.projects) {
                let before = p.children.len();
                p.children.retain(|c| c.at.elapsed() < CHILD_SILENCE);
                changed |= p.children.len() != before;
            }
            changed
        };
        if changed {
            self.broadcast_tree();
        }
    }

    pub(super) fn set_pane_hook_status(
        &self,
        pane: PaneId,
        status: PaneStatus,
        note: Option<String>,
    ) {
        let changed = {
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if !is_stale_report(p.status, status) => {
                    if self.restoring.load(std::sync::atomic::Ordering::Relaxed) {
                        p.restore_status_reported = true;
                    }
                    // A note explains one state; the report that leaves that
                    // state takes it away with it, so a stale "waiting for
                    // the db password" can't sit under a working row.
                    let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
                    let mut changed = p.status != status || p.note != note;
                    p.status = status;
                    p.note = note;
                    // The turn that spawned them is over, so anything still
                    // listed under this row has finished without saying so.
                    // A background agent outliving the turn is not lost by
                    // this: its next report lists it again.
                    if status == PaneStatus::Idle && !p.children.is_empty() {
                        p.children.clear();
                        changed = true;
                    }
                    changed
                }
                Some(_) => false,
                None => match starting.get_mut(&pane) {
                    Some(pending)
                        if !is_stale_report(
                            pending
                                .status
                                .as_ref()
                                .map_or(PaneStatus::Idle, |(status, _)| *status),
                            status,
                        ) =>
                    {
                        let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
                        let mut changed = pending.status.as_ref() != Some(&(status, note.clone()));
                        pending.status = Some((status, note));
                        if status == PaneStatus::Idle && !pending.children.is_empty() {
                            pending.children.clear();
                            changed = true;
                        }
                        changed
                    }
                    _ => false,
                },
            }
        };
        if changed {
            self.broadcast_tree();
        }
    }

    /// Renames a pane. The daemon does this from a prompt-submit event so
    /// a column of agents is not four rows all called "claude"; the agent
    /// can still refine it with `argus-hook title`. Ignored for a pane that
    /// has exited — a rename racing a crash shouldn't relabel a dead row.
    pub(super) fn set_pane_title(&self, pane: PaneId, title: &str) {
        let title = clean_title(title);
        if title.is_empty() {
            return;
        }
        let changed = {
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if !matches!(p.status, PaneStatus::Exited { .. }) => {
                    if self.restoring.load(std::sync::atomic::Ordering::Relaxed) {
                        p.restore_title_reported = true;
                    }
                    if p.title != title {
                        p.title = title;
                        true
                    } else {
                        false
                    }
                }
                Some(_) => false,
                None => match starting.get_mut(&pane) {
                    Some(pending) if pending.title.as_deref() != Some(&title) => {
                        pending.title = Some(title);
                        true
                    }
                    _ => false,
                },
            }
        };
        if changed {
            self.broadcast_tree();
        }
    }

    /// Records the conversation Argus would resume this pane with.
    ///
    /// A claim from a session that is not the pane's current owner is only
    /// honoured while the pane is not working: the pane's own agent starting
    /// over (`/clear`, a resume) is idle at that moment, whereas a CLI
    /// spawned from inside a turn arrives mid-work. The latter is listed as
    /// a child instead, which is what keeps a nested agent from stealing the
    /// identity the row resumes from.
    pub(super) fn set_pane_session_id(&self, pane: PaneId, raw: &str) {
        let Some(session_id) = valid_session_id(raw) else {
            return;
        };
        if self.child_of(pane, Some(&session_id)).is_some() {
            let working = {
                let starting = self.starting_agents.lock().unwrap();
                let inner = self.inner.lock().unwrap();
                find_pane_ref(&inner.projects, pane)
                    .map(|p| p.status)
                    .or_else(|| {
                        starting
                            .get(&pane)
                            .and_then(|pending| pending.status.as_ref().map(|(status, _)| *status))
                    })
                    == Some(PaneStatus::Working)
            };
            if working {
                self.with_child(pane, &session_id, |_| {});
                return;
            }
        }
        let changed = {
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p)
                    if !matches!(p.status, PaneStatus::Exited { .. })
                        && p.harness_session_id.as_deref() != Some(&session_id) =>
                {
                    p.harness_session_id = Some(session_id);
                    p.children.clear();
                    true
                }
                Some(_) => false,
                None => match starting.get_mut(&pane) {
                    Some(pending) if pending.harness_session_id.as_deref() != Some(&session_id) => {
                        pending.harness_session_id = Some(session_id);
                        pending.children.clear();
                        true
                    }
                    _ => false,
                },
            }
        };
        if changed {
            self.broadcast_tree();
        }
    }

    /// Moves a live agent row to the known checkout it has started working
    /// in. The PTY stays intact; this changes Argus's affiliation, not the
    /// child process's working directory. The reporting command runs in the
    /// destination directory, which is the evidence that the agent moved.
    pub(super) fn move_agent_to_checkout(
        &self,
        pane: PaneId,
        destination: &std::path::Path,
    ) -> anyhow::Result<()> {
        let (source_path, target_path, template, source_has_agent) = {
            let mut inner = self.inner.lock().unwrap();
            let (project_index, source_repository_index, source_index, pane_index) = inner
                .projects
                .iter()
                .enumerate()
                .find_map(|(project_index, project)| {
                    project.repositories.iter().enumerate().find_map(
                        |(repository_index, repository)| {
                            repository.checkouts.iter().enumerate().find_map(
                                |(checkout_index, checkout)| {
                                    checkout
                                        .panes
                                        .iter()
                                        .position(|candidate| candidate.id == pane)
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
                .ok_or_else(|| anyhow::anyhow!("no such pane"))?;

            let project = &mut inner.projects[project_index];
            let (target_repository_index, target_index) = project
                .repositories
                .iter()
                .enumerate()
                .find_map(|(repository_index, repository)| {
                    repository
                        .checkouts
                        .iter()
                        .position(|checkout| same_path(&checkout.path, destination))
                        .map(|checkout_index| (repository_index, checkout_index))
                })
                .ok_or_else(|| anyhow::anyhow!("destination is not a checkout in this project"))?;
            if source_repository_index == target_repository_index && source_index == target_index {
                return Ok(());
            }

            let moving = &project.repositories[source_repository_index].checkouts[source_index]
                .panes[pane_index];
            if moving.kind != PaneKind::Agent {
                anyhow::bail!("only agent panes can change checkout affiliation");
            }
            if matches!(moving.status, PaneStatus::Exited { .. }) {
                anyhow::bail!("an exited pane cannot change checkout affiliation");
            }

            let source_path = project.repositories[source_repository_index].checkouts[source_index]
                .path
                .clone();
            let target_path = project.repositories[target_repository_index].checkouts[target_index]
                .path
                .clone();
            let moving = project.repositories[source_repository_index].checkouts[source_index]
                .panes
                .remove(pane_index);
            let template = moving.template.clone();
            project.repositories[target_repository_index].checkouts[target_index]
                .panes
                .push(moving);
            let source_has_agent = project.repositories[source_repository_index].checkouts
                [source_index]
                .panes
                .iter()
                .any(|candidate| candidate.kind == PaneKind::Agent);
            (source_path, target_path, template, source_has_agent)
        };

        if !source_has_agent {
            for harness in &self.harnesses {
                if let Err(error) = harness.uninstall(&source_path) {
                    tracing::warn!(
                        "failed to clear {} hooks in {}: {error}",
                        harness.name,
                        source_path.display()
                    );
                }
            }
        }

        let found = template.as_deref().and_then(|name| {
            self.templates
                .lock()
                .unwrap()
                .iter()
                .find(|template| template.name == name)
                .cloned()
        });
        if let Some(template) = found.as_ref() {
            let harness = self.harness_for(template);
            let port = self.hook_port.load(std::sync::atomic::Ordering::Relaxed);
            if port != 0 {
                if let Err(error) = harness.install(&target_path, pane, port, &self.hook_token) {
                    tracing::warn!(
                        "failed to install {} hooks in {}: {error}",
                        harness.name,
                        target_path.display()
                    );
                }
            }
        }

        self.broadcast_tree();
        Ok(())
    }
}
