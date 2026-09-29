//! The live channel a harness's plugin keeps open, for what the daemon has
//! to tell the agent: a reply, an interrupt, an answer.
//!
//! A plugin running inside an agent (opencode's, pi's) can take these
//! through the harness's own interface, which beats typing: nothing lands
//! in a dialog, a reply can steer or wait as the harness does, and a
//! question the plugin posed can be answered. Only the pane's own agent may
//! open it, and the newest connection wins, so a plugin that reconnects
//! after a hiccup takes over from its old stream.

use argus_protocol::InboxItem;

use super::*;

impl Daemon {
    /// Opens the pane's inbox for its own agent: the generation that names
    /// this connection, and where its items arrive. `None` for a pane that
    /// is gone or a report from an agent the pane does not belong to.
    pub(super) fn open_inbox(
        &self,
        pane: PaneId,
        reporter: Option<&str>,
    ) -> Option<(u64, tokio::sync::mpsc::UnboundedReceiver<InboxItem>)> {
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

    /// Hands an item to the pane's plugin. Says whether one was listening.
    pub(super) fn tell(&self, pane: PaneId, item: InboxItem) -> bool {
        let sender = {
            let inner = self.inner.lock().unwrap();
            find_pane_ref(&inner.projects, pane).and_then(|p| p.inbox.as_ref().map(|(_, tx)| tx.clone()))
        };
        sender.is_some_and(|tx| tx.send(item).is_ok())
    }

    /// Answers a question the pane's harness posed. Only a harness with a
    /// live channel poses one; any other takes its answers from its screen.
    pub fn answer(&self, pane: PaneId, question: String, choice: String) -> anyhow::Result<()> {
        if self.tell(pane, InboxItem::Answer { question, choice }) {
            Ok(())
        } else {
            anyhow::bail!("this agent takes answers on its screen, not through Argus")
        }
    }
}
