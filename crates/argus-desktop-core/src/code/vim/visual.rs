//! What visual mode does with the selection beyond moving it: an operator
//! on it, and an object taken into it.

use super::command::Operator;
use super::object::{self, Object};
use super::region;
use super::registers::Registers;
use super::{Mode, Vim};

impl Vim {
    /// An operator on the selection, or on its whole lines. The selection
    /// ends and the cursor goes to its start before the operator acts.
    pub(super) fn operate_selection(
        &mut self,
        regs: &mut Registers,
        op: Operator,
        lines: bool,
        times: usize,
        register: Option<char>,
    ) {
        let t = self.buf.text();
        let region = region::of_selection(t, self.anchor, self.cursor, lines || self.mode == Mode::VisualLine);
        self.exit_visual();
        self.cursor = region.from;
        self.operate(regs, op, region, register, false, times);
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
        }
        if self.anchor == self.cursor || span.linewise {
            self.anchor = span.from;
            self.cursor = end;
        } else {
            self.anchor = self.anchor.min(span.from);
            self.cursor = self.cursor.max(end);
        }
    }
}
