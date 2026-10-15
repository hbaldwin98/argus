//! A repository's agent sessions as the daemon lists them: one row per
//! conversation a harness kept for any of its checkouts, or an agent pane
//! running one, newest first, with enough of each to pick one to preview
//! or resume. What a session said is `transcript`'s, asked for one session
//! at a time.

use serde::{Deserialize, Serialize};

use crate::ids::{CheckoutId, PaneId};

/// The most sessions one listing holds. A repository an agent has worked
/// in for months has thousands; a list is for picking up recent work.
pub const MAX_SESSIONS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The harness that ran it, whose `resume_id` reopens it.
    pub harness: String,
    /// The harness's own id for it: what its hooks report and what its
    /// `resume_id` template is filled with.
    pub session_id: String,
    /// The checkout it ran in, or for one whose worktree has since been
    /// removed, the repository's primary checkout.
    pub checkout: CheckoutId,
    /// Where it ran, when that was a worktree of the repository that is
    /// gone: its transcript outlives the directory, and a client shows it
    /// as past rather than one to resume where it was.
    #[serde(default)]
    pub removed_worktree: Option<String>,
    /// The harness's title for it, else the first thing the person asked.
    #[serde(default)]
    pub title: Option<String>,
    /// The first and last times its transcript records, as the harness
    /// wrote them (RFC 3339 for every harness that writes one).
    #[serde(default)]
    pub started: Option<String>,
    #[serde(default)]
    pub ended: Option<String>,
    /// When its transcript was last written, in milliseconds since the Unix
    /// epoch: what the list is ordered by, and the only time a harness that
    /// writes none (Cursor) has. `None` for a session with no transcript.
    #[serde(default)]
    pub modified: Option<u64>,
    /// How many times the person spoke. `None` where a listing did not
    /// read far enough to count: a transcript too long to read whole for a
    /// list, or none at all. Reading the session counts them.
    #[serde(default)]
    pub turns: Option<u32>,
    /// The pane the session is running in now, if any.
    #[serde(default)]
    pub pane: Option<PaneId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_written_with_only_its_identity_still_reads() {
        // What a daemon that knows nothing of a session but its pane sends,
        // less the fields it would leave empty.
        #[derive(Serialize)]
        struct Bare {
            harness: &'static str,
            session_id: &'static str,
            checkout: CheckoutId,
        }
        let bytes = rmp_serde::to_vec_named(&Bare {
            harness: "opencode",
            session_id: "s1",
            checkout: CheckoutId(2),
        })
        .unwrap();
        let read: Session = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(read.session_id, "s1");
        assert_eq!((read.turns, read.pane, read.modified), (None, None, None));
        assert_eq!(read.removed_worktree, None);
    }
}
