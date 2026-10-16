//! What an agent says about itself, and what it is told.
//!
//! Every report carries the session it came from, because a CLI spawned
//! inside a pane inherits that pane's hook URL and token and cannot be
//! stopped from calling home. The agent Argus started stays the authority
//! on what the row says; everything else is listed underneath it.

use argus_protocol::{ReviewAnchor, ReviewComment, MAX_REVIEW_COMMENT_BYTES};

use super::*;

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

/// A pane, live in the tree or still in the pre-spawn mailbox. Resolving a
/// pane id to whichever of the two currently holds it, then editing it, is
/// one operation with two backing stores; this is where that dispatch lives
/// so callers state only what changed and what "changed" means for them.
enum PaneOrPending<'a> {
    Live(&'a mut Pane),
    Pending(&'a mut PendingStart),
}

impl<'a> PaneOrPending<'a> {
    /// `None` when the pane is in neither store, or has already exited —
    /// nothing said after exit should resurrect a row. Only
    /// [`Daemon::edit_pane`] calls this, so that rule holds for every edit.
    fn find(
        projects: &'a mut [Project],
        starting: &'a mut HashMap<PaneId, PendingStart>,
        pane: PaneId,
    ) -> Option<Self> {
        match find_pane(projects, pane) {
            Some(p) if matches!(p.status, PaneStatus::Exited { .. }) => None,
            Some(p) => Some(Self::Live(p)),
            None => starting.get_mut(&pane).map(Self::Pending),
        }
    }

    fn status(&self) -> PaneStatus {
        match self {
            Self::Live(p) => p.status,
            Self::Pending(p) => p.status.as_ref().map_or(PaneStatus::Idle, |(status, _)| *status),
        }
    }

    fn harness_session_id(&self) -> Option<&str> {
        match self {
            Self::Live(p) => p.harness_session_id.as_deref(),
            Self::Pending(p) => p.harness_session_id.as_deref(),
        }
    }

    /// Whether `reporter` is a session running inside this pane rather than
    /// the one that owns it. A pane that has claimed no session yet has no
    /// children: whoever reports first is its own agent.
    fn is_child(&self, reporter: &str) -> bool {
        self.harness_session_id().is_some_and(|owner| owner != reporter)
    }

    fn children(&mut self) -> &mut Vec<ChildAgent> {
        match self {
            Self::Live(p) => &mut p.children,
            Self::Pending(p) => &mut p.children,
        }
    }

    /// Edits the child running as `session`, listing it if it is new. Always
    /// a change: hearing from a child at all is news about its row.
    fn edit_child(
        &mut self,
        now: std::time::Instant,
        session: &str,
        edit: impl FnOnce(&mut ChildAgent),
    ) -> bool {
        let children = self.children();
        match children.iter_mut().find(|c| c.session_id == session) {
            Some(child) => {
                child.at = now;
                edit(child)
            }
            None => {
                let mut child = ChildAgent {
                    session_id: session.to_string(),
                    label: None,
                    status: PaneStatus::Working,
                    note: None,
                    at: now,
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
        true
    }

    /// The turn that spawned any children is over once a pane goes idle, so
    /// anything still listed under it has finished without saying so. A
    /// background agent outliving the turn is not lost by this: its next
    /// report lists it again.
    fn clear_children_on_idle(&mut self, status: PaneStatus) -> bool {
        let children = self.children();
        if status == PaneStatus::Idle && !children.is_empty() {
            children.clear();
            true
        } else {
            false
        }
    }

    fn set_status(&mut self, status: PaneStatus, note: Option<String>) -> bool {
        let changed = match self {
            Self::Live(p) => {
                if p.status != status {
                    p.status_since = std::time::SystemTime::now();
                }
                let changed = p.status != status || p.note != note;
                p.status = status;
                p.note = note;
                changed
            }
            Self::Pending(p) => {
                let changed = p.status.as_ref() != Some(&(status, note.clone()));
                p.status = Some((status, note));
                changed
            }
        };
        changed | self.clear_children_on_idle(status)
    }

    /// A pane still in the pre-spawn mailbox has had no turn to leave
    /// anything behind.
    fn set_background(&mut self, background: bool) -> bool {
        match self {
            Self::Live(p) => std::mem::replace(&mut p.background, background) != background,
            Self::Pending(_) => false,
        }
    }

    fn set_title(&mut self, title: String) -> bool {
        match self {
            Self::Live(p) => {
                if p.title != title {
                    p.title = title;
                    true
                } else {
                    false
                }
            }
            Self::Pending(p) => {
                if p.title.as_deref() != Some(title.as_str()) {
                    p.title = Some(title);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Claims a conversation. Only `Live` carries telemetry, which is reset
    /// (keeping the model, which usually carries over) because a different
    /// conversation starts from an empty context.
    fn set_session_id(&mut self, session_id: String) -> bool {
        if self.harness_session_id() == Some(session_id.as_str()) {
            return false;
        }
        match self {
            Self::Live(p) => {
                if p.harness_session_id.is_some() {
                    p.telemetry = argus_protocol::AgentTelemetry {
                        model: p.telemetry.model.take(),
                        ..Default::default()
                    };
                }
                p.harness_session_id = Some(session_id);
                p.children.clear();
                p.background = false;
            }
            Self::Pending(p) => {
                p.harness_session_id = Some(session_id);
                p.children.clear();
            }
        }
        true
    }

    fn restore_status_reported(&mut self) {
        if let Self::Live(p) = self {
            p.restore_status_reported = true;
        }
    }

    fn restore_title_reported(&mut self) {
        if let Self::Live(p) = self {
            p.restore_title_reported = true;
        }
    }
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

/// A status report from the pane's own agent, applied unless it is stale.
fn own_status(
    p: &mut PaneOrPending<'_>,
    restoring: bool,
    status: PaneStatus,
    note: Option<String>,
    background: bool,
) -> bool {
    if is_stale_report(p.status(), status) {
        return false;
    }
    if restoring {
        p.restore_status_reported();
    }
    // A note explains one state; the report that leaves that state takes
    // it away with it, so a stale "waiting for the db password" can't sit
    // under a working row.
    let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
    // One edit, so no client is shown the turn ended before it is shown
    // what the turn left running.
    p.set_background(background) | p.set_status(status, note)
}

fn own_title(p: &mut PaneOrPending<'_>, restoring: bool, title: String) -> bool {
    if restoring {
        p.restore_title_reported();
    }
    p.set_title(title)
}

impl Daemon {
    /// Persists first, then best-effort notifies the selected agent: through
    /// its harness's inbox when one is open, else typed into its terminal. A
    /// failed notification does not erase durable feedback.
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

        let checkout_path = {
            let inner = self.inner.lock().unwrap();
            let checkout = find_checkout_ref(&inner.projects, checkout_id)
                .ok_or_else(|| anyhow::anyhow!("no such checkout"))?;
            let pane = checkout
                .panes
                .iter()
                .find(|pane| pane.id == recipient)
                .ok_or_else(|| anyhow::anyhow!("recipient is not in that checkout"))?;
            if !pane.is_running_agent() {
                anyhow::bail!("recipient must be a live agent pane");
            }
            checkout.path.clone()
        };

        let comment = self
            .store
            .add_review_comment(&checkout_path, anchor, body)?;
        let delivered = self.notify(recipient, &comment.anchor.notification(&comment.body));
        Ok((comment.id, delivered))
    }

    /// Comments visible to the checkout containing this live agent. Pane ids
    /// are runtime-only, so durable access is deliberately checkout-scoped.
    pub fn review_comments_for_agent(&self, pane_id: PaneId) -> anyhow::Result<Vec<ReviewComment>> {
        self.store.review_comments(&self.agent_checkout(pane_id)?)
    }

    /// The one way to edit an agent's row, live in the tree or still in
    /// the pre-spawn mailbox. Both locks are taken in insertion's order and
    /// held across finding the pane, deciding and writing, so nothing — an
    /// exit, the pane entering the tree, another report — lands between a
    /// decision and the edit it led to. `edit` says whether anything a
    /// client is shown changed, and the tree goes out only if it did.
    fn edit_pane(&self, pane: PaneId, edit: impl FnOnce(&mut PaneOrPending<'_>) -> bool) -> bool {
        let changed = {
            let mut starting = self.starting_agents.lock().unwrap();
            let mut inner = self.inner.lock().unwrap();
            PaneOrPending::find(&mut inner.projects, &mut starting, pane)
                .is_some_and(|mut p| edit(&mut p))
        };
        if changed {
            self.broadcast_tree();
        }
        changed
    }

    /// Applies a hook report to whichever agent sent it.
    ///
    /// Every report a harness makes carries the session it came from, so a
    /// CLI spawned inside a pane — which inherits the pane's hook URL and
    /// token and cannot be stopped from calling home — lands in that pane's
    /// child list instead of overwriting the row. The agent Argus started
    /// stays the authority on what the pane says.
    ///
    /// `background` is whether the reporter left work running behind its
    /// turn. Only the pane's own agent can say so: what a child leaves
    /// behind is not the pane's.
    pub(crate) fn report_pane_status(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
        status: PaneStatus,
        note: Option<String>,
        background: bool,
    ) {
        let restoring = self.restoring.load(std::sync::atomic::Ordering::Relaxed);
        let now = self.clock.now();
        self.edit_pane(pane, |p| match reporter.filter(|r| p.is_child(r)) {
            Some(session) => {
                let note = note.map(|n| clean_title(&n)).filter(|n| !n.is_empty());
                p.edit_child(now, session, |child| {
                    child.status = status;
                    child.note = note;
                })
            }
            None => own_status(p, restoring, status, note, background),
        });
    }

    /// Merges a telemetry report into the pane's own. A nested agent's
    /// numbers are not the pane's, so a report from a child is dropped.
    ///
    /// The merged record goes out on its own rather than in a tree: this
    /// runs on every tool call, and each connection decides whether its
    /// client can take the one pane or needs the whole tree.
    pub(crate) fn report_pane_telemetry(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
        report: argus_protocol::AgentTelemetry,
    ) {
        if self.child_of(pane, reporter).is_some() || report.is_empty() {
            return;
        }
        // Merged under the tree's order, so no tree read before the merge
        // can be sent after it.
        self.pushes.telemetry(|| {
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if !matches!(p.status, PaneStatus::Exited { .. }) => {
                    p.telemetry.merge(report).then(|| (pane, p.telemetry.clone()))
                }
                _ => None,
            }
        });
    }

    /// Renames a pane, or the child that reported. The daemon does this
    /// from a prompt-submit event so a column of agents is not four rows
    /// all called "claude"; the agent can still refine it with
    /// `argus-hook title`.
    pub(crate) fn report_pane_title(&self, pane: PaneId, reporter: Option<&str>, title: &str) {
        let title = clean_title(title);
        if title.is_empty() {
            return;
        }
        let restoring = self.restoring.load(std::sync::atomic::Ordering::Relaxed);
        let now = self.clock.now();
        self.edit_pane(pane, |p| match reporter.filter(|r| p.is_child(r)) {
            Some(session) => p.edit_child(now, session, |child| child.label = Some(title)),
            None => own_title(p, restoring, title),
        });
    }

    /// A person naming a pane from a client. A console is no agent's, so
    /// it is looked for apart from the tree.
    pub fn rename_pane(&self, pane: PaneId, title: &str) -> anyhow::Result<()> {
        let title = clean_title(title);
        if title.is_empty() {
            anyhow::bail!("a pane needs a name");
        }
        let mut found = false;
        self.edit_pane(pane, |p| {
            found = true;
            p.set_title(title.clone())
        });
        if found {
            return Ok(());
        }
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let console = inner.consoles.iter_mut().find(|p| p.id == pane);
            let console = console.ok_or_else(|| anyhow::anyhow!("no such pane"))?;
            std::mem::replace(&mut console.title, title.clone()) != title
        };
        if changed {
            self.broadcast_tree();
        }
        Ok(())
    }

    /// The reporting session, when it is not the one that owns the pane.
    /// A report with no session at all is the pane's own: only a harness
    /// event carries one, and `argus-hook status` typed by hand has none.
    ///
    /// For a caller that only reads. One that edits the row decides inside
    /// [`Self::edit_pane`] instead, where ownership cannot change under it.
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

    /// Forgets children that have gone quiet. A child says so when it
    /// finishes and is cleared with its parent's turn either way, so this
    /// only catches the one that was killed, crashed, or lost its harness
    /// mid-turn — otherwise its row would sit there claiming to be working
    /// for as long as the parent kept going.
    pub(super) fn drop_silent_children(&self) {
        let now = self.clock.now();
        let changed = {
            let mut inner = self.inner.lock().unwrap();
            let mut changed = false;
            for p in panes_mut(&mut inner.projects) {
                let before = p.children.len();
                p.children.retain(|c| now.saturating_duration_since(c.at) < CHILD_SILENCE);
                changed |= p.children.len() != before;
            }
            changed
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
    /// identity the row resumes from. Owner, status and the claim are read
    /// and written in one hold of the locks: decided apart, the parent could
    /// start a turn between the check and the write and lose its row.
    pub(crate) fn set_pane_session_id(&self, pane: PaneId, raw: &str) {
        let Some(session_id) = valid_session_id(raw) else {
            return;
        };
        let now = self.clock.now();
        self.edit_pane(pane, |p| {
            if p.is_child(&session_id) && p.status() == PaneStatus::Working {
                p.edit_child(now, &session_id, |_| {})
            } else {
                p.set_session_id(session_id)
            }
        });
    }
}
