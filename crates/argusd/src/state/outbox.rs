//! What a client asked to say to an agent, and when it is typed.
//!
//! Typing into a pane is typing into whatever has the agent's keyboard, and
//! a dialog that has it takes the text as its answer. So a message waits
//! until the agent says it is idle — or done, failed, or wanting a review,
//! which an agent also says at the end of a turn — and has said so for long
//! enough that its prompt is back. Codex's and Cursor's approval prompts
//! report as working, so they hold a message too.
//!
//! A pane whose harness reports is not typed into until it has reported
//! something: before its first hook, `Idle` is only the default, and what
//! is on its screen may be a startup dialog — a folder to trust — that
//! would take the message as its answer.
//!
//! One message goes out per turn: after typing one, the next waits until
//! the pane has been busy and come back, or, for a harness that reports
//! nothing, a couple of seconds. The queue lives only in memory; a daemon
//! that restarts forgets it, and a client that queued something learns so
//! from the tree.

use argus_protocol::{QueuedMessage, Sent, MAX_SEND_BYTES};

use super::*;

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

/// Whether an agent in this state is at its prompt.
fn at_prompt(status: PaneStatus) -> bool {
    matches!(
        status,
        PaneStatus::Idle | PaneStatus::NeedsReview | PaneStatus::Done | PaneStatus::Failed
    )
}

/// Whether a pane in `status` since `since`, last typed into at `typed`,
/// is ready for its next message at `now`.
fn ready(
    status: PaneStatus,
    since: std::time::SystemTime,
    typed: Option<std::time::SystemTime>,
    now: std::time::SystemTime,
) -> bool {
    let settled = now.duration_since(since).is_ok_and(|held| held >= SETTLE);
    let turned = match typed {
        None => true,
        Some(typed) => since > typed || now.duration_since(typed).is_ok_and(|d| d >= UNTURNED),
    };
    at_prompt(status) && settled && turned
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
        let is_live_agent = |p: &Pane| {
            p.kind == PaneKind::Agent && !matches!(p.status, PaneStatus::Exited { .. })
        };
        // A harness with a live channel takes the message itself, and waits
        // or steers as it does; nothing is typed, so nothing needs holding.
        let live = {
            let inner = self.inner.lock().unwrap();
            find_pane_ref(&inner.projects, pane).is_some_and(|p| is_live_agent(p) && p.inbox.is_some())
        };
        let message = argus_protocol::InboxItem::Message {
            text: text.to_string(),
            steer: now,
        };
        if live && self.tell(pane, message) {
            return Sent::Typed;
        }
        if now {
            let live = {
                let inner = self.inner.lock().unwrap();
                find_pane_ref(&inner.projects, pane).is_some_and(is_live_agent)
            };
            if !live {
                return refused("that is not a running agent");
            }
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
                Some(p) if is_live_agent(p) => {
                    p.queued.push_back(QueuedMessage {
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
                let before = p.queued.len();
                p.queued.retain(|m| m.id != id);
                p.queued.len() != before
            })
        };
        if removed {
            self.broadcast_tree();
        }
        removed
    }

    /// Interrupts an agent: through its live channel when it has one, else
    /// with its harness's own key.
    pub fn interrupt(&self, pane: PaneId) -> anyhow::Result<()> {
        if self.tell(pane, argus_protocol::InboxItem::Interrupt) {
            return Ok(());
        }
        let keys = {
            let inner = self.inner.lock().unwrap();
            let p = find_pane_ref(&inner.projects, pane).ok_or_else(|| anyhow::anyhow!("no such pane"))?;
            p.harness
                .as_deref()
                .and_then(|name| self.harnesses.iter().find(|h| h.name == name))
                .map_or_else(|| vec![0x1b], |h| h.interrupt_keys())
        };
        self.write_pane(pane, &keys)
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
    /// queue of any agent that has exited, since nothing will read it.
    /// Says whether anything is still waiting.
    fn type_ready(&self) -> bool {
        let now = std::time::SystemTime::now();
        let mut due = Vec::new();
        let mut waiting = false;
        let mut dropped = false;
        {
            let mut inner = self.inner.lock().unwrap();
            for p in panes_mut(&mut inner.projects) {
                if p.queued.is_empty() {
                    continue;
                }
                let silent_harness = !p
                    .harness
                    .as_deref()
                    .and_then(|name| self.harnesses.iter().find(|h| h.name == name))
                    .is_some_and(|h| h.reports());
                if !p.heard && !silent_harness {
                    waiting = true;
                    continue;
                }
                if matches!(p.status, PaneStatus::Exited { .. }) {
                    p.queued.clear();
                    dropped = true;
                    continue;
                }
                if ready(p.status, p.status_since, p.typed_at, now) {
                    if let Some(message) = p.queued.pop_front() {
                        p.typed_at = Some(now);
                        due.push((p.id, message.text));
                    }
                }
                waiting |= !p.queued.is_empty();
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
    use std::time::SystemTime;

    #[test]
    fn a_pane_is_ready_once_its_idle_has_settled() {
        let now = SystemTime::now();
        let just = now - Duration::from_millis(50);
        let settled = now - SETTLE - Duration::from_millis(1);
        assert!(!ready(PaneStatus::Idle, just, None, now), "not yet settled");
        assert!(ready(PaneStatus::Idle, settled, None, now));
        for busy in [PaneStatus::Working, PaneStatus::Waiting, PaneStatus::Exited { code: None }] {
            assert!(!ready(busy, settled, None, now), "{busy:?} holds a message");
        }
        for done in [PaneStatus::NeedsReview, PaneStatus::Done, PaneStatus::Failed] {
            assert!(ready(done, settled, None, now), "{done:?} is the end of a turn");
        }
    }

    #[test]
    fn the_next_message_waits_for_a_turn() {
        let now = SystemTime::now();
        let typed = now - Duration::from_millis(500);
        let before_typing = typed - Duration::from_secs(1);
        let after_typing = typed + Duration::from_millis(100);
        // Idle since before the last message: the agent never took its turn.
        assert!(!ready(PaneStatus::Idle, before_typing, Some(typed), now));
        // Busy and back since: a turn has passed.
        assert!(ready(PaneStatus::Idle, after_typing, Some(typed), now));
        // A harness that reports nothing still gets the next, a while later.
        let long_ago = now - UNTURNED - Duration::from_millis(1);
        assert!(ready(PaneStatus::Idle, before_typing - UNTURNED, Some(long_ago), now));
    }
}
