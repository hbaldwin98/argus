//! `p`, `P`, `]p` and `[p`: a register put into the text, after or before
//! the cursor, on lines of its own if it is lines, as a block if it is one,
//! and over a selection in visual mode, where `p` leaves what it replaced
//! in the unnamed register. `]p` and `[p` fit the text's indent to the
//! line the cursor is on.

use crate::code::change::Change;

use super::registers::Reg;
use super::{Mode, Shared, Vim};

/// The width of a tab, as the editor's indent counts it.
const TAB: usize = 4;

/// Width of the blanks that begin `line`.
fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| matches!(c, ' ' | '\t')).map(|c| if c == '\t' { TAB } else { 1 }).sum()
}

/// `text` with each line's indent moved by the same amount, so that its
/// first line has `indent`.
fn matched(text: &str, indent: usize) -> String {
    let (body, chomped) = match text.strip_suffix('\n') {
        Some(b) => (b, true),
        None => (text, false),
    };
    let first = body.lines().next().map_or(0, indent_of);
    let mut out: Vec<String> = body
        .split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                return String::new();
            }
            let width = (indent + indent_of(line)).saturating_sub(first);
            let rest = line.trim_start_matches([' ', '\t']);
            format!("{}{rest}", " ".repeat(width))
        })
        .collect();
    if chomped {
        out.push(String::new());
    }
    out.join("\n")
}

impl Vim {
    pub(super) fn paste(&mut self, sh: &mut Shared, register: Option<char>, before: bool, match_indent: bool, n: usize) {
        let Some(mut reg) = sh.regs.get(register) else { return };
        if self.buf.read_only() {
            return;
        }
        if match_indent && !reg.blockwise {
            let t = self.buf.text();
            let current = t.line_text(t.line_of(self.cursor));
            reg.text = matched(&reg.text, indent_of(&current));
        }
        if self.is_visual() {
            self.paste_over(sh, reg, before, n);
            return;
        }
        if reg.blockwise {
            let at = self.cursor;
            let t = self.buf.text();
            let (line, mut col) = t.pos(at);
            if !before {
                col = (col + 1).min(t.line_len(line));
            }
            let top_left = self.insert_rows(&reg.text, line, col);
            self.cursor = top_left;
            self.want = self.buf.text().col(top_left);
            return;
        }
        let t = self.buf.text();
        let line = t.line_of(self.cursor);
        if reg.linewise {
            let body = reg.text.repeat(n);
            let (start, next) = t.line_span(line);
            let (at, text, first) = if before {
                (start, body, line)
            } else if next == t.len() && t.char_at(t.prev(next)) != Some('\n') && next > 0 {
                // After a last line with no break of its own: the break comes first.
                (next, format!("\n{}", body.strip_suffix('\n').unwrap_or(&body)), line + 1)
            } else {
                (next, body, line + 1)
            };
            self.buf.replace(at, at, &text);
            let t = self.buf.text();
            self.cursor = t.first_nonblank(first.min(t.last_line()));
        } else {
            let text = reg.text.repeat(n);
            let empty = t.line_start(line) == t.line_end(line);
            let at = if before || empty { self.cursor } else { t.next(self.cursor) };
            self.buf.replace(at, at, &text);
            let t = self.buf.text();
            self.cursor = if text.contains('\n') { at } else { t.prev(at + text.len()) };
        }
        self.want = self.buf.text().col(self.cursor);
    }

    /// Puts the rows of `text` one under another from column `col` of line
    /// `line`, adding lines and padding short ones with spaces to reach the
    /// column; an empty row is a space. The top-left corner, as a byte.
    fn insert_rows(&mut self, text: &str, line: usize, col: usize) -> usize {
        for (i, row) in text.split('\n').enumerate() {
            let t = self.buf.text();
            let ln = line + i;
            if ln > t.last_line() {
                let end = t.len();
                self.buf.replace(end, end, "\n");
            }
            let t = self.buf.text();
            let len = t.line_len(ln);
            let end = t.line_end(ln);
            if len < col {
                self.buf.replace(end, end, &" ".repeat(col - len));
            }
            let at = self.buf.text().at(ln, col);
            let row = if row.is_empty() { " " } else { row };
            self.buf.replace(at, at, row);
        }
        self.buf.text().at(line, col)
    }

    fn paste_over(&mut self, sh: &mut Shared, reg: Reg, before: bool, n: usize) {
        let block = self.mode == Mode::VisualBlock;
        let (top, _, left) = if block { self.block_bounds() } else { (0, 0, 0) };
        let region = self.selection_region();
        let t = self.buf.text();
        if block {
            let rows = self.block_rows();
            let old = Reg::block(&rows.iter().map(|&(a, b)| t.slice(a, b)).collect::<Vec<_>>());
            self.exit_visual();
            let edits: Vec<Change> = rows.iter().map(|&(from, to)| Change { from, to, insert: String::new() }).collect();
            self.buf.edit(&edits);
            if !before {
                sh.regs.delete(None, old, false);
            }
            if reg.blockwise {
                self.cursor = self.insert_rows(&reg.text, top, left);
            } else {
                let at = self.buf.text().at(top, left);
                self.buf.replace(at, at, &reg.text.repeat(n));
                self.cursor = at;
            }
            return;
        }
        let old = Reg::new(&t.slice(region.from, region.to), region.linewise);
        let at_end = region.to == t.len() && t.char_at(t.prev(region.to)) != Some('\n');
        let mut text = reg.text.repeat(n);
        match (region.linewise, reg.linewise) {
            (true, false) if !at_end => text.push('\n'),
            (true, true) if at_end => {
                text.pop();
            }
            (false, true) => text.insert(0, '\n'),
            _ => {}
        }
        self.exit_visual();
        self.buf.replace(region.from, region.to, &text);
        if !before {
            // `P` keeps the register it pasted from; `p` takes what it replaced.
            sh.regs.delete(None, old, false);
        }
        let t = self.buf.text();
        self.cursor = if region.linewise { t.first_nonblank(t.line_of(region.from.min(t.len()))) } else { region.from };
    }
}

#[cfg(test)]
mod tests;
