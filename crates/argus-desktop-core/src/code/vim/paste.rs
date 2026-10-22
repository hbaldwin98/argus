//! `p` and `P`: a register put into the text, after or before the cursor,
//! on lines of its own if it is lines; and over a selection in visual mode,
//! where `p` leaves what it replaced in the unnamed register.

use super::registers::{Reg, Registers};
use super::Vim;

impl Vim {
    pub(super) fn paste(&mut self, regs: &mut Registers, register: Option<char>, before: bool, n: usize) {
        let Some(reg) = regs.get(register) else { return };
        if self.buf.read_only() {
            return;
        }
        if self.is_visual() {
            self.paste_over(regs, reg, before, n);
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

    fn paste_over(&mut self, regs: &mut Registers, reg: Reg, before: bool, n: usize) {
        let region = self.selection_region();
        let t = self.buf.text();
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
            regs.delete(None, old, false);
        }
        let t = self.buf.text();
        self.cursor = if region.linewise { t.first_nonblank(t.line_of(region.from.min(t.len()))) } else { region.from };
    }
}
