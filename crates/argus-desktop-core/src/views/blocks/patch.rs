//! A diff view block's unified diff, read into the review viewer's own rows, so a diff an agent
//! wrote looks as every other diff does and the diff view stays the one place that knows how a line
//! is laid out (PATCH-1 to PATCH-3).
//!
//! The page's `views/blocks/patch.ts`. Counts in a hunk header say where the hunk ends, which is how
//! a removed line that reads `-- x` is told from the next file's `--- a/x`. Text with no hunk at
//! all is not a diff and is `None`: the block is then drawn as a `diff` code block. A hunk cut short
//! is still read.

use crate::connection::page::{DiffRow, DiffSide};

/// A diff read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    pub rows: Vec<DiffRow>,
    pub added: usize,
    pub removed: usize,
    /// The new file's name from a `+++` line, for a diff given no `path`.
    pub file: Option<String>,
}

fn side(kind: &'static str, marker: &'static str, text: &str, old: Option<u32>, new: Option<u32>) -> DiffSide {
    DiffSide { old, new, marker, kind, parts: vec![(text.to_owned(), "")] }
}

fn push(rows: &mut Vec<DiffRow>, kind: &'static str, text: &str, left: Option<DiffSide>) {
    rows.push(DiffRow { row: rows.len(), kind, text: text.to_owned(), hidden: None, step: None, left, right: None, cursor: false, picked: false });
}

/// `@@ -a[,b] +c[,d] @@` as (old start, old count, new start, new count); a count left out is 1.
fn hunk(line: &str) -> Option<(u32, u32, u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let range = |text: &str| -> Option<(u32, u32)> {
        match text.split_once(',') {
            Some((start, count)) => Some((number(start)?, number(count)?)),
            None => Some((number(text)?, 1)),
        }
    };
    let ((old_start, old_count), (new_start, new_count)) = (range(old)?, range(new)?);
    Some((old_start, old_count, new_start, new_count))
}

/// Digits only, as a regular expression's `\d+` reads them; a count too big for a line number is no hunk.
fn number(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// The diff's rows, or `None` when `text` holds no hunk to draw.
pub fn parse_patch(text: &str) -> Option<Patch> {
    let text = text.replace("\r\n", "\n");
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut out = Patch { rows: Vec::new(), added: 0, removed: 0, file: None };
    let (mut hunks, mut old_left, mut new_left, mut old_no, mut new_no) = (0, 0u32, 0u32, 0u32, 0u32);
    for line in lines {
        if old_left > 0 || new_left > 0 {
            let mut chars = line.chars();
            let lead = chars.next();
            let body = chars.as_str();
            match lead {
                Some('\\') => push(&mut out.rows, "note", line, None),
                Some('+') if new_left > 0 => {
                    push(&mut out.rows, "line", "", Some(side("added", "+", body, None, Some(new_no))));
                    new_no += 1;
                    new_left -= 1;
                    out.added += 1;
                }
                Some('-') if old_left > 0 => {
                    push(&mut out.rows, "line", "", Some(side("removed", "-", body, Some(old_no), None)));
                    old_no += 1;
                    old_left -= 1;
                    out.removed += 1;
                }
                Some(' ') | None if old_left > 0 && new_left > 0 => {
                    push(&mut out.rows, "line", "", Some(side("context", "", body, Some(old_no), Some(new_no))));
                    old_no += 1;
                    new_no += 1;
                    old_left -= 1;
                    new_left -= 1;
                }
                _ => return None,
            }
            continue;
        }
        if let Some((old_start, old_count, new_start, new_count)) = hunk(line) {
            hunks += 1;
            (old_no, new_no, old_left, new_left) = (old_start, new_start, old_count, new_count);
            push(&mut out.rows, "hunk", line, None);
        } else if let Some(name) = line.strip_prefix("+++ ") {
            let name = name.split('\t').next().unwrap_or_default();
            if out.file.is_none() && name != "/dev/null" {
                out.file = Some(name.strip_prefix("b/").unwrap_or(name).to_owned());
            }
        } else if line.starts_with("diff ") || line.starts_with('\\') {
            push(&mut out.rows, "note", line, None);
        }
        // `index`, `---`, mode and rename lines say nothing the header does not.
    }
    (hunks > 0).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: &str = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n";

    fn kinds(patch: &Patch) -> Vec<(&'static str, Option<&'static str>)> {
        patch.rows.iter().map(|row| (row.kind, row.left.as_ref().map(|side| side.kind))).collect()
    }

    // PATCH-1, PATCH-2: a diff is read into rows, numbered on the side or sides a line is on.
    #[test]
    fn a_unified_diff_is_read_into_rows_numbered_by_the_hunk_header() {
        let patch = parse_patch(ONE).expect("a diff");
        assert_eq!(
            kinds(&patch),
            [("note", None), ("hunk", None), ("line", Some("context")), ("line", Some("removed")), ("line", Some("added")), ("line", Some("context"))]
        );
        let numbers: Vec<_> = patch.rows.iter().filter_map(|row| row.left.as_ref()).map(|side| (side.old, side.new)).collect();
        assert_eq!(numbers, [(Some(1), Some(1)), (Some(2), None), (None, Some(2)), (Some(3), Some(3))]);
        assert_eq!((patch.added, patch.removed), (1, 1));
        assert_eq!(patch.file.as_deref(), Some("src/a.rs"), "the +++ name, `b/` taken off");
        assert_eq!(patch.rows.iter().map(|row| row.row).collect::<Vec<_>>(), [0, 1, 2, 3, 4, 5], "a row's number is its place, which is what a click would send");
    }

    // PATCH-2: the counts in the header end the hunk, so `-- x` is a removed line, not a file header.
    #[test]
    fn a_removed_line_that_reads_like_a_file_header_is_a_removed_line_while_the_hunk_has_lines_left() {
        let patch = parse_patch("@@ -1,2 +1,0 @@\n-- x\n--- y\n").expect("two removed lines");
        assert_eq!((patch.removed, patch.added), (2, 0));
        let texts: Vec<_> = patch.rows.iter().filter_map(|row| row.left.as_ref()).map(|side| side.parts[0].0.as_str()).collect();
        assert_eq!(texts, ["- x", "-- y"]);
        // Past the hunk's end the same text is a header and is dropped.
        let patch = parse_patch("@@ -1 +1,0 @@\n-gone\n--- a/next\n+++ b/next\n@@ -5 +5 @@\n-a\n").unwrap();
        assert_eq!((patch.removed, patch.rows.len(), patch.file.as_deref()), (2, 4, Some("next")));
    }

    #[test]
    fn a_count_left_out_of_a_header_is_one_and_an_empty_line_is_a_context_line() {
        let patch = parse_patch("@@ -4 +4 @@\n\n").expect("one context line");
        let side = patch.rows[1].left.as_ref().unwrap();
        assert_eq!((side.kind, side.old, side.new, side.parts[0].0.as_str()), ("context", Some(4), Some(4), ""));
    }

    #[test]
    fn a_no_newline_note_is_a_note_row_inside_a_hunk_and_outside_one() {
        let patch = parse_patch("@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n\\ No newline at end of file\n").unwrap();
        assert_eq!(kinds(&patch).iter().map(|(kind, _)| *kind).collect::<Vec<_>>(), ["hunk", "line", "note", "line", "note"]);
        assert_eq!(patch.rows[2].text, "\\ No newline at end of file");
    }

    #[test]
    fn the_new_file_name_is_cut_at_a_tab_and_a_dev_null_is_no_name() {
        let patch = parse_patch("+++ b/a b.txt\t2026-01-01\n@@ -0,0 +1 @@\n+x\n").unwrap();
        assert_eq!(patch.file.as_deref(), Some("a b.txt"));
        let patch = parse_patch("+++ /dev/null\n+++ b/real\n@@ -1 +0,0 @@\n-x\n").unwrap();
        assert_eq!(patch.file.as_deref(), Some("real"), "the first name that is a file wins");
    }

    // PATCH-1: no hunk is not a diff; a hunk cut short still is.
    #[test]
    fn text_with_no_hunk_is_not_a_diff_and_a_hunk_cut_short_is_read() {
        assert!(parse_patch("").is_none());
        assert!(parse_patch("just some words\nand more\n").is_none());
        assert!(parse_patch("--- a/x\n+++ b/x\n").is_none(), "headers alone hold nothing to draw");
        let cut = parse_patch("@@ -1,5 +1,5 @@\n a\n-b\n").expect("cut short");
        assert_eq!(cut.rows.len(), 3);
        assert!(parse_patch("@@ -1,2 +1,2 @@\n a\nwhat is this\n").is_none(), "a stray line inside a hunk makes the whole text unreadable");
    }

    #[test]
    fn carriage_returns_before_the_newlines_are_not_part_of_a_line() {
        let patch = parse_patch("@@ -1 +1 @@\r\n-a\r\n+b\r\n").unwrap();
        let texts: Vec<_> = patch.rows.iter().filter_map(|row| row.left.as_ref()).map(|side| side.parts[0].0.as_str()).collect();
        assert_eq!(texts, ["a", "b"]);
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            // PATCH-1: text from an agent is read or refused, never a panic, and what is counted is what is drawn.
            #[test]
            fn any_text_is_read_or_refused_and_the_counts_are_the_rows_drawn(lines in proptest::collection::vec("(@@ -[0-9]{1,2}(,[0-9])? \\+[0-9]{1,2}(,[0-9])? @@|[-+ \\\\]?[a-z -]{0,6}|diff x|\\+\\+\\+ b/f)", 0..10), crlf in any::<bool>()) {
                let text = lines.join(if crlf { "\r\n" } else { "\n" });
                if let Some(patch) = parse_patch(&text) {
                    let line = |kind: &str| patch.rows.iter().filter(|row| row.left.as_ref().is_some_and(|side| side.kind == kind)).count();
                    prop_assert_eq!(patch.added, line("added"));
                    prop_assert_eq!(patch.removed, line("removed"));
                    prop_assert!(patch.rows.iter().enumerate().all(|(at, row)| row.row == at));
                    prop_assert!(patch.rows.iter().any(|row| row.kind == "hunk"));
                }
            }
        }
    }
}
