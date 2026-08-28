//! A checkout's diff flattened into the rows the cursor moves over
//! (DESIGN.md §9 M4). Comments anchor to lines, so only lines are
//! selectable — moving down from a file's last line lands on the next
//! file's first, never on a header you'd have to skip by hand.
//!
//! The same diff flattens two ways. Unified gives every diff line a row of
//! its own; split pairs a hunk's removed lines against its added ones so
//! each row holds both sides. Only the flattening differs — navigation,
//! selection, and anchoring run over whichever rows came out, which is why
//! a comment written in one view means the same thing in the other.

use argus_protocol::{FileDiff, LineKind, Review, ReviewAnchor};

/// Headers and notes are drawn but never selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    File {
        file: usize,
    },
    Hunk {
        file: usize,
        hunk: usize,
    },
    Line {
        file: usize,
        hunk: usize,
        line: usize,
    },
    /// One row of a split view: the removed side and the added side of the
    /// same change, either of which may be absent where a run of one is
    /// longer than the run it replaced. A context line stands on both
    /// sides and carries the same index twice.
    Pair {
        file: usize,
        hunk: usize,
        left: Option<usize>,
        right: Option<usize>,
    },
    /// Stands in for a binary or oversized file, which would otherwise
    /// look unchanged.
    Note {
        file: usize,
    },
}

impl Row {
    pub fn file(self) -> usize {
        match self {
            Row::File { file }
            | Row::Hunk { file, .. }
            | Row::Line { file, .. }
            | Row::Pair { file, .. }
            | Row::Note { file } => file,
        }
    }

    /// Whether the cursor stops here. Headers and notes cannot take a
    /// comment, so they are drawn and skipped over.
    pub fn is_selectable(self) -> bool {
        matches!(self, Row::Line { .. } | Row::Pair { .. })
    }

    /// The diff lines this row covers, removed side first, in the order a
    /// comment quotes them. A context line sits on both sides of a split
    /// row but is still one line, so it is yielded once.
    pub fn lines(self) -> impl Iterator<Item = (usize, usize, usize)> {
        let (file, hunk, first, second) = match self {
            Row::Line { file, hunk, line } => (file, hunk, Some(line), None),
            Row::Pair {
                file,
                hunk,
                left,
                right,
            } => (file, hunk, left, if right == left { None } else { right }),
            _ => (0, 0, None, None),
        };
        first
            .into_iter()
            .chain(second)
            .map(move |line| (file, hunk, line))
    }
}

pub struct ReviewView {
    pub review: Review,
    pub rows: Vec<Row>,
    pub sel: usize,
    pub top: usize,
    /// The other end of a `v` selection.
    pub mark: Option<usize>,
    /// Whether the rows pair the two sides against each other rather than
    /// stacking them.
    pub split: bool,
}

impl ReviewView {
    pub fn new(review: Review, split: bool) -> Self {
        let rows = flatten(&review.files, split);
        let sel = rows.iter().position(|r| r.is_selectable()).unwrap_or(0);
        ReviewView {
            review,
            rows,
            sel,
            top: 0,
            mark: None,
            split,
        }
    }

    /// Reflattens the same diff the other way, keeping the cursor on the
    /// line it was on. A half-made range is dropped: the rows under it are
    /// not the rows it was drawn over, so extending it would silently mean
    /// something else.
    pub fn set_split(&mut self, split: bool) {
        if split == self.split {
            return;
        }
        let was = self.rows.get(self.sel).and_then(|r| r.lines().next());
        self.split = split;
        self.rows = flatten(&self.review.files, split);
        self.mark = None;
        self.sel = was
            .and_then(|at| self.rows.iter().position(|r| r.lines().any(|l| l == at)))
            .or_else(|| self.rows.iter().position(|r| r.is_selectable()))
            .unwrap_or(0);
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Clamps rather than wraps; wrapping in a long diff loses your place.
    pub fn move_by(&mut self, delta: isize) {
        let marked_file = self
            .mark
            .and_then(|mark| self.rows.get(mark))
            .map(|row| row.file());
        let selectable: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.is_selectable() && marked_file.is_none_or(|file| r.file() == file))
            .map(|(i, _)| i)
            .collect();
        if selectable.is_empty() {
            return;
        }
        let here = selectable
            .iter()
            .position(|&i| i >= self.sel)
            .unwrap_or(selectable.len() - 1) as isize;
        let next = (here + delta).clamp(0, selectable.len() as isize - 1) as usize;
        self.sel = selectable[next];
    }

    /// In a diff of any size most movement is between files, not within one.
    pub fn jump_file(&mut self, forward: bool) {
        let here = self.rows.get(self.sel).map(|r| r.file());
        let candidates: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.is_selectable() && Some(r.file()) != here)
            .map(|(i, _)| i)
            .collect();
        let next = if forward {
            candidates.iter().find(|&&i| i > self.sel).copied()
        } else {
            // The previous file's first row, so `[` inverts `]`.
            candidates
                .iter()
                .rev()
                .find(|&&i| i < self.sel)
                .map(|&i| self.rows[i].file())
                .and_then(|f| {
                    self.rows
                        .iter()
                        .position(|r| r.is_selectable() && r.file() == f)
                })
        };
        if let Some(i) = next {
            self.sel = i;
            self.mark = None;
        }
    }

    /// Puts the cursor on the first line of file `file`.
    pub fn jump_to_file(&mut self, file: usize) {
        if let Some(i) = self
            .rows
            .iter()
            .position(|r| r.is_selectable() && r.file() == file)
        {
            self.sel = i;
            self.mark = None;
        }
    }

    pub fn top_of_diff(&mut self) {
        self.sel = self
            .rows
            .iter()
            .position(|r| r.is_selectable())
            .unwrap_or(0);
        self.mark = None;
    }

    pub fn bottom_of_diff(&mut self) {
        self.sel = self
            .rows
            .iter()
            .rposition(|r| r.is_selectable())
            .unwrap_or(0);
        self.mark = None;
    }

    pub fn toggle_mark(&mut self) {
        self.mark = match self.mark {
            Some(_) => None,
            None => Some(self.sel),
        };
    }

    /// Ascending, and one row unless `v` extended it.
    pub fn selection(&self) -> (usize, usize) {
        match self.mark {
            Some(m) if m < self.sel => (m, self.sel),
            Some(m) => (self.sel, m),
            None => (self.sel, self.sel),
        }
    }

    /// Scrolls as little as it can to keep the cursor on screen.
    pub fn scroll_into_view(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + height {
            self.top = self.sel + 1 - height;
        }
        let max_top = self.rows.len().saturating_sub(height);
        self.top = self.top.min(max_top);
    }

    /// What a comment written now would attach to.
    pub fn anchor(&self) -> Option<ReviewAnchor> {
        if self.rows.is_empty() {
            return None;
        }
        let (from, to) = self.selection();
        let rows: Vec<&Row> = self.rows[from..=to.min(self.rows.len() - 1)]
            .iter()
            .filter(|r| r.is_selectable())
            .collect();
        let first = rows.first()?;
        let file = &self.review.files[first.file()];
        let path = file.path.clone();
        let old_path = file.old_path.clone();

        let mut text = Vec::new();
        let (mut old_start, mut old_end) = (None, None);
        let (mut new_start, mut new_end) = (None, None);
        'rows: for row in &rows {
            for (file, hunk, line) in row.lines() {
                if file != first.file() {
                    break 'rows;
                }
                let l = &self.review.files[file].hunks[hunk].lines[line];
                if old_start.is_none() {
                    old_start = l.old_lineno;
                }
                if l.old_lineno.is_some() {
                    old_end = l.old_lineno;
                }
                if new_start.is_none() {
                    new_start = l.new_lineno;
                }
                if l.new_lineno.is_some() {
                    new_end = l.new_lineno;
                }
                text.push(format!("{}{}", marker(l.kind), l.text));
            }
        }

        Some(ReviewAnchor {
            base: self.review.base,
            commit: self.review.commit.as_ref().map(|c| c.oid.clone()),
            path,
            old_path,
            old_start,
            old_end,
            new_start,
            new_end,
            text,
        })
    }
}

pub fn marker(kind: LineKind) -> char {
    match kind {
        LineKind::Added => '+',
        LineKind::Removed => '-',
        LineKind::Context => ' ',
    }
}

/// Walks the files once, letting `body` fill in the rows for each hunk.
/// Both views agree on everything outside a hunk, and an unrendered file is
/// a note either way.
fn flatten(files: &[FileDiff], split: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    for (f, file) in files.iter().enumerate() {
        rows.push(Row::File { file: f });
        if file.note.is_some() {
            rows.push(Row::Note { file: f });
            continue;
        }
        for (h, hunk) in file.hunks.iter().enumerate() {
            rows.push(Row::Hunk { file: f, hunk: h });
            if split {
                paired(&mut rows, f, h, hunk);
            } else {
                rows.extend((0..hunk.lines.len()).map(|l| Row::Line {
                    file: f,
                    hunk: h,
                    line: l,
                }));
            }
        }
    }
    rows
}

/// Pairs a hunk's two sides the way git's own hunks are shaped: a run of
/// removals and the run of additions that replaced them belong to the same
/// change, and a context line ends both runs because it is where the two
/// sides are known to line up again.
fn paired(rows: &mut Vec<Row>, file: usize, hunk: usize, diff: &argus_protocol::Hunk) {
    let (mut removed, mut added): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    let flush = |rows: &mut Vec<Row>, removed: &mut Vec<usize>, added: &mut Vec<usize>| {
        for i in 0..removed.len().max(added.len()) {
            rows.push(Row::Pair {
                file,
                hunk,
                left: removed.get(i).copied(),
                right: added.get(i).copied(),
            });
        }
        removed.clear();
        added.clear();
    };
    for (i, line) in diff.lines.iter().enumerate() {
        match line.kind {
            LineKind::Removed => removed.push(i),
            LineKind::Added => added.push(i),
            LineKind::Context => {
                flush(rows, &mut removed, &mut added);
                rows.push(Row::Pair {
                    file,
                    hunk,
                    left: Some(i),
                    right: Some(i),
                });
            }
        }
    }
    flush(rows, &mut removed, &mut added);
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{ChangeKind, CheckoutId, DiffLine, Hunk};

    fn line(kind: LineKind, no: u32, text: &str) -> DiffLine {
        let (old_lineno, new_lineno) = match kind {
            LineKind::Added => (None, Some(no)),
            LineKind::Removed => (Some(no), None),
            LineKind::Context => (Some(no), Some(no)),
        };
        DiffLine {
            kind,
            old_lineno,
            new_lineno,
            text: text.to_string(),
            spans: Vec::new(),
        }
    }

    fn file(path: &str, lines: Vec<DiffLine>) -> FileDiff {
        FileDiff {
            path: path.to_string(),
            old_path: None,
            kind: ChangeKind::Modified,
            hunks: vec![Hunk {
                header: "@@ -1,3 +1,3 @@".to_string(),
                lines,
            }],
            note: None,
        }
    }

    fn view(files: Vec<FileDiff>) -> ReviewView {
        split_view(files, false)
    }

    fn split_view(files: Vec<FileDiff>, split: bool) -> ReviewView {
        ReviewView::new(
            Review {
                request_id: 1,
                checkout: CheckoutId(1),
                base: argus_protocol::ReviewBase::Unstaged,
                files,
                commit: None,
            },
            split,
        )
    }

    fn two_files() -> ReviewView {
        view(vec![
            file(
                "a.rs",
                vec![
                    line(LineKind::Context, 1, "one"),
                    line(LineKind::Added, 2, "two"),
                ],
            ),
            file(
                "b.rs",
                vec![
                    line(LineKind::Removed, 1, "old"),
                    line(LineKind::Added, 1, "new"),
                ],
            ),
        ])
    }

    #[test]
    fn a_diff_flattens_to_headers_and_lines() {
        let v = two_files();
        assert_eq!(
            v.rows.len(),
            2 * (1 + 1 + 2),
            "a file header, a hunk header and two lines, twice"
        );
        assert!(matches!(v.rows[0], Row::File { .. }));
        assert!(matches!(v.rows[1], Row::Hunk { .. }));
    }

    #[test]
    fn the_cursor_starts_on_the_first_line_not_a_header() {
        // A header can't take a comment, so starting there would mean every
        // session opens with a keystroke of pure ceremony.
        let v = two_files();
        assert!(v.rows[v.sel].is_selectable());
    }

    #[test]
    fn moving_down_skips_the_headers_between_files() {
        let mut v = two_files();
        v.move_by(1); // second line of a.rs
        v.move_by(1); // should be the first line of b.rs
        let Row::Line { file, line, .. } = v.rows[v.sel] else {
            panic!("not a line: {:?}", v.rows[v.sel]);
        };
        assert_eq!((file, line), (1, 0));
    }

    #[test]
    fn the_cursor_clamps_instead_of_wrapping() {
        // Wrapping in a long diff silently loses your place.
        let mut v = two_files();
        v.move_by(-5);
        assert_eq!(
            v.rows[v.sel],
            Row::Line {
                file: 0,
                hunk: 0,
                line: 0
            }
        );
        v.move_by(500);
        assert_eq!(
            v.rows[v.sel],
            Row::Line {
                file: 1,
                hunk: 0,
                line: 1
            }
        );
    }

    #[test]
    fn jumping_forward_lands_on_the_next_files_first_line() {
        let mut v = two_files();
        v.jump_file(true);
        assert_eq!(
            v.rows[v.sel],
            Row::Line {
                file: 1,
                hunk: 0,
                line: 0
            }
        );
    }

    #[test]
    fn jumping_back_lands_on_the_previous_files_first_line_not_its_last() {
        // `[` should land where `]` would have, so the two are inverses.
        let mut v = two_files();
        v.jump_file(true);
        v.move_by(1);
        v.jump_file(false);
        assert_eq!(
            v.rows[v.sel],
            Row::Line {
                file: 0,
                hunk: 0,
                line: 0
            }
        );
    }

    #[test]
    fn jumping_past_the_last_file_stays_put() {
        let mut v = two_files();
        v.jump_file(true);
        let was = v.sel;
        v.jump_file(true);
        assert_eq!(v.sel, was);
    }

    #[test]
    fn g_and_shift_g_reach_both_ends() {
        let mut v = two_files();
        v.bottom_of_diff();
        assert_eq!(
            v.rows[v.sel],
            Row::Line {
                file: 1,
                hunk: 0,
                line: 1
            }
        );
        v.top_of_diff();
        assert_eq!(
            v.rows[v.sel],
            Row::Line {
                file: 0,
                hunk: 0,
                line: 0
            }
        );
    }

    #[test]
    fn an_unrendered_file_still_gets_a_row_of_its_own() {
        // Otherwise a binary file reads as an unchanged one.
        let mut f = file("blob.bin", Vec::new());
        f.hunks.clear();
        f.note = Some("binary file".to_string());
        let v = view(vec![f]);
        assert_eq!(v.rows, vec![Row::File { file: 0 }, Row::Note { file: 0 }]);
        assert!(v.anchor().is_none(), "and it can't take a comment");
    }

    #[test]
    fn an_empty_review_navigates_without_panicking() {
        let mut v = view(Vec::new());
        assert!(v.is_empty());
        v.move_by(1);
        v.jump_file(true);
        v.bottom_of_diff();
        assert!(v.anchor().is_none());
    }

    // --- the split view -----------------------------------------------------

    fn split_two_files() -> ReviewView {
        let mut v = two_files();
        v.set_split(true);
        v
    }

    #[test]
    fn a_split_row_holds_a_removal_and_what_replaced_it() {
        // b.rs is one line swapped for another: side by side that is one
        // row, not two.
        let v = split_two_files();
        assert_eq!(
            v.rows[v.rows.len() - 1],
            Row::Pair {
                file: 1,
                hunk: 0,
                left: Some(0),
                right: Some(1)
            }
        );
    }

    #[test]
    fn a_context_line_stands_on_both_sides_at_once() {
        // It is unchanged, so both columns show it and the two sides stay
        // in step from there on.
        let v = split_two_files();
        assert_eq!(
            v.rows[2],
            Row::Pair {
                file: 0,
                hunk: 0,
                left: Some(0),
                right: Some(0)
            }
        );
    }

    #[test]
    fn a_line_with_no_counterpart_leaves_the_other_side_empty() {
        // a.rs only adds; nothing was removed to put opposite it.
        let v = split_two_files();
        assert_eq!(
            v.rows[3],
            Row::Pair {
                file: 0,
                hunk: 0,
                left: None,
                right: Some(1)
            }
        );
    }

    #[test]
    fn uneven_runs_pair_as_far_as_they_go_and_then_hang() {
        // Three lines replaced by one: the first pairs, the rest have no
        // added line to sit against, and none of them may be dropped.
        let v = split_view(
            vec![file(
                "a.rs",
                vec![
                    line(LineKind::Removed, 1, "one"),
                    line(LineKind::Removed, 2, "two"),
                    line(LineKind::Removed, 3, "three"),
                    line(LineKind::Added, 1, "only"),
                ],
            )],
            true,
        );
        let pairs: Vec<Row> = v
            .rows
            .iter()
            .copied()
            .filter(|r| r.is_selectable())
            .collect();
        assert_eq!(
            pairs,
            vec![
                Row::Pair {
                    file: 0,
                    hunk: 0,
                    left: Some(0),
                    right: Some(3)
                },
                Row::Pair {
                    file: 0,
                    hunk: 0,
                    left: Some(1),
                    right: None
                },
                Row::Pair {
                    file: 0,
                    hunk: 0,
                    left: Some(2),
                    right: None
                },
            ]
        );
    }

    #[test]
    fn both_views_draw_every_line_of_the_diff() {
        // A row model that quietly loses a line is worse than no split
        // view at all.
        let unified = two_files();
        let split = split_two_files();
        let seen = |v: &ReviewView| {
            let mut all: Vec<(usize, usize, usize)> =
                v.rows.iter().flat_map(|r| r.lines()).collect();
            all.sort();
            all.dedup();
            all
        };
        assert_eq!(seen(&unified), seen(&split));
    }

    #[test]
    fn headers_are_still_headers_when_the_diff_is_split() {
        let v = split_two_files();
        assert!(matches!(v.rows[0], Row::File { .. }));
        assert!(matches!(v.rows[1], Row::Hunk { .. }));
        assert!(
            v.rows[v.sel].is_selectable(),
            "and the cursor starts on a line"
        );
    }

    #[test]
    fn moving_still_skips_the_headers_between_files() {
        let mut v = split_two_files();
        v.move_by(1); // the added line of a.rs
        v.move_by(1); // over two headers, into b.rs
        assert_eq!(v.rows[v.sel].file(), 1);
    }

    #[test]
    fn toggling_the_view_keeps_the_cursor_on_the_line_it_was_on() {
        // Flipping the layout is a way of looking at the diff, not a way of
        // losing your place in it.
        let mut v = two_files();
        v.jump_file(true);
        v.move_by(1); // b.rs's added line
        assert_eq!(
            v.rows[v.sel],
            Row::Line {
                file: 1,
                hunk: 0,
                line: 1
            }
        );
        v.set_split(true);
        assert!(
            v.rows[v.sel].lines().any(|l| l == (1, 0, 1)),
            "the row now holding that line: {:?}",
            v.rows[v.sel]
        );
    }

    #[test]
    fn toggling_back_lands_on_the_split_rows_first_line() {
        let mut v = split_two_files();
        v.bottom_of_diff();
        v.set_split(false);
        assert_eq!(
            v.rows[v.sel],
            Row::Line {
                file: 1,
                hunk: 0,
                line: 0
            },
            "the removed side, which is the row's first line"
        );
    }

    #[test]
    fn toggling_the_view_drops_a_half_made_range() {
        // The rows under a mark are not the rows it was drawn over, so
        // extending it afterwards would silently mean something else.
        let mut v = two_files();
        v.toggle_mark();
        v.set_split(true);
        assert!(v.mark.is_none());
    }

    #[test]
    fn setting_the_view_it_is_already_in_changes_nothing() {
        let mut v = two_files();
        v.move_by(1);
        v.toggle_mark();
        let (sel, mark) = (v.sel, v.mark);
        v.set_split(false);
        assert_eq!((v.sel, v.mark), (sel, mark));
    }

    #[test]
    fn a_comment_on_a_split_row_carries_both_sides() {
        // The row shows one line becoming another, so the anchor spans both
        // numbers and quotes both — the same anchor the unified view gives
        // for the two lines selected together.
        let mut v = split_two_files();
        v.bottom_of_diff();
        let a = v.anchor().unwrap();
        assert_eq!(a.path, "b.rs");
        assert_eq!(a.text, vec!["-old", "+new"]);
        assert_eq!((a.old_start, a.old_end), (Some(1), Some(1)));
        assert_eq!((a.new_start, a.new_end), (Some(1), Some(1)));
    }

    #[test]
    fn a_comment_on_a_context_row_quotes_it_once_not_twice() {
        // It stands on both sides, but it is still one line of the file.
        let mut v = split_two_files();
        v.top_of_diff();
        let a = v.anchor().unwrap();
        assert_eq!(a.text, vec![" one"]);
    }

    #[test]
    fn the_two_views_anchor_the_same_change_the_same_way() {
        let mut unified = two_files();
        unified.jump_file(true);
        unified.toggle_mark();
        unified.move_by(1);

        let mut split = split_two_files();
        split.bottom_of_diff();

        assert_eq!(unified.anchor(), split.anchor());
    }

    // --- anchoring ----------------------------------------------------------

    #[test]
    fn a_comment_on_one_line_carries_its_path_number_and_text() {
        let mut v = two_files();
        v.move_by(1); // the added line
        let a = v.anchor().unwrap();
        assert_eq!(a.path, "a.rs");
        assert_eq!((a.new_start, a.new_end), (Some(2), Some(2)));
        assert_eq!((a.old_start, a.old_end), (None, None));
        assert_eq!(a.text, vec!["+two"], "marker included, as git shows it");
    }

    #[test]
    fn a_removed_line_is_anchored_by_its_old_number() {
        let mut v = two_files();
        v.jump_file(true); // b.rs, the removed line
        let a = v.anchor().unwrap();
        assert_eq!(a.old_start, Some(1));
        assert_eq!(a.new_start, None);
        assert_eq!(a.text, vec!["-old"]);
    }

    #[test]
    fn v_extends_the_selection_over_a_range() {
        let mut v = two_files();
        v.toggle_mark();
        v.move_by(1);
        assert_eq!(v.selection(), (v.sel - 1, v.sel));
        let a = v.anchor().unwrap();
        assert_eq!(a.text, vec![" one", "+two"]);
        assert_eq!((a.new_start, a.new_end), (Some(1), Some(2)));
        assert_eq!((a.old_start, a.old_end), (Some(1), Some(1)));
    }

    #[test]
    fn a_range_selected_upwards_reads_the_same_way_round() {
        let mut v = two_files();
        v.move_by(1);
        v.toggle_mark();
        v.move_by(-1);
        let a = v.anchor().unwrap();
        assert_eq!(a.text, vec![" one", "+two"], "always top to bottom");
    }

    #[test]
    fn pressing_v_again_drops_back_to_a_single_line() {
        let mut v = two_files();
        v.toggle_mark();
        v.move_by(1);
        v.toggle_mark();
        assert_eq!(v.selection(), (v.sel, v.sel));
    }

    #[test]
    fn a_range_stops_at_the_end_of_its_file() {
        let mut v = two_files();
        v.move_by(1);
        v.toggle_mark();
        v.move_by(1);
        let a = v.anchor().unwrap();
        assert_eq!(a.path, "a.rs");
        assert_eq!(a.text, vec!["+two"]);
    }

    #[test]
    fn jumping_files_drops_a_half_made_range() {
        // A range left open across a jump would silently span the whole gap.
        let mut v = two_files();
        v.toggle_mark();
        v.jump_file(true);
        assert!(v.mark.is_none());
    }

    // --- scrolling ----------------------------------------------------------

    #[test]
    fn the_viewport_follows_the_cursor_down_and_back_up() {
        let mut v = two_files();
        v.bottom_of_diff();
        v.scroll_into_view(3);
        assert_eq!(v.top, v.sel + 1 - 3);

        v.top_of_diff();
        v.scroll_into_view(3);
        assert!(
            (v.top..v.top + 3).contains(&v.sel),
            "scrolls the least it can, but the cursor is on screen"
        );
    }

    #[test]
    fn a_diff_shorter_than_the_viewport_never_scrolls() {
        let mut v = two_files();
        v.bottom_of_diff();
        v.scroll_into_view(100);
        assert_eq!(v.top, 0);
    }

    #[test]
    fn a_zero_height_viewport_is_not_a_division_by_anything() {
        let mut v = two_files();
        v.scroll_into_view(0);
        assert_eq!(v.top, 0);
    }
    // --- the message sent to the agent -------------------------------------

    #[test]
    fn a_single_line_comment_names_the_place_and_quotes_the_line() {
        let mut v = two_files();
        v.move_by(1);
        let m = v.anchor().unwrap().notification("this should be two");
        assert_eq!(m, "a.rs:2 `+two`: this should be two");
    }

    #[test]
    fn a_range_comment_gives_the_span_and_how_many_lines() {
        // Quoting several lines would be unreadable inline, and the agent
        // can read the file; what it can't guess is which lines you meant.
        let mut v = two_files();
        v.toggle_mark();
        v.move_by(1);
        let m = v.anchor().unwrap().notification("rework this");
        assert_eq!(m, "a.rs:1-2 (2 lines): rework this");
    }

    #[test]
    fn the_message_is_always_one_line() {
        // It is typed into a harness's prompt; a newline would submit it
        // half-written.
        let mut v = two_files();
        v.toggle_mark();
        v.move_by(1);
        let m = v
            .anchor()
            .unwrap()
            .notification("first thought\nsecond thought");
        assert!(!m.contains('\n'), "{m:?}");
    }
}
