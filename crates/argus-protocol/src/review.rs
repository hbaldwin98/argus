//! A checkout's uncommitted work as the review viewer shows it (DESIGN.md
//! §9 M4). A rendered diff rather than a raw patch, so the client doesn't
//! re-parse what the daemon already knows.

use serde::{Deserialize, Serialize};

use crate::ids::CheckoutId;

/// Which half of a checkout's uncommitted work a review shows, toggled with
/// `b`. These are the two sides Git itself keeps apart, and a file modified
/// on both sides has a different diff in each — which is the whole reason
/// not to merge them into one endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewBase {
    /// `git diff`: the index against the working tree.
    Unstaged,
    /// `git diff --cached`: `HEAD` against the index.
    Staged,
}

impl ReviewBase {
    pub fn next(self) -> Self {
        match self {
            ReviewBase::Unstaged => ReviewBase::Staged,
            ReviewBase::Staged => ReviewBase::Unstaged,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ReviewBase::Unstaged => "unstaged",
            ReviewBase::Staged => "staged",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

impl ChangeKind {
    /// Matches `git status --short`.
    pub fn marker(self) -> char {
        match self {
            ChangeKind::Added => 'A',
            ChangeKind::Modified => 'M',
            ChangeKind::Deleted => 'D',
            ChangeKind::Renamed => 'R',
            ChangeKind::Untracked => '?',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

/// Both line numbers are carried so a comment can say which side it means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    /// No leading marker, no trailing newline; the client draws the marker.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    /// The `@@ ... @@` line, verbatim.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    /// Repo-relative, forward-slashed on every platform.
    pub path: String,
    /// The pre-rename path, when `kind` is `Renamed`.
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    pub hunks: Vec<Hunk>,
    /// Why `hunks` is empty despite real changes: binary, or over the cap.
    pub note: Option<String>,
}

impl FileDiff {
    pub fn added_lines(&self) -> usize {
        self.count(LineKind::Added)
    }

    pub fn removed_lines(&self) -> usize {
        self.count(LineKind::Removed)
    }

    fn count(&self, kind: LineKind) -> usize {
        self.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == kind)
            .count()
    }
}

/// Carries the checkout id so a client that moved on can drop a stale reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub request_id: u64,
    pub checkout: CheckoutId,
    pub base: ReviewBase,
    pub files: Vec<FileDiff>,
}

impl Review {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::ReviewBase;

    #[test]
    fn review_bases_toggle_between_the_two_sides() {
        assert_eq!(ReviewBase::Unstaged.next(), ReviewBase::Staged);
        assert_eq!(ReviewBase::Staged.next(), ReviewBase::Unstaged);
    }
}
