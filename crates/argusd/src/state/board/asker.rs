//! Who is asking for a change to a board: a person at a client, or an
//! agent through its pane.
//!
//! Where the asker is looking from is the board's address; this carries
//! only what a rule decides on and what a row records as whose it is.

use argus_protocol::PaneId;

#[derive(Debug, Clone, Copy)]
pub enum Asker<'a> {
    /// A person at a client: neither a conversation nor a pane, which is
    /// what lets a claim or a note say whose it is without a flag for it.
    Person,
    /// An agent: the pane it asked through, and the conversation it is
    /// when its harness said.
    Agent {
        pane: PaneId,
        session: Option<&'a str>,
    },
}

impl<'a> Asker<'a> {
    pub fn session(self) -> Option<&'a str> {
        match self {
            Asker::Person => None,
            Asker::Agent { session, .. } => session,
        }
    }

    pub fn pane(self) -> Option<PaneId> {
        match self {
            Asker::Person => None,
            Asker::Agent { pane, .. } => Some(pane),
        }
    }
}

/// How a row an agent left names it to the person reading it.
pub fn pane_label(pane: PaneId) -> String {
    format!("pane #{}", pane.0)
}
