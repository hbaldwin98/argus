//! What `m`, `Ctrl-o`, `Ctrl-i` and a selection's end do with the marks and
//! the jump list: set a mark, record a jump, walk the list, and keep the
//! two ends of the selection that was just left for `'<`, `'>` and `gv`.

use super::marks::{ANCHOR, HEAD};
use super::{LastVisual, Mode, Vim};

impl Vim {
    /// `m<x>`: the cursor's place is named `x`.
    pub(super) fn set_mark(&mut self, name: char) {
        self.buf.marks.set(name, self.cursor);
    }

    /// The cursor went from `from` to `to` by a jump motion: both are kept,
    /// unless they are one place.
    pub(super) fn jumped(&mut self, from: usize, to: usize) {
        if from != to {
            self.buf.marks.jumps.add(from, to);
        }
    }

    /// `Ctrl-o` (back) and `Ctrl-i` (forward), `count` places along the
    /// list. A place edited away, or the end of the list, leaves the cursor.
    pub(super) fn jump_walk(&mut self, forward: bool, count: usize) {
        let offset = count.max(1) as isize * if forward { 1 } else { -1 };
        if let Some(at) = self.buf.marks.jumps.walk(offset, self.cursor) {
            self.cursor = at.min(self.buf.doc().len());
            self.want = self.buf.text().col(self.cursor);
        }
    }

    /// A selection is left: its ends become `'<` and `'>`, and what it was
    /// is kept for `gv`.
    pub(super) fn remember_selection(&mut self) {
        let eol = self.mode == Mode::VisualBlock && self.want == usize::MAX;
        let (lo, hi) = (self.anchor.min(self.cursor), self.anchor.max(self.cursor));
        let marks = &mut self.buf.marks;
        marks.set_hidden(ANCHOR, self.anchor);
        marks.set_hidden(HEAD, self.cursor);
        marks.set('<', lo);
        marks.set('>', hi);
        self.last_visual = Some(LastVisual { mode: self.mode, eol });
    }
}

#[cfg(test)]
mod tests;
