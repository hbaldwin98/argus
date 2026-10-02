//! What a person says to an agent — a message, a steer, an interrupt, an
//! answer — and the way it reaches it: through its harness's inbox when one
//! is open, typed now, or held until the agent is back at its prompt.
//!
//! An inbox is an in-process channel held by an adapter that speaks the
//! harness's own protocol (opencode's and pi's plugin stream, the Codex
//! app-server follower), which beats typing: nothing lands in a dialog, a
//! message can steer or wait as the harness does, and a question the
//! harness posed can be answered. An adapter opens it only once it can act
//! on what it is told, so an open inbox is the whole route decision. Only
//! the pane's own agent may open one, and the newest connection wins, so a
//! plugin that reconnects after a hiccup takes over from its old stream.
//!
//! Typing into a pane is typing into whatever has the agent's keyboard, and
//! a dialog that has it takes the text as its answer. So a message waits
//! until the agent is at its prompt; [`delivery`] holds every rule for when
//! that is, and why. The queue lives only in memory; a daemon that restarts
//! forgets it, and a client that queued something learns so from the tree.

use std::collections::VecDeque;
use std::time::SystemTime;

use argus_protocol::{InboxItem, QueuedMessage, Sent, MAX_SEND_BYTES};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::*;
use crate::harness::Harness;

/// How long an agent must have been idle before it is typed into. A turn's
/// last hook can arrive a moment before the prompt it returns to is drawn.
const SETTLE: Duration = Duration::from_millis(300);

/// How long after typing a message the next may follow into a pane whose
/// harness reports no turn at all.
const UNTURNED: Duration = Duration::from_secs(2);

/// How often a pane with something queued is looked at, besides whenever
/// the tree changes.
const POLL: Duration = Duration::from_millis(250);

/// How long a message's Enter waits behind its paste. Claude Code reads an
/// Enter that arrives with the paste as part of what was pasted, and leaves
/// the message sitting unsent in its input.
const ENTER_AFTER: Duration = Duration::from_millis(150);

/// The task typing queued messages, started by the first message queued,
/// and the id the next one gets.
#[derive(Default)]
pub(super) struct Outbox {
    worker: Option<tokio::task::JoinHandle<()>>,
    next_id: u64,
}

/// What a pane holds of what is said to its agent.
#[derive(Default)]
pub(super) struct Speaking {
    /// Messages waiting to be typed once the agent is at its prompt.
    pub(super) queued: VecDeque<QueuedMessage>,
    /// When a queued message was last typed into the pane.
    pub(super) typed_at: Option<SystemTime>,
    /// Whether its agent has reported a status yet, set by the first
    /// report. Until one whose harness reports has, `Idle` is only the
    /// default, and its screen may hold a startup dialog, not a prompt.
    pub(super) heard: bool,
    /// The inbox its harness's adapter holds open, by the generation of the
    /// connection that opened it.
    pub(super) inbox: Option<(u64, UnboundedSender<InboxItem>)>,
}

/// The way something said to a pane went, or must go.
enum Way {
    /// Its harness's inbox took it.
    Inbox,
    /// Its terminal, where `interrupt` is what stops the agent.
    Terminal { interrupt: Vec<u8> },
    /// No running agent is there to hear it.
    Nobody,
}

/// What the outbox does with the head of a pane's queue.
#[derive(Debug, PartialEq)]
enum Delivery {
    Type,
    Wait,
    /// Throws the whole queue away.
    Drop,
}

/// Everything [`delivery`] reads of a pane with a message queued.
struct Moment {
    status: PaneStatus,
    status_since: SystemTime,
    typed_at: Option<SystemTime>,
    heard: bool,
    /// Whether its harness reports its status at all.
    reports: bool,
    now: SystemTime,
}

/// Whether the next queued message is typed now.
///
/// - An agent that has exited reads nothing, so its queue goes.
/// - One whose harness reports is not typed into until it has reported
///   something (see [`Speaking::heard`]); one whose harness never reports
///   would wait forever, so it does not.
/// - It must be at its prompt: idle, or done, failed or wanting a review,
///   which an agent also says at the end of a turn. Codex's and Cursor's
///   approval prompts report as working, so they hold a message too.
/// - And have been so for [`SETTLE`].
/// - One message goes out per turn: after one is typed, the next waits
///   until the pane has been busy and come back, or, for a harness that
///   reports no turn, [`UNTURNED`].
fn delivery(m: &Moment) -> Delivery {
    if matches!(m.status, PaneStatus::Exited { .. }) {
        return Delivery::Drop;
    }
    if m.reports && !m.heard {
        return Delivery::Wait;
    }
    let at_prompt = matches!(
        m.status,
        PaneStatus::Idle | PaneStatus::NeedsReview | PaneStatus::Done | PaneStatus::Failed
    );
    let settled = m.now.duration_since(m.status_since).is_ok_and(|held| held >= SETTLE);
    let turned = match m.typed_at {
        None => true,
        Some(typed) => {
            m.status_since > typed || m.now.duration_since(typed).is_ok_and(|d| d >= UNTURNED)
        }
    };
    if at_prompt && settled && turned {
        Delivery::Type
    } else {
        Delivery::Wait
    }
}

impl Daemon {
    /// Says `text` to an agent: queued until it is at its prompt, or typed
    /// now when a person has chosen to steer it mid-turn.
    pub fn send_to_agent(self: &Arc<Self>, pane: PaneId, text: &str, now: bool) -> Sent {
        let text = text.trim_end();
        if text.trim().is_empty() {
            return refused("there is nothing to send");
        }
        if text.len() > MAX_SEND_BYTES {
            return refused(&format!("a message holds at most {MAX_SEND_BYTES} bytes"));
        }

        let message = InboxItem::Message {
            text: text.to_string(),
            steer: now,
        };
        match self.say(pane, message) {
            // The harness waits or steers as it does; nothing is typed, so
            // nothing needs holding.
            Way::Inbox => return Sent::Typed,
            Way::Nobody => return refused("that is not a running agent"),
            Way::Terminal { .. } => {}
        }
        if now {
            return match self.type_into(pane, text) {
                Ok(()) => Sent::Typed,
                Err(error) => refused(&error.to_string()),
            };
        }

        let id = {
            let mut outbox = self.outbox.lock().unwrap();
            outbox.next_id += 1;
            outbox.next_id
        };
        let queued = {
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if p.is_running_agent() => {
                    p.speaking.queued.push_back(QueuedMessage {
                        id,
                        text: text.to_string(),
                    });
                    true
                }
                _ => false,
            }
        };
        if !queued {
            return refused("that is not a running agent");
        }
        self.start_outbox();
        self.broadcast_tree();
        Sent::Queued { id }
    }

    /// Takes back a message still waiting. Says whether it was.
    pub fn cancel_queued(&self, pane: PaneId, id: u64) -> bool {
        let removed = {
            let mut inner = self.inner.lock().unwrap();
            find_pane(&mut inner.projects, pane).is_some_and(|p| {
                let queued = &mut p.speaking.queued;
                let before = queued.len();
                queued.retain(|m| m.id != id);
                queued.len() != before
            })
        };
        if removed {
            self.broadcast_tree();
        }
        removed
    }

    /// Interrupts an agent: through its inbox when one is open, else with
    /// its harness's own key.
    pub fn interrupt(&self, pane: PaneId) -> anyhow::Result<()> {
        match self.say(pane, InboxItem::Interrupt) {
            Way::Inbox => Ok(()),
            Way::Terminal { interrupt } => self.write_pane(pane, &interrupt),
            Way::Nobody => anyhow::bail!("that is not a running agent"),
        }
    }

    /// Answers a question the pane's harness posed. Only a harness with an
    /// inbox poses one; any other takes its answers from its screen.
    pub fn answer(&self, pane: PaneId, question: String, choice: String) -> anyhow::Result<()> {
        match self.say(pane, InboxItem::Answer { question, choice }) {
            Way::Inbox => Ok(()),
            Way::Terminal { .. } => {
                anyhow::bail!("this agent takes answers on its screen, not through Argus")
            }
            Way::Nobody => anyhow::bail!("that is not a running agent"),
        }
    }

    /// Tells an agent something now, without waiting for its prompt: what
    /// Argus has to say on its own account, such as a review comment left
    /// for it. Through its inbox when one is open, where the harness queues
    /// it as it would a person's message, else typed in. Says whether it
    /// went.
    pub(super) fn notify(&self, pane: PaneId, text: &str) -> bool {
        let message = InboxItem::Message { text: text.to_string(), steer: false };
        match self.say(pane, message) {
            Way::Inbox => true,
            Way::Terminal { .. } => self.write_pane(pane, format!("{text}\r").as_bytes()).is_ok(),
            Way::Nobody => false,
        }
    }

    /// Hands `item` to the pane's inbox if one is open, or says which way
    /// it has to go instead. The one place the route is decided.
    fn say(&self, pane: PaneId, item: InboxItem) -> Way {
        let inner = self.inner.lock().unwrap();
        let Some(p) = find_pane_ref(&inner.projects, pane).filter(|p| p.is_running_agent()) else {
            return Way::Nobody;
        };
        // A send fails only in the moment between an adapter letting go of
        // its end and closing the inbox, when the terminal is the way left.
        let taken = p.speaking.inbox.as_ref().is_some_and(|(_, tx)| tx.send(item).is_ok());
        if taken {
            return Way::Inbox;
        }
        let interrupt = self
            .harness_of(p)
            .map_or_else(|| Harness::generic().interrupt_keys(), Harness::interrupt_keys);
        Way::Terminal { interrupt }
    }

    /// The harness the pane started under, while the config still has it.
    fn harness_of(&self, p: &Pane) -> Option<&Harness> {
        let name = p.harness.as_deref()?;
        self.harnesses.iter().find(|h| h.name == name)
    }

    /// Opens the pane's inbox for its own agent: the generation that names
    /// this connection, and where its items arrive. `None` for a pane that
    /// is gone or a report from an agent the pane does not belong to.
    pub(super) fn open_inbox(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
    ) -> Option<(u64, UnboundedReceiver<InboxItem>)> {
        if self.child_of(pane, reporter).is_some() {
            return None;
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let generation = self
            .next_inbox
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let became_live = {
            let mut inner = self.inner.lock().unwrap();
            let p = find_pane(&mut inner.projects, pane)?;
            let was = p.speaking.inbox.is_some();
            p.speaking.inbox = Some((generation, tx));
            !was
        };
        if became_live {
            self.broadcast_tree();
        }
        Some((generation, rx))
    }

    /// Closes the inbox a connection opened, unless a newer one has taken
    /// its place.
    pub(super) fn close_inbox(&self, pane: PaneId, generation: u64) {
        let closed = {
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if p.speaking.inbox.as_ref().is_some_and(|(g, _)| *g == generation) => {
                    p.speaking.inbox = None;
                    true
                }
                _ => false,
            }
        };
        if closed {
            self.broadcast_tree();
        }
    }

    /// One paste and, a moment later, Enter: what a person pasting the
    /// message and pressing return would have sent.
    fn type_into(&self, pane: PaneId, text: &str) -> anyhow::Result<()> {
        self.paste_pane(pane, text)?;
        let input = self
            .pane_input(pane)
            .ok_or_else(|| anyhow::anyhow!("no such pane"))?;
        tokio::spawn(async move {
            tokio::time::sleep(ENTER_AFTER).await;
            let _ = input.write(b"\r");
        });
        Ok(())
    }

    fn start_outbox(self: &Arc<Self>) {
        let mut outbox = self.outbox.lock().unwrap();
        if outbox.worker.as_ref().is_none_or(|w| w.is_finished()) {
            outbox.worker = Some(tokio::spawn(deliver(self.clone())));
        }
    }

    /// Types the next message into every pane ready for one, and drops the
    /// queue of any that will never read it. Says whether anything is
    /// still waiting.
    fn type_ready(&self) -> bool {
        let now = SystemTime::now();
        let mut due = Vec::new();
        let mut waiting = false;
        let mut dropped = false;
        {
            let mut inner = self.inner.lock().unwrap();
            for p in panes_mut(&mut inner.projects) {
                if p.speaking.queued.is_empty() {
                    continue;
                }
                let moment = Moment {
                    status: p.status,
                    status_since: p.status_since,
                    typed_at: p.speaking.typed_at,
                    heard: p.speaking.heard,
                    reports: self.harness_of(p).is_some_and(Harness::reports),
                    now,
                };
                match delivery(&moment) {
                    Delivery::Wait => waiting = true,
                    Delivery::Drop => {
                        p.speaking.queued.clear();
                        dropped = true;
                    }
                    Delivery::Type => {
                        let message = p.speaking.queued.pop_front().expect("the queue is not empty");
                        p.speaking.typed_at = Some(now);
                        due.push((p.id, message.text));
                        waiting |= !p.speaking.queued.is_empty();
                    }
                }
            }
        }
        for (pane, text) in &due {
            if let Err(error) = self.type_into(*pane, text) {
                tracing::warn!("could not type a queued message into pane {}: {error}", pane.0);
            }
        }
        if !due.is_empty() || dropped {
            self.broadcast_tree();
        }
        waiting
    }
}

/// Watches for panes ready for their next message: whenever the tree
/// changes, which every status report and every queued message does, and
/// every [`POLL`] while anything waits, for the settle and the harnesses
/// that never report.
async fn deliver(daemon: Arc<Daemon>) {
    let mut tree = daemon.subscribe_tree();
    loop {
        if daemon.type_ready() {
            tokio::select! {
                _ = tree.recv() => {}
                _ = tokio::time::sleep(POLL) => {}
            }
        } else if let Err(broadcast::error::RecvError::Closed) = tree.recv().await {
            return;
        }
    }
}

fn refused(reason: &str) -> Sent {
    Sent::Refused {
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reporting agent that has reported, idle long enough to have
    /// settled, and never typed into: ready. Each test changes what its
    /// rule reads.
    fn ready_moment() -> Moment {
        let now = SystemTime::now();
        Moment {
            status: PaneStatus::Idle,
            status_since: now - SETTLE - Duration::from_millis(1),
            typed_at: None,
            heard: true,
            reports: true,
            now,
        }
    }

    #[test]
    fn a_settled_idle_agent_that_has_reported_is_typed_into() {
        assert_eq!(delivery(&ready_moment()), Delivery::Type);
    }

    #[test]
    fn an_exited_agents_queue_is_dropped_whatever_else_holds() {
        for heard in [false, true] {
            let m = Moment {
                status: PaneStatus::Exited { code: Some(0) },
                heard,
                ..ready_moment()
            };
            assert_eq!(delivery(&m), Delivery::Drop, "heard: {heard}");
        }
    }

    #[test]
    fn a_reporting_agent_is_not_typed_into_before_it_has_reported() {
        // The bug: a Claude pane still showing its folder-trust dialog read
        // as idle, being the default, and the dialog took the message.
        let m = Moment {
            heard: false,
            ..ready_moment()
        };
        assert_eq!(delivery(&m), Delivery::Wait);
    }

    #[test]
    fn an_agent_whose_harness_never_reports_does_not_wait_to_be_heard() {
        let m = Moment {
            heard: false,
            reports: false,
            ..ready_moment()
        };
        assert_eq!(delivery(&m), Delivery::Type);
    }

    #[test]
    fn only_an_agent_at_its_prompt_is_typed_into() {
        for busy in [PaneStatus::Working, PaneStatus::Waiting] {
            let m = Moment {
                status: busy,
                ..ready_moment()
            };
            assert_eq!(delivery(&m), Delivery::Wait, "{busy:?} holds a message");
        }
        for done in [PaneStatus::NeedsReview, PaneStatus::Done, PaneStatus::Failed] {
            let m = Moment {
                status: done,
                ..ready_moment()
            };
            assert_eq!(delivery(&m), Delivery::Type, "{done:?} is the end of a turn");
        }
    }

    #[test]
    fn an_agent_is_typed_into_only_once_its_prompt_has_settled() {
        let m = ready_moment();
        let just = Moment {
            status_since: m.now - Duration::from_millis(50),
            ..m
        };
        assert_eq!(delivery(&just), Delivery::Wait);
    }

    #[test]
    fn the_next_message_waits_for_a_turn() {
        let m = ready_moment();
        let typed = m.now - Duration::from_millis(500);

        // Idle since before the last message: the agent never took its turn.
        let untaken = Moment {
            status_since: typed - Duration::from_secs(1),
            typed_at: Some(typed),
            ..ready_moment()
        };
        assert_eq!(delivery(&untaken), Delivery::Wait);

        // Busy and back since: a turn has passed.
        let taken = Moment {
            status_since: typed + Duration::from_millis(100),
            typed_at: Some(typed),
            ..ready_moment()
        };
        assert_eq!(delivery(&taken), Delivery::Type);

        // A harness that reports no turn still gets the next, a while later.
        let long_ago = m.now - UNTURNED - Duration::from_millis(1);
        let unturned = Moment {
            status_since: long_ago - Duration::from_secs(1),
            typed_at: Some(long_ago),
            ..m
        };
        assert_eq!(delivery(&unturned), Delivery::Type);
    }
}
