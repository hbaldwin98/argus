//! A working-tree file edited live, as the daemon and a client share it:
//! the changes each side sends the other, and the version each answers
//! to. The file on disk is the authority. A client's change applies only
//! to the version it was made on, and whatever else writes the file — an
//! agent above all — reaches the client as a change of its own, so the two
//! meet in the client's buffer rather than one overwriting the other.

use serde::{Deserialize, Serialize};

use crate::tree::ids::CheckoutId;

/// `from..to` replaced by `insert`. Positions count UTF-16 code units, as
/// the page's editor does, in the text with each CRLF read as one LF; the
/// daemon writes the file's own line ending back. A change set is
/// positioned in the text before any of it applies: sorted, none
/// overlapping another.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextChange {
    pub from: u32,
    pub to: u32,
    pub insert: String,
}

/// A file opened for editing: its text at `version`, which the client's
/// first change is made on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditOpened {
    pub checkout: CheckoutId,
    pub path: String,
    pub version: u64,
    pub text: String,
}

/// The file changed on disk, by something other than this client: the
/// changes that take version `from` to `version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditChanged {
    pub checkout: CheckoutId,
    pub path: String,
    pub from: u64,
    pub version: u64,
    pub changes: Vec<TextChange>,
}
