//! What each operator does to a region of the text: delete and yank into
//! the registers, change into insert mode, indent, and the three cases.
//! The cursor lands where Vim leaves it.

use crate::code::change::Change;

use super::command::Operator;
use super::region::Region;
use super::registers::{Reg, Registers};
use super::{Mode, Vim};

/// Four spaces, the editor's indent unit.
pub const INDENT: &str = "    ";

fn swapped(c: char) -> String {
    if c.is_lowercase() {
        c.to_uppercase().collect()
    } else if c.is_uppercase() {
        c.to_lowercase().collect()
    } else {
        c.to_string()
    }
}

fn recased(text: &str, op: Operator) -> String {
    match op {
        Operator::Lower => text.to_lowercase(),
        Operator::Upper => text.to_uppercase(),
        _ => text.chars().map(swapped).collect(),
    }
}

impl Vim {
    /// The register's worth of a region.
    fn reg_of(&self, region: Region) -> Reg {
        Reg::new(&self.buf.text().slice(region.from, region.to), region.linewise)
    }

    /// Applies `op` to `region`. `far` is a motion Vim files deletes of as
    /// a line's worth whatever they span; `times` repeats an indent.
    pub(super) fn operate(
        &mut self,
        regs: &mut Registers,
        op: Operator,
        region: Region,
        register: Option<char>,
        far: bool,
        times: usize,
    ) {
        match op {
            Operator::Yank => self.yank(regs, region, register),
            Operator::Delete => self.delete(regs, region, register, far),
            Operator::Change => self.change(regs, region, register, far),
            Operator::Indent | Operator::Outdent => self.shift(region, op == Operator::Indent, times),
            Operator::Swap | Operator::Lower | Operator::Upper => self.recase(region, op),
        }
    }

    fn yank(&mut self, regs: &mut Registers, region: Region, register: Option<char>) {
        regs.yank(register, self.reg_of(region));
        let t = self.buf.text();
        if region.linewise {
            let (first, _) = region.lines(t);
            if first < t.line_of(self.cursor) {
                self.cursor = t.at_col(first, t.col(self.cursor));
            }
        } else {
            self.cursor = region.from;
        }
    }

    fn delete(&mut self, regs: &mut Registers, region: Region, register: Option<char>, far: bool) {
        if self.buf.read_only() {
            return;
        }
        regs.delete(register, self.reg_of(region), far);
        let t = self.buf.text();
        let mut from = region.from;
        // The last lines go with the break before them, or a blank is left.
        if region.linewise && region.to == t.len() && from > 0 && t.char_at(t.prev(region.to)) != Some('\n') {
            from -= 1;
        }
        self.buf.replace(from, region.to, "");
        let t = self.buf.text();
        self.cursor = if region.linewise {
            let line = t.line_of(from.min(t.len())).min(t.last_line());
            let p = t.first_nonblank(line);
            if p >= t.line_end(line) {
                t.last_char(line)
            } else {
                p
            }
        } else {
            from
        };
        self.want = t.col(self.cursor);
    }

    fn change(&mut self, regs: &mut Registers, region: Region, register: Option<char>, far: bool) {
        if self.buf.read_only() {
            return;
        }
        if region.linewise {
            // The lines' text goes; one empty line stays to type on.
            regs.delete(register, self.reg_of(region), far);
            let t = self.buf.text();
            let end = if region.to > region.from && t.char_at(t.prev(region.to)) == Some('\n') {
                t.prev(region.to)
            } else {
                region.to
            };
            self.buf.replace(region.from, end, "");
            self.cursor = region.from;
        } else {
            self.delete(regs, region, register, far);
            self.cursor = region.from;
        }
        self.start_insert(1, false);
    }

    fn shift(&mut self, region: Region, indent: bool, times: usize) {
        let t = self.buf.text();
        let (first, last) = region.lines(t);
        let mut edits = Vec::new();
        for line in first..=last {
            let start = t.line_start(line);
            if indent {
                if !t.is_blank_line(line) {
                    edits.push(Change { from: start, to: start, insert: INDENT.repeat(times.max(1)) });
                }
                continue;
            }
            let (mut cols, mut end) = (0, start);
            while cols < 4 * times.max(1) {
                match t.char_at(end) {
                    Some(' ') => cols += 1,
                    Some('\t') => cols += 4,
                    _ => break,
                }
                end += 1;
            }
            if end > start {
                edits.push(Change { from: start, to: end, insert: String::new() });
            }
        }
        self.buf.edit(&edits);
        let t = self.buf.text();
        let p = t.first_nonblank(first.min(t.last_line()));
        self.cursor = p;
        self.want = t.col(p);
    }

    fn recase(&mut self, region: Region, op: Operator) {
        let t = self.buf.text();
        let old = t.slice(region.from, region.to);
        let new = recased(&old, op);
        if new != old {
            self.buf.replace(region.from, region.to, &new);
        }
        let t = self.buf.text();
        if region.linewise {
            let (first, _) = region.lines(t);
            if first < t.line_of(self.cursor) {
                self.cursor = t.at_col(first, t.col(self.cursor));
            }
        } else {
            self.cursor = region.from;
        }
        if self.mode != Mode::Insert {
            self.want = t.col(self.cursor.min(t.len()));
        }
    }
}
