//! What a client may ask git's index of a checkout to take or give back:
//! whole files, or lines picked off a diff it was shown.

use serde::{Deserialize, Serialize};

use crate::review::view::ReviewView;
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

/// The changed lines under `rows` of `view`, as a pick of them names them,
/// with the file they are in: the first row's file, since a pick is of one
/// file and a range dragged past its end means no more than its own lines.
/// None when no row is a changed line.
pub fn picked_lines(view: &ReviewView, rows: &[usize]) -> Option<(String, Vec<PickedLine>)> {
    let files = &view.review.files;
    let file = rows.iter().find_map(|&r| view.rows.get(r)).map(|r| r.file())?;
    let mut picked: Vec<PickedLine> = Vec::new();
    for row in rows.iter().filter_map(|&r| view.rows.get(r)).filter(|r| r.file() == file) {
        for (f, h, l) in row.lines() {
            let line = &files[f].hunks[h].lines[l];
            let lineno = match line.kind {
                LineKind::Added => line.new_lineno,
                LineKind::Removed => line.old_lineno,
                LineKind::Context => None,
            };
            let Some(lineno) = lineno else { continue };
            let pick = PickedLine { kind: line.kind, lineno, text: line.text.clone() };
            if !picked.contains(&pick) {
                picked.push(pick);
            }
        }
    }
    (!picked.is_empty()).then(|| (files[file].path.clone(), picked))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::{DiffLine, FileDiff, Hunk, Review};
    use crate::{ChangeKind, CheckoutId};

    fn line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
        DiffLine { kind, old_lineno: old, new_lineno: new, text: text.into(), spans: Vec::new() }
    }

    fn view(split: bool) -> ReviewView {
        let file = |path: &str| FileDiff {
            path: path.into(),
            old_path: None,
            kind: ChangeKind::Modified,
            hunks: vec![Hunk {
                header: "@@ -1,2 +1,2 @@".into(),
                lines: vec![
                    line(LineKind::Context, Some(1), Some(1), "a"),
                    line(LineKind::Removed, Some(2), None, "b"),
                    line(LineKind::Added, None, Some(2), "B"),
                ],
            }],
            note: None,
            new_blob: None,
            new_lines: None,
        };
        let review = Review {
            request_id: 1,
            checkout: CheckoutId(1),
            base: ReviewBase::Unstaged,
            files: vec![file("a.rs"), file("b.rs")],
            commit: None,
        };
        ReviewView::new(review, split)
    }

    #[test]
    fn the_changed_lines_of_the_rows_are_picked_by_the_number_on_their_own_side() {
        let v = view(false);
        let all: Vec<usize> = (0..v.rows.len()).collect();
        let (path, lines) = picked_lines(&v, &all).unwrap();
        assert_eq!(path, "a.rs", "a pick is of the first row's file");
        assert_eq!(
            lines,
            [
                PickedLine { kind: LineKind::Removed, lineno: 2, text: "b".into() },
                PickedLine { kind: LineKind::Added, lineno: 2, text: "B".into() },
            ]
        );
    }

    #[test]
    fn a_split_row_picks_both_its_sides_and_context_picks_nothing() {
        let v = view(true);
        let pair = v.rows.iter().position(|r| matches!(r, crate::review::view::Row::Pair { left: Some(1), .. })).unwrap();
        assert_eq!(picked_lines(&v, &[pair]).unwrap().1.len(), 2);
        let context = v.rows.iter().position(|r| matches!(r, crate::review::view::Row::Pair { left: Some(0), .. })).unwrap();
        assert_eq!(picked_lines(&v, &[context]), None);
    }
}
