//! What a client may ask git's index of a checkout to take or give back:
//! whole files, or lines picked off a diff it was shown.

use serde::{Deserialize, Serialize};

use crate::review::{LineKind, ReviewBase};

/// One change to a checkout's index, written at once — as `git add -p`
/// writes it, never held anywhere for later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexChange {
    /// These files as they are on disk, deletions and new files included:
    /// `git add -A`.
    Stage { paths: Vec<String> },
    /// These files as `HEAD` has them, or gone from the index where `HEAD`
    /// has none.
    Unstage { paths: Vec<String> },
    /// Some of one file's changed lines, picked off the diff on side `from`:
    /// staged from [`ReviewBase::Unstaged`], unstaged from
    /// [`ReviewBase::Staged`]. Refused when a picked line is no longer where
    /// the diff showed it, since the file moved on and the person picked
    /// from what they could see.
    Lines {
        from: ReviewBase,
        path: String,
        lines: Vec<PickedLine>,
    },
}

/// A changed line as the diff showed it. Added lines are named by their new
/// number and removed lines by their old one; the text is what makes a pick
/// of a line that has since moved a refusal rather than a different line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickedLine {
    pub kind: LineKind,
    pub lineno: u32,
    pub text: String,
}
