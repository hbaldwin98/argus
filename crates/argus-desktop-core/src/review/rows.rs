//! A review's diff as the page draws it: one file's rows with the cursor and
//! the pick marked, each line's highlighting cut into pieces, the files'
//! list entries, and what the picked lines are called.

use argus_protocol::review::view::{fold_of, marker, ReviewView, Row, FOLD_STEP};
use argus_protocol::{ChangeKind, DiffLine, HighlightKind, HighlightSpan, LineKind};

use crate::connection::page::{DiffRow, DiffSide, ReviewFile};

/// The rows of one file of `view`, the cursor's marked.
pub fn file_rows(view: &ReviewView, file: usize, cursor: Option<usize>, picked: impl Fn(usize) -> bool) -> Vec<DiffRow> {
    view.rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.file() == file && !matches!(r, Row::File { .. }))
        .map(|(i, r)| diff_row(view, i, *r, Some(i) == cursor, picked(i)))
        .collect()
}

pub fn files_of(view: &ReviewView) -> Vec<ReviewFile> {
    view.review.files.iter().map(file).collect()
}

pub fn file(diff: &argus_protocol::FileDiff) -> ReviewFile {
    let (dir, name) = match diff.path.rfind('/') {
        Some(at) => (diff.path[..=at].to_owned(), diff.path[at + 1..].to_owned()),
        None => (String::new(), diff.path.clone()),
    };
    ReviewFile {
        path: diff.path.clone(),
        dir,
        name,
        added: diff.added_lines(),
        removed: diff.removed_lines(),
        kind: match diff.kind {
            ChangeKind::Added => "added",
            ChangeKind::Modified => "modified",
            ChangeKind::Deleted => "deleted",
            ChangeKind::Renamed => "renamed",
            ChangeKind::Untracked => "untracked",
        },
    }
}

fn diff_row(view: &ReviewView, index: usize, row: Row, cursor: bool, picked: bool) -> DiffRow {
    let files = &view.review.files;
    let line = |file: usize, hunk: usize, line: usize| side(&files[file].hunks[hunk].lines[line]);
    if let Row::Fold { file, gap } = row {
        let hidden = fold_of(&files[file], gap).map_or(0, |f| f.hidden());
        let lines = if hidden == 1 { "line" } else { "lines" };
        return DiffRow {
            row: index,
            kind: "fold",
            text: format!("{hidden} {lines} not in the diff"),
            hidden: Some(hidden),
            step: (hidden > FOLD_STEP).then_some(FOLD_STEP),
            left: None,
            right: None,
            cursor,
            picked: false,
        };
    }
    let (kind, text, left, right) = match row {
        Row::Hunk { file, hunk } => ("hunk", files[file].hunks[hunk].header.clone(), None, None),
        Row::Note { file } => ("note", files[file].note.clone().unwrap_or_default(), None, None),
        Row::Line { file, hunk, line: l } => ("line", String::new(), Some(line(file, hunk, l)), None),
        Row::Pair { file, hunk, left, right } => (
            "pair",
            String::new(),
            left.map(|l| line(file, hunk, l)),
            right.map(|l| line(file, hunk, l)),
        ),
        Row::File { .. } | Row::Fold { .. } => ("hunk", String::new(), None, None),
    };
    DiffRow { row: index, kind, text, hidden: None, step: None, left, right, cursor, picked }
}

/// A line with its highlighting cut into pieces here, where the byte
/// offsets the daemon sends mean what they say: the page counts in UTF-16.
fn side(l: &DiffLine) -> DiffSide {
    let parts = parts(&l.text, &l.spans);
    DiffSide {
        old: l.old_lineno,
        new: l.new_lineno,
        marker: match marker(l.kind) {
            '+' => "+",
            '-' => "−",
            _ => " ",
        },
        kind: match l.kind {
            LineKind::Added => "added",
            LineKind::Removed => "removed",
            LineKind::Context => "context",
        },
        parts,
    }
}

/// A line's text cut at its highlighting, as `[text, highlight]`. A span
/// that does not land on character boundaries inside the text is dropped:
/// colour is not worth a wrong slice.
pub fn parts(text: &str, spans: &[HighlightSpan]) -> Vec<(String, &'static str)> {
    let mut parts = Vec::new();
    let mut at = 0usize;
    for span in spans {
        let (start, end) = (span.start as usize, span.end as usize);
        if start < at || end > text.len() || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            continue;
        }
        if start > at {
            parts.push((text[at..start].to_owned(), ""));
        }
        parts.push((text[start..end].to_owned(), highlight(span.kind)));
        at = end;
    }
    if at < text.len() {
        parts.push((text[at..].to_owned(), ""));
    }
    parts
}

fn highlight(kind: HighlightKind) -> &'static str {
    match kind {
        HighlightKind::Keyword => "k",
        HighlightKind::Str => "s",
        HighlightKind::Comment => "c",
        HighlightKind::Number | HighlightKind::Constant => "n",
        HighlightKind::Type => "t",
        HighlightKind::Function => "f",
        HighlightKind::Property => "p",
        HighlightKind::Operator | HighlightKind::Punctuation => "o",
    }
}

/// What the picked lines are, as the header and the comment box say it:
/// `new lines 39–40`, or `old line 12` for a removal.
pub fn pick_label(anchor: &argus_protocol::ReviewAnchor) -> String {
    let range = |side: &str, start: u32, end: u32| {
        if start == end {
            format!("{side} line {start}")
        } else {
            format!("{side} lines {start}–{end}")
        }
    };
    match (anchor.new_start, anchor.new_end, anchor.old_start, anchor.old_end) {
        (Some(s), Some(e), _, _) => range("new", s, e),
        (_, _, Some(s), Some(e)) => range("old", s, e),
        _ => anchor.path.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighting_is_cut_into_pieces_on_character_boundaries() {
        let mut l = DiffLine { kind: LineKind::Added, old_lineno: None, new_lineno: Some(1), text: "let é = 1;".into(), spans: Vec::new() };
        l.spans = vec![
            HighlightSpan { start: 0, end: 3, kind: HighlightKind::Keyword },
            HighlightSpan { start: 9, end: 10, kind: HighlightKind::Number },
        ];
        let parts = side(&l).parts;
        assert_eq!(parts, [("let".into(), "k"), (" é = ".into(), ""), ("1".into(), "n"), (";".into(), "")]);
    }
}
