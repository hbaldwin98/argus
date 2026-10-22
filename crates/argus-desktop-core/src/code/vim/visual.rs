//! What visual mode does with the selection beyond moving it: starting,
//! switching and leaving it, `gv`, `o` and `O`, an operator on it, and an
//! object taken into it.

use super::command::Operator;
use super::marks::{ANCHOR, HEAD};
use super::object::{self, Object};
use super::region::{self, Region};
use super::{LastVisual, Mode, Shared, Vim};

impl Vim {
    /// An operator on the selection, or on its whole lines. The selection
    /// ends and the cursor goes to its start before the operator acts.
    pub(super) fn operate_selection(
        &mut self,
        sh: &mut Shared,
        op: Operator,
        lines: bool,
        times: usize,
        register: Option<char>,
    ) {
        if self.mode == Mode::VisualBlock {
            self.operate_block(sh, op, lines, times, register);
            return;
        }
        let t = self.buf.text();
        let region = region::of_selection(t, self.anchor, self.cursor, lines || self.mode == Mode::VisualLine);
        self.exit_visual();
        self.cursor = region.from;
        self.operate(sh, op, region, register, false, times);
    }

    /// `S` and `R`: the selection's lines are changed, a block's included.
    pub(super) fn change_lines(&mut self, sh: &mut Shared, times: usize, register: Option<char>) {
        if self.mode == Mode::VisualBlock {
            self.mode = Mode::Visual;
        }
        self.operate_selection(sh, Operator::Change, true, times, register);
    }

    /// `iw`, `a(` and the rest in visual mode: the object is the selection,
    /// or grows it if one is already made.
    pub(super) fn select(&mut self, object: Object, inner: bool, n: usize) {
        let t = self.buf.text();
        let Some(span) = object::span(object, inner, t, self.cursor, n) else { return };
        if span.to <= span.from {
            return;
        }
        let end = t.prev(span.to);
        if span.linewise {
            self.mode = Mode::VisualLine;
        } else if self.mode == Mode::VisualBlock {
            self.mode = Mode::Visual;
        }
        if self.anchor == self.cursor || span.linewise {
            self.anchor = span.from;
            self.cursor = end;
        } else {
            self.anchor = self.anchor.min(span.from);
            self.cursor = self.cursor.max(end);
        }
    }

    /// `o`: the cursor goes to the selection's other end. `O` does the same,
    /// but in a block to the other end of the same row.
    pub(super) fn other_end(&mut self, same_line: bool) {
        if same_line && self.mode == Mode::VisualBlock {
            let t = self.buf.text();
            let (la, lc) = (t.line_of(self.anchor), t.line_of(self.cursor));
            let (ca, cc) = (t.col(self.anchor), t.col(self.cursor));
            self.anchor = t.at_col(la, cc);
            self.cursor = t.at_col(lc, ca);
        } else {
            std::mem::swap(&mut self.cursor, &mut self.anchor);
        }
        self.want = self.buf.text().col(self.cursor);
    }

    pub(super) fn visual_mode(&mut self, kind: super::command::Visual) {
        use super::command::Visual;
        let to = match kind {
            Visual::Char => Mode::Visual,
            Visual::Line => Mode::VisualLine,
            Visual::Block => Mode::VisualBlock,
            Visual::Reselect => return self.reselect(),
        };
        match self.mode {
            m if m == to => self.exit_visual(),
            m if self.is_visual() && m != to => self.mode = to,
            _ => {
                self.anchor = self.cursor;
                self.mode = to;
            }
        }
    }

    /// `gv`: the last selection is made again, if the text has not edited
    /// its ends away. In a selection it swaps with the one now made.
    fn reselect(&mut self) {
        let Some(last) = self.last_visual else { return };
        let ends = (self.buf.marks.get(ANCHOR), self.buf.marks.get(HEAD));
        let (Some(anchor), Some(cursor)) = ends else { return };
        if self.is_visual() {
            self.remember_selection();
        }
        self.mode = last.mode;
        self.anchor = anchor;
        self.cursor = cursor;
        self.want = if last.eol { usize::MAX } else { self.buf.text().col(cursor) };
        self.remember_marks_only();
    }

    /// The selection just made is `'<` and `'>`.
    fn remember_marks_only(&mut self) {
        let (lo, hi) = (self.anchor.min(self.cursor), self.anchor.max(self.cursor));
        self.buf.marks.set('<', lo);
        self.buf.marks.set('>', hi);
    }

    pub(super) fn exit_visual(&mut self) {
        if self.is_visual() {
            self.remember_selection();
            self.mode = Mode::Normal;
        }
    }

    pub(super) fn selection_region(&self) -> Region {
        region::of_selection(self.buf.text(), self.anchor, self.cursor, self.mode == Mode::VisualLine)
    }

    /// What the selection was when it was left, if there was one.
    pub fn last_selection(&self) -> Option<Mode> {
        self.last_visual.map(|LastVisual { mode, .. }| mode)
    }
}

#[cfg(test)]
mod tests;
