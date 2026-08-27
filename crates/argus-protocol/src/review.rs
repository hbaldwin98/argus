//! A checkout's uncommitted work as the review viewer shows it (DESIGN.md
//! §9 M4). A rendered diff rather than a raw patch, so the client doesn't
//! re-parse what the daemon already knows.

use serde::{Deserialize, Serialize};

use crate::ids::CheckoutId;

pub const MAX_REVIEW_COMMENT_BYTES: usize = 4096;
pub const MAX_REVIEW_COMMENTS: usize = 100;

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

/// The exact reviewed lines a comment refers to. Both sides are retained:
/// removed lines have no new number, and added lines have no old number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewAnchor {
    pub base: ReviewBase,
    pub path: String,
    pub old_path: Option<String>,
    pub old_start: Option<u32>,
    pub old_end: Option<u32>,
    pub new_start: Option<u32>,
    pub new_end: Option<u32>,
    /// Marker included, so an agent sees the same text the viewer showed.
    pub text: Vec<String>,
}

impl ReviewAnchor {
    pub fn preferred_start(&self) -> Option<u32> {
        self.new_start.or(self.old_start)
    }

    /// One line, because this is typed into a harness prompt as an immediate
    /// notification and a newline would submit it half-written.
    pub fn notification(&self, body: &str) -> String {
        let (start, end) = match (self.new_start, self.new_end) {
            (None, None) => (self.old_start, self.old_end),
            range => range,
        };
        let where_ = match (start, end) {
            (Some(a), Some(b)) if a != b => format!("{}:{a}-{b}", self.path),
            (Some(a), _) => format!("{}:{a}", self.path),
            _ => self.path.clone(),
        };
        let quoted = match self.text.len() {
            1 => format!(" `{}`", self.text[0].trim()),
            n if n > 1 => format!(" ({n} lines)"),
            _ => String::new(),
        };
        let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
        format!("{where_}{quoted}: {body}")
    }
}

/// One durable, checkout-scoped piece of review feedback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: u64,
    pub anchor: ReviewAnchor,
    pub body: String,
}

#[cfg(test)]
mod tests {
    use super::{ReviewAnchor, ReviewBase};

    #[test]
    fn review_bases_toggle_between_the_two_sides() {
        assert_eq!(ReviewBase::Unstaged.next(), ReviewBase::Staged);
        assert_eq!(ReviewBase::Staged.next(), ReviewBase::Unstaged);
    }

    #[test]
    fn a_notification_uses_the_new_side_and_stays_on_one_line() {
        let anchor = ReviewAnchor {
            base: ReviewBase::Unstaged,
            path: "src/main.rs".into(),
            old_path: None,
            old_start: Some(4),
            old_end: Some(4),
            new_start: Some(5),
            new_end: Some(5),
            text: vec!["+new".into()],
        };

        assert_eq!(
            anchor.notification("first thought\nsecond thought"),
            "src/main.rs:5 `+new`: first thought second thought"
        );
    }
}
