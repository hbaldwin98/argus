//! A checkout's diff flattened into the rows the cursor moves over
//! (DESIGN.md §9 M4). Comments anchor to lines, so only lines are
//! selectable — moving down from a file's last line lands on the next
//! file's first, never on a header you'd have to skip by hand.

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
            | Row::Note { file } => file,
        }
    }

    pub fn is_line(self) -> bool {
        matches!(self, Row::Line { .. })
    }
}

pub struct ReviewView {
    pub review: Review,
    pub rows: Vec<Row>,
    pub sel: usize,
    pub top: usize,
    /// The other end of a `v` selection.
    pub mark: Option<usize>,
}

impl ReviewView {
    pub fn new(review: Review) -> Self {
        let rows = flatten(&review.files);
        let sel = rows.iter().position(|r| r.is_line()).unwrap_or(0);
        ReviewView {
            review,
            rows,
            sel,
            top: 0,
            mark: None,
        }
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
            .filter(|(_, r)| r.is_line() && marked_file.is_none_or(|file| r.file() == file))
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
            .filter(|(_, r)| r.is_line() && Some(r.file()) != here)
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
                .and_then(|f| self.rows.iter().position(|r| r.is_line() && r.file() == f))
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
            .position(|r| r.is_line() && r.file() == file)
        {
            self.sel = i;
            self.mark = None;
        }
    }

    pub fn top_of_diff(&mut self) {
        self.sel = self.rows.iter().position(|r| r.is_line()).unwrap_or(0);
        self.mark = None;
    }

    pub fn bottom_of_diff(&mut self) {
        self.sel = self.rows.iter().rposition(|r| r.is_line()).unwrap_or(0);
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
            .filter(|r| r.is_line())
            .collect();
        let first = rows.first()?;
        let file = &self.review.files[first.file()];
        let path = file.path.clone();
        let old_path = file.old_path.clone();

        let mut text = Vec::new();
        let (mut old_start, mut old_end) = (None, None);
        let (mut new_start, mut new_end) = (None, None);
        for row in &rows {
            let Row::Line { file, hunk, line } = **row else {
                continue;
            };
            if file != first.file() {
                break;
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

fn flatten(files: &[FileDiff]) -> Vec<Row> {
    let mut rows = Vec::new();
    for (f, file) in files.iter().enumerate() {
        rows.push(Row::File { file: f });
        if file.note.is_some() {
            rows.push(Row::Note { file: f });
            continue;
        }
        for (h, hunk) in file.hunks.iter().enumerate() {
            rows.push(Row::Hunk { file: f, hunk: h });
            for l in 0..hunk.lines.len() {
                rows.push(Row::Line {
                    file: f,
                    hunk: h,
                    line: l,
                });
            }
        }
    }
    rows
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
        ReviewView::new(Review {
            request_id: 1,
            checkout: CheckoutId(1),
            base: argus_protocol::ReviewBase::Unstaged,
            files,
            commit: None,
        })
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
        assert!(v.rows[v.sel].is_line());
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
