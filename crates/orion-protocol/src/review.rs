//! A checkout's uncommitted work as the review viewer shows it (DESIGN.md
//! §9 M4). A rendered diff rather than a raw patch, so the client doesn't
//! re-parse what the daemon already knows.

use serde::{Deserialize, Serialize};

use crate::ids::CheckoutId;

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
    pub checkout: CheckoutId,
    pub files: Vec<FileDiff>,
}

impl Review {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}
