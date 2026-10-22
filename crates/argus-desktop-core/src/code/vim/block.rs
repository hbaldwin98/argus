//! Visual block mode: the rectangle between the anchor and the cursor, its
//! rows, and what an operator does to it. A row is the part of a line
//! inside the columns, empty where the line is too short to reach them;
//! after `$` every row runs to its line's end. A block's text is yanked as
//! rows, and what is typed after `I`, `A` or `c` goes into every row.

use crate::code::change::Change;

use super::command::Operator;
use super::operate::{recased, INDENT};
use super::registers::Reg;
use super::text::Text;
use super::{Mode, Shared, Vim};

/// Where in each row an insert goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Col {
    At(usize),
    Eol,
}

/// An insert that every row of a block is typed into, once it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockInsert {
    pub first: usize,
    pub last: usize,
    pub col: Col,
    /// Where the insert began, on the first row: where the cursor ends.
    pub start: usize,
}

/// The rows of the block between `anchor` and `cursor`: for each line of
/// it, the bytes inside the columns.
pub fn rows(t: Text, anchor: usize, cursor: usize, eol: bool) -> Vec<(usize, usize)> {
    let (la, ca) = t.pos(anchor);
    let (lc, cc) = t.pos(cursor);
    let (left, right) = (ca.min(cc), ca.max(cc));
    (la.min(lc)..=la.max(lc))
        .map(|line| {
            let from = t.at(line, left);
            let to = if eol { t.line_end(line) } else { t.at(line, right + 1) };
            (from, to.max(from))
        })
        .collect()
}

/// The rows from the left column to each line's end.
fn to_line_ends(t: Text, rows: &[(usize, usize)]) -> Vec<(usize, usize)> {
    rows.iter().map(|&(from, _)| (from, t.line_end(t.line_of(from)))).collect()
}

impl Vim {
    pub(super) fn block_rows(&self) -> Vec<(usize, usize)> {
        rows(self.buf.text(), self.anchor, self.cursor, self.want == usize::MAX)
    }

    /// The block's first line, last line, and left column.
    pub(super) fn block_bounds(&self) -> (usize, usize, usize) {
        let t = self.buf.text();
        let (la, ca) = t.pos(self.anchor);
        let (lc, cc) = t.pos(self.cursor);
        (la.min(lc), la.max(lc), ca.min(cc))
    }

    /// An operator on the block, or on its rows to their ends (`lines`).
    pub(super) fn operate_block(
        &mut self,
        sh: &mut Shared,
        op: Operator,
        lines: bool,
        times: usize,
        register: Option<char>,
    ) {
        // The text as it was, which the selection ending does not disturb.
        let doc = self.buf.doc().clone();
        let t = Text(&doc);
        let (top, bottom, left) = self.block_bounds();
        let mut rows = self.block_rows();
        if lines {
            rows = to_line_ends(t, &rows);
        }
        let texts: Vec<String> = rows.iter().map(|&(a, b)| t.slice(a, b)).collect();
        let lined = super::region::of_lines(t, top, bottom);
        self.exit_visual();
        let corner = |vim: &Vim| vim.buf.text().at(top, left);
        match op {
            Operator::Yank => {
                sh.regs.yank(register, Reg::block(&texts));
                self.cursor = corner(self);
            }
            Operator::Delete | Operator::Change => {
                if self.buf.read_only() {
                    return;
                }
                sh.regs.delete(register, Reg::block(&texts), false);
                let edits: Vec<Change> =
                    rows.iter().map(|&(from, to)| Change { from, to, insert: String::new() }).collect();
                self.buf.edit(&edits);
                self.cursor = corner(self);
                if op == Operator::Change {
                    let start = self.cursor;
                    self.start_insert(1, false);
                    if let Some(i) = &mut self.insert {
                        i.block = Some(BlockInsert { first: top, last: bottom, col: Col::At(left), start });
                    }
                }
            }
            Operator::Indent => {
                let edits: Vec<Change> = rows
                    .iter()
                    .map(|&(from, _)| Change { from, to: from, insert: INDENT.repeat(times.max(1)) })
                    .collect();
                self.buf.edit(&edits);
                self.cursor = corner(self);
            }
            Operator::Outdent => {
                let edits: Vec<Change> = rows
                    .iter()
                    .filter_map(|&(from, _)| {
                        let mut end = from;
                        for _ in 0..times.max(1) {
                            match t.char_at(end) {
                                Some('\t') => end += 1,
                                Some(' ') => {
                                    end += 1;
                                    for _ in 1..INDENT.len() {
                                        if t.char_at(end) != Some(' ') {
                                            break;
                                        }
                                        end += 1;
                                    }
                                }
                                _ => break,
                            }
                        }
                        (end > from).then(|| Change { from, to: end, insert: String::new() })
                    })
                    .collect();
                self.buf.edit(&edits);
                self.cursor = corner(self);
            }
            Operator::Swap | Operator::Lower | Operator::Upper | Operator::Rot13 => {
                let edits: Vec<Change> = rows
                    .iter()
                    .zip(&texts)
                    .filter(|(_, old)| !old.is_empty())
                    .map(|(&(from, to), old)| Change { from, to, insert: recased(old, op) })
                    .collect();
                self.buf.edit(&edits);
                self.cursor = corner(self);
            }
            Operator::Wrap | Operator::WrapKeep | Operator::Comment | Operator::Reindent => {
                self.operate(sh, op, lined, register, false, times);
            }
        }
        self.want = self.buf.text().col(self.cursor.min(self.buf.doc().len()));
    }

    /// `r` over a block: every character in it becomes `ch`, a tab as four.
    pub(super) fn replace_block(&mut self, ch: char) {
        let t = self.buf.text();
        let (top, _, left) = self.block_bounds();
        let edits: Vec<Change> = self
            .block_rows()
            .iter()
            .map(|&(from, to)| {
                let old = t.slice(from, to);
                let new: String = old
                    .chars()
                    .flat_map(|c| std::iter::repeat_n(ch, if c == '\t' { 4 } else { 1 }))
                    .collect();
                Change { from, to, insert: new }
            })
            .collect();
        self.exit_visual();
        self.buf.edit(&edits);
        self.cursor = self.buf.text().at(top, left);
    }

    /// `I` and `A` in visual mode. A block opens an insert on its first
    /// row, which each other row gets when it ends; any other selection
    /// opens one where the library puts it.
    pub(super) fn insert_at_selection(&mut self, end: bool) {
        if self.buf.read_only() {
            return;
        }
        let t = self.buf.text();
        if self.mode == Mode::VisualBlock {
            let (top, bottom, left) = self.block_bounds();
            let (_, ca) = t.pos(self.anchor);
            let (_, cc) = t.pos(self.cursor);
            let eol = self.want == usize::MAX;
            let col = if !end {
                Col::At(left)
            } else if eol {
                Col::Eol
            } else {
                Col::At(ca.max(cc) + 1)
            };
            self.exit_visual();
            let t = self.buf.text();
            self.cursor = match col {
                Col::At(c) => t.at(top, c),
                Col::Eol => t.line_end(top),
            };
            let start = self.cursor;
            self.start_insert(1, false);
            if let Some(i) = &mut self.insert {
                i.block = Some(BlockInsert { first: top, last: bottom, col, start });
            }
            return;
        }
        let (la, lc) = (t.line_of(self.anchor), t.line_of(self.cursor));
        let at = if !end {
            if lc < la {
                self.cursor
            } else {
                t.line_start(la)
            }
        } else if lc >= la {
            t.next(self.cursor).min(t.line_end(lc))
        } else {
            t.line_start(la)
        };
        self.exit_visual();
        self.cursor = at;
        self.start_insert(1, false);
    }

    /// An insert that began on a block ends: the text typed goes into the
    /// rows below the first, where its column is in them.
    pub(super) fn fill_block(&mut self, block: BlockInsert, text: &str) {
        if text.is_empty() || text.contains('\n') {
            return;
        }
        let t = self.buf.text();
        let edits: Vec<Change> = (block.first + 1..=block.last.min(t.last_line()))
            .map(|line| {
                let at = match block.col {
                    Col::At(c) => t.at(line, c),
                    Col::Eol => t.line_end(line),
                };
                Change { from: at, to: at, insert: text.to_owned() }
            })
            .collect();
        self.buf.edit(&edits);
    }
}

#[cfg(test)]
mod tests;
