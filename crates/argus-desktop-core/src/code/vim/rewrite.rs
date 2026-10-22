//! The commands that rewrite text where the cursor is without an
//! operator's target: `r` replaces characters, `~` toggles their case, and
//! `J` joins lines.

use crate::code::change::Change;

use super::{Mode, Vim};

fn toggled(c: char) -> String {
    if c.is_lowercase() {
        c.to_uppercase().collect()
    } else {
        c.to_lowercase().collect()
    }
}

impl Vim {
    /// The starts of up to `n` characters from `from` that are on its line.
    fn chars_from(&self, from: usize, n: usize) -> Vec<usize> {
        let t = self.buf.text();
        let end = t.line_end(t.line_of(from));
        let mut out = Vec::new();
        let mut p = from;
        while out.len() < n && p < end {
            out.push(p);
            p = t.next(p);
        }
        out
    }

    /// `r`: the `n` characters from the cursor, or the selection, become
    /// `ch`. Fewer than `n` on the line is no change.
    pub(super) fn replace_chars(&mut self, ch: char, n: usize) {
        if self.buf.read_only() {
            return;
        }
        if self.is_visual() {
            let region = self.selection_region();
            let old = self.buf.text().slice(region.from, region.to);
            let new: String = old.chars().map(|c| if c == '\n' { c } else { ch }).collect();
            self.exit_visual();
            self.buf.replace(region.from, region.to, &new);
            self.cursor = region.from;
            return;
        }
        let starts = self.chars_from(self.cursor, n);
        if starts.len() < n || n == 0 {
            return;
        }
        let t = self.buf.text();
        let end = t.next(*starts.last().expect("n is above 0"));
        let text: String = std::iter::repeat_n(ch, n).collect();
        self.buf.replace(self.cursor, end, &text);
        self.cursor += text.len() - ch.len_utf8();
    }

    /// `~`: the case of the next `n` characters, and the cursor past them.
    pub(super) fn toggle_case(&mut self, n: usize) {
        if self.buf.read_only() {
            return;
        }
        let starts = self.chars_from(self.cursor, n);
        let Some(&last) = starts.last() else { return };
        let t = self.buf.text();
        let end = t.next(last);
        let new: String = t.slice(self.cursor, end).chars().map(toggled).collect();
        self.buf.replace(self.cursor, end, &new);
        self.cursor += new.len();
        self.want = self.buf.text().col(self.cursor.min(self.buf.doc().len()));
    }

    /// `J` and `gJ`: `count` lines (at least two) from the cursor's, or the
    /// selection's, become one. `J` leaves one space between them, none
    /// where either side has blank to give or the next begins with `)`.
    pub(super) fn join(&mut self, spaces: bool, count: Option<usize>) {
        if self.buf.read_only() {
            return;
        }
        let t = self.buf.text();
        let (first, total) = if self.is_visual() {
            let (a, b) = self.selection_region().lines(t);
            (a, (b - a + 1).max(2))
        } else {
            (t.line_of(self.cursor), count.unwrap_or(2).max(2))
        };
        self.exit_visual();
        let mut at = None;
        for _ in 1..total {
            let t = self.buf.text();
            if first >= t.last_line() {
                break;
            }
            let end = t.line_end(first);
            let next_end = t.line_end(first + 1);
            let skip = if spaces { t.first_nonblank(first + 1).min(next_end) } else { end + 1 };
            let sep = if !spaces {
                ""
            } else {
                let before = t.slice(t.line_start(first), end);
                let blank = before.is_empty() || before.ends_with([' ', '\t']);
                if blank || skip >= next_end || t.char_at(skip) == Some(')') {
                    ""
                } else {
                    " "
                }
            };
            self.buf.edit(&[Change { from: end, to: skip, insert: sep.into() }]);
            at = Some(end);
        }
        if let Some(at) = at {
            self.cursor = at;
            self.want = self.buf.text().col(at);
        }
        if self.mode != Mode::Normal {
            self.mode = Mode::Normal;
        }
    }
}
