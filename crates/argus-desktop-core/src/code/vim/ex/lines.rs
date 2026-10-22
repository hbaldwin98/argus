//! The ex commands over lines: `:d` deletes them, `:y` yanks them, `:pu`
//! puts a register under one, `:j` joins them. Each takes the lines named,
//! or the selection's, or the cursor's, and a register where it has one.

use crate::code::vim::command::Operator;
use crate::code::vim::region;
use crate::code::vim::registers::Registers;

use super::range::Parsed;
use super::super::{Shared, Vim};

/// A register named by the first argument, if it is one.
fn register_of(p: &Parsed) -> Option<char> {
    p.args().first().and_then(|a| {
        let mut chars = a.chars();
        let c = chars.next()?;
        (chars.next().is_none() && Registers::valid(c)).then_some(c)
    })
}

impl Vim {
    pub(super) fn ex_delete(&mut self, sh: &mut Shared, p: &Parsed) {
        if self.buf.read_only() {
            return;
        }
        let (first, last) = p.lines(self.buf.text());
        let region = region::of_lines(self.buf.text(), first, last);
        self.operate(sh, Operator::Delete, region, register_of(p), false, 1);
        let t = self.buf.text();
        self.cursor = t.first_nonblank(first.min(t.last_line()));
    }

    pub(super) fn ex_yank(&mut self, sh: &mut Shared, p: &Parsed) {
        let (first, last) = p.lines(self.buf.text());
        let keep = self.cursor;
        let region = region::of_lines(self.buf.text(), first, last);
        self.operate(sh, Operator::Yank, region, register_of(p), false, 1);
        self.cursor = keep;
    }

    pub(super) fn ex_join(&mut self, p: &Parsed) {
        let t = self.buf.text();
        let (first, last) = p.lines(t);
        self.cursor = t.line_start(first);
        let n = if last > first { last - first + 1 } else { 2 };
        self.join(true, Some(n));
    }

    /// `:pu`: the register as lines below the last line named, `:pu!` above
    /// the first.
    pub(super) fn ex_put(&mut self, sh: &mut Shared, p: &Parsed) {
        let args = p.args();
        let above = args.first().is_some_and(|a| a.starts_with('!'));
        let name = args.iter().skip(usize::from(above)).find_map(|a| a.chars().next());
        let Some(reg) = sh.regs.get(name) else { return };
        if self.buf.read_only() {
            return;
        }
        let (first, last) = p.lines(self.buf.text());
        let line = if above { first } else { last };
        let mut body = reg.text.clone();
        if !body.ends_with('\n') {
            body.push('\n');
        }
        let t = self.buf.text();
        let (start, next) = t.line_span(line);
        let (at, text, target) = if above {
            (start, body, line)
        } else if next == t.len() && next > 0 && t.char_at(t.prev(next)) != Some('\n') {
            (next, format!("\n{}", body.strip_suffix('\n').unwrap_or(&body)), line + 1)
        } else {
            (next, body, line + 1)
        };
        self.buf.replace(at, at, &text);
        let t = self.buf.text();
        self.cursor = t.first_nonblank(target.min(t.last_line()));
    }
}

#[cfg(test)]
mod tests;
