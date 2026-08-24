//! A checkout's uncommitted work as the review viewer shows it (DESIGN.md
//! §9 M4). A rendered diff rather than a raw patch, so the client doesn't
//! re-parse what the daemon already knows.

use serde::{Deserialize, Serialize};

use crate::ids::CheckoutId;

/// What a review is a diff *against* (DESIGN.md §5), cycled with `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewBase {
    /// Uncommitted edits: `HEAD` against the working tree.
    WorkingTree,
    /// Everything this branch did: its fork point against the working tree,
    /// falling back to the upstream branch where there is no fork point.
    BranchPoint,
    /// Changes after the last snapshot the operator explicitly accepted.
    SinceLastLooked,
}

impl ReviewBase {
    pub fn next(self) -> Self {
        match self {
            ReviewBase::WorkingTree => ReviewBase::BranchPoint,
            ReviewBase::BranchPoint => ReviewBase::SinceLastLooked,
            ReviewBase::SinceLastLooked => ReviewBase::WorkingTree,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ReviewBase::WorkingTree => "uncommitted",
            ReviewBase::BranchPoint => "this branch",
            ReviewBase::SinceLastLooked => "last looked",
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
    /// Opaque identities understood only by the daemon.
    pub target_snapshot: String,
    pub baseline_snapshot: Option<String>,
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
    fn review_bases_cycle_through_all_three_choices() {
        assert_eq!(ReviewBase::WorkingTree.next(), ReviewBase::BranchPoint);
        assert_eq!(ReviewBase::BranchPoint.next(), ReviewBase::SinceLastLooked);
        assert_eq!(ReviewBase::SinceLastLooked.next(), ReviewBase::WorkingTree);
    }
}
