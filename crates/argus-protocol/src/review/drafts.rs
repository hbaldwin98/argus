//! A review being written: comments held as drafts against a checkout
//! until the person sends them, all at once, to one agent.

use serde::{Deserialize, Serialize};

use crate::review::ReviewAnchor;
use crate::tree::ids::PaneId;

/// How many drafts one checkout holds, as many as are shown of the sent.
pub const MAX_REVIEW_DRAFTS: usize = crate::review::MAX_REVIEW_COMMENTS;

/// A comment not yet sent. Kept by the daemon, so a draft outlives the
/// window it was written in and a restart of either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewDraft {
    pub id: u64,
    pub anchor: ReviewAnchor,
    pub body: String,
}

/// What a client does to a checkout's drafts. Each is answered with the
/// drafts as they then are, never with only what changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftAction {
    List,
    Add { anchor: Box<ReviewAnchor>, body: String },
    Edit { id: u64, body: String },
    Delete { id: u64 },
    /// Every draft, unsent.
    Discard,
    /// Every draft, sent as one review: each becomes a review comment, and
    /// `recipient`, a live agent in the checkout, is told of them in one
    /// notification that opens with `summary`.
    Submit { recipient: PaneId, summary: String },
}
