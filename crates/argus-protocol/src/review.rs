//! A checkout's uncommitted work, and a single commit against its first
//! parent, as the review viewer shows them (DESIGN.md §9 M4). A rendered
//! diff rather than a raw patch, so the client doesn't re-parse what the
//! daemon already knows.

use serde::{Deserialize, Serialize};

use crate::ids::CheckoutId;

pub const MAX_REVIEW_COMMENT_BYTES: usize = 4096;
pub const MAX_REVIEW_COMMENTS: usize = 100;
/// Newest first, walking back from HEAD. Enough to browse recent work;
/// older than this is what `git log` is for.
pub const MAX_HISTORY_COMMITS: usize = 100;

/// Which snapshot a review shows. `b` toggles the two uncommitted sides;
/// a committed snapshot is a separate request, not a third value of that
/// toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewBase {
    /// `git diff`: the index against the working tree.
    Unstaged,
    /// `git diff --cached`: `HEAD` against the index.
    Staged,
    /// The parent of a commit against that commit. Which commit is on
    /// [`Review::commit`], not here — this stays a flag so it remains `Copy`.
    Commit,
}

impl ReviewBase {
    pub fn next(self) -> Self {
        match self {
            ReviewBase::Unstaged => ReviewBase::Staged,
            ReviewBase::Staged => ReviewBase::Unstaged,
            ReviewBase::Commit => ReviewBase::Unstaged,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ReviewBase::Unstaged => "unstaged",
            ReviewBase::Staged => "staged",
            ReviewBase::Commit => "commit",
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

/// What a run of source text is, not what colour it should be. The daemon
/// owns the parse because it owns the blobs; the client's theme owns the
/// palette, so a renderer stays replaceable and the wire carries no styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HighlightKind {
    Keyword,
    Str,
    Comment,
    Number,
    Type,
    Function,
    Constant,
    Property,
    Operator,
    Punctuation,
}

/// A highlighted run within one line's `text`. Byte offsets, always on UTF-8
/// character boundaries because they come from token edges. Spans never
/// overlap and are ordered by `start`; text no span covers is unhighlighted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightSpan {
    pub start: u32,
    pub end: u32,
    pub kind: HighlightKind,
}

/// Both line numbers are carried so a comment can say which side it means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    /// No leading marker, no trailing newline; the client draws the marker.
    pub text: String,
    /// Empty when the file has no grammar, when the parse failed, or when the
    /// line is plain text. Defaulted so an older daemon still deserializes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<HighlightSpan>,
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

/// Identity of a commit as the history overlay and a commit review title
/// need it. File lists ride on [`HistoryCommit`]; a review of the commit
/// carries the files as [`FileDiff`]s instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitInfo {
    /// Full hex object id.
    pub oid: String,
    /// The usual 7-character abbreviation.
    pub short: String,
    /// First line of the message, empty only for a truly empty one.
    pub summary: String,
    pub author: String,
    /// Committer time, seconds since epoch.
    pub time: i64,
}

impl CommitInfo {
    pub fn title(&self) -> String {
        if self.summary.is_empty() {
            self.short.clone()
        } else {
            format!("{}  {}", self.short, self.summary)
        }
    }
}

/// One path a commit touched, without the hunks — enough for the history
/// overlay to list what changed, cheap enough to send fifty at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitFile {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    /// Counted while summarizing, so the list can show a shape without
    /// carrying the hunks that only a commit review needs.
    pub added: usize,
    pub removed: usize,
}

/// One row of `git log --stat` as the history overlay draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryCommit {
    pub info: CommitInfo,
    pub files: Vec<CommitFile>,
}

/// Carries the checkout id so a client that moved on can drop a stale reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub request_id: u64,
    pub checkout: CheckoutId,
    pub base: ReviewBase,
    pub files: Vec<FileDiff>,
    /// Set when [`ReviewBase::Commit`]: the snapshot this diff is of.
    #[serde(default)]
    pub commit: Option<CommitInfo>,
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
    /// The commit this comment was made against, when `base` is `Commit`.
    #[serde(default)]
    pub commit: Option<String>,
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
    use super::{DiffLine, LineKind, ReviewAnchor, ReviewBase};

    #[test]
    fn review_bases_toggle_between_the_two_sides() {
        assert_eq!(ReviewBase::Unstaged.next(), ReviewBase::Staged);
        assert_eq!(ReviewBase::Staged.next(), ReviewBase::Unstaged);
    }

    #[test]
    fn leaving_a_commit_returns_to_unstaged() {
        assert_eq!(ReviewBase::Commit.next(), ReviewBase::Unstaged);
        assert_eq!(ReviewBase::Commit.label(), "commit");
    }

    #[test]
    fn a_notification_uses_the_new_side_and_stays_on_one_line() {
        let anchor = ReviewAnchor {
            base: ReviewBase::Unstaged,
            commit: None,
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
    /// The wire gained a field, so a peer that predates it must still parse.
    /// This is what `serde(default)` on `spans` is for, and the only way to
    /// know it holds is to deserialize a payload that genuinely lacks it.
    #[test]
    fn a_diff_line_without_spans_still_deserializes() {
        #[derive(serde::Serialize)]
        struct OldDiffLine {
            kind: LineKind,
            old_lineno: Option<u32>,
            new_lineno: Option<u32>,
            text: String,
        }

        let old = OldDiffLine {
            kind: LineKind::Added,
            old_lineno: None,
            new_lineno: Some(7),
            text: "let x = 1;".to_string(),
        };
        let bytes = rmp_serde::to_vec_named(&old).unwrap();
        let line: DiffLine = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(line.new_lineno, Some(7));
        assert_eq!(line.text, "let x = 1;");
        assert!(line.spans.is_empty(), "an absent field reads as no highlighting");
    }
}
