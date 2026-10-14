//! How what Argus says to an agent reaches it: through its harness's inbox
//! when one is open, typed into its terminal otherwise.
//!
//! An inbox is an in-process channel the pane API streams to an adapter
//! inside the agent (opencode's plugin, pi's extension, Claude Code's mod)
//! that speaks the harness's own protocol, which beats typing: nothing
//! lands in a dialog, and a message waits for the agent's turn as the
//! harness queues one. An adapter opens it only once it can act on what it
//! is told, so an open inbox is the whole route decision. Only the pane's
//! own agent may open one, and the newest connection wins, so a plugin that
//! reconnects after a hiccup takes over from its old stream.

use argus_protocol::InboxItem;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::*;

/// The inbox a pane's harness adapter holds open, by the generation of the
/// connection that opened it.
pub(super) type Inbox = Option<(u64, UnboundedSender<InboxItem>)>;

impl Daemon {
    /// Tells an agent something now, without waiting for its prompt: what
    /// Argus has to say on its own account, such as a review comment left
    /// for it. Through its inbox when one is open, where the harness queues
    /// it as it would a person's message, else typed in. Says whether it
    /// went.
    pub(super) fn notify(&self, pane: PaneId, text: &str) -> bool {
        let message = InboxItem::Message { text: text.to_string() };
        let taken = {
            let inner = self.inner.lock().unwrap();
            let Some(p) = find_pane_ref(&inner.projects, pane).filter(|p| p.is_running_agent())
            else {
                return false;
            };
            // A send fails only in the moment between an adapter letting go
            // of its end and closing the inbox, when the terminal is the way
            // left.
            p.inbox.as_ref().is_some_and(|(_, tx)| tx.send(message).is_ok())
        };
        taken || self.write_pane(pane, format!("{text}\r").as_bytes()).is_ok()
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
            let was = p.inbox.is_some();
            p.inbox = Some((generation, tx));
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
                Some(p) if p.inbox.as_ref().is_some_and(|(g, _)| *g == generation) => {
                    p.inbox = None;
                    true
                }
                _ => false,
            }
        };
        if closed {
            self.broadcast_tree();
        }
    }
}
