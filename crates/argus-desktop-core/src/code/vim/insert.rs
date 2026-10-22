//! Insert and replace mode: the keys that type, the few that edit what was
//! typed (`Ctrl-w`, `Ctrl-u`, `Ctrl-t`, `Ctrl-d`, `Ctrl-r`), and Escape,
//! which repeats the text for a count, files it in `".`, and steps the
//! cursor back onto the last character.

use super::key::{Code, Key};
use super::operate::INDENT;
use super::registers::Registers;
use super::text::{Class, Text};
use super::{Insert, Mode, Vim};

/// The most a counted insert may add: a count is a typing aid, not a way to
/// fill memory.
const MOST_REPEATED: usize = 1 << 20;

impl Vim {
    /// Enters insert mode with the cursor where it is. A read-only file is
    /// not entered (EDITOR-VIM-18). `lines` is `o` and `O`, whose count
    /// repeats the new line.
    pub(super) fn start_insert(&mut self, count: usize, lines: bool) {
        if self.buf.read_only() {
            return;
        }
        self.mode = Mode::Insert;
        self.insert = Some(Insert { count: count.max(1), lines, text: String::new(), register: false });
        self.recording = !self.replaying;
    }

    fn record(&mut self, key: Key) {
        if self.recording {
            if let Some(dot) = &mut self.dot {
                dot.typed.push(key);
            }
        }
    }

    fn typed_text(&mut self, text: &str) {
        if let Some(i) = &mut self.insert {
            i.text.push_str(text);
        }
    }

    /// Takes back the last `n` characters of what the insert has typed, for
    /// the keys that delete.
    fn forget(&mut self, n: usize) {
        if let Some(i) = &mut self.insert {
            for _ in 0..n {
                i.text.pop();
            }
        }
    }

    fn put(&mut self, text: &str) {
        self.buf.replace(self.cursor, self.cursor, text);
        self.cursor += text.len();
        self.typed_text(text);
    }

    pub(super) fn insert_key(&mut self, regs: &mut Registers, key: Key) -> bool {
        if key.code == Code::Esc {
            self.leave_insert(regs);
            return true;
        }
        if self.insert.as_ref().is_some_and(|i| i.register) {
            if let Some(i) = &mut self.insert {
                i.register = false;
            }
            if let Some(reg) = key.typed().and_then(|c| regs.get(Some(c))) {
                self.put(&reg.text);
            }
            self.record(key);
            return true;
        }
        let t = self.buf.text();
        match (key.code, key.ctrl) {
            (Code::Char(c), false) => self.type_char(c),
            (Code::Enter, false) => {
                let line = t.line_of(self.cursor);
                let blanks = t.first_nonblank(line).min(self.cursor);
                let indent = t.slice(t.line_start(line), blanks);
                self.put(&format!("\n{indent}"));
            }
            (Code::Backspace, false) => {
                if self.cursor > 0 {
                    let p = t.prev(self.cursor);
                    if self.mode == Mode::Insert {
                        self.buf.replace(p, self.cursor, "");
                        self.forget(1);
                    }
                    self.cursor = p;
                }
            }
            (Code::Delete, false) => {
                if self.cursor < t.len() {
                    self.buf.replace(self.cursor, t.next(self.cursor), "");
                }
            }
            (Code::Left, false) => self.cursor = t.prev(self.cursor),
            (Code::Right, false) => self.cursor = t.next(self.cursor),
            (Code::Home, false) => self.cursor = t.line_start(t.line_of(self.cursor)),
            (Code::End, false) => self.cursor = t.line_end(t.line_of(self.cursor)),
            (Code::Up | Code::Down, false) => self.caret_vertically(key.code == Code::Up),
            (Code::Insert, false) => {
                self.mode = if self.mode == Mode::Insert { Mode::Replace } else { Mode::Insert };
            }
            (Code::Char('w'), true) => self.delete_word_before(),
            (Code::Char('u'), true) => {
                let start = t.line_start(t.line_of(self.cursor));
                let (from, to) = if self.cursor == start { (t.prev(start), start) } else { (start, self.cursor) };
                self.forget(t.slice(from, to).chars().count());
                self.buf.replace(from, to, "");
                self.cursor = from;
            }
            (Code::Char('t'), true) => {
                let start = t.line_start(t.line_of(self.cursor));
                self.buf.replace(start, start, INDENT);
                self.cursor += INDENT.len();
            }
            (Code::Char('d'), true) => {
                let start = t.line_start(t.line_of(self.cursor));
                let mut end = start;
                while end - start < INDENT.len() && t.char_at(end) == Some(' ') {
                    end += 1;
                }
                self.buf.replace(start, end, "");
                self.cursor = self.cursor.saturating_sub(end - start).max(start);
            }
            (Code::Char('r'), true) => {
                if let Some(i) = &mut self.insert {
                    i.register = true;
                }
            }
            _ => return false,
        }
        self.record(key);
        self.want = self.buf.text().col(self.cursor.min(self.buf.text().len()));
        true
    }

    fn type_char(&mut self, c: char) {
        let t = self.buf.text();
        let mut utf8 = [0; 4];
        let s: &str = c.encode_utf8(&mut utf8);
        let over = self.mode == Mode::Replace && matches!(t.char_at(self.cursor), Some(o) if o != '\n');
        let end = if over { t.next(self.cursor) } else { self.cursor };
        self.buf.replace(self.cursor, end, s);
        self.cursor += s.len();
        self.typed_text(s);
    }

    fn caret_vertically(&mut self, up: bool) {
        let t = self.buf.text();
        let line = t.line_of(self.cursor);
        let target = if up { line.checked_sub(1) } else { (line < t.last_line()).then_some(line + 1) };
        let Some(target) = target else { return };
        let (start, end) = (t.line_start(target), t.line_end(target));
        let mut p = start;
        for _ in 0..t.col(self.cursor) {
            if p >= end {
                break;
            }
            p = t.next(p);
        }
        self.cursor = p.min(end);
    }

    fn delete_word_before(&mut self) {
        let t = self.buf.text();
        let start = t.line_start(t.line_of(self.cursor));
        let mut p = self.cursor;
        if p == start {
            p = t.prev(p);
        } else {
            while p > start && t.class_at(t.prev(p), false) == Class::Space {
                p = t.prev(p);
            }
            if p > start {
                let c = t.class_at(t.prev(p), false);
                while p > start && t.class_at(t.prev(p), false) == c {
                    p = t.prev(p);
                }
            }
        }
        self.forget(t.slice(p, self.cursor).chars().count());
        self.buf.replace(p, self.cursor, "");
        self.cursor = p;
    }

    fn leave_insert(&mut self, regs: &mut Registers) {
        if let Some(ins) = self.insert.take() {
            if ins.count > 1 && !ins.text.is_empty() {
                let unit = if ins.lines { format!("\n{}", ins.text) } else { ins.text.clone() };
                let times = (ins.count - 1).min(MOST_REPEATED / unit.len().max(1));
                let more = unit.repeat(times);
                self.buf.replace(self.cursor, self.cursor, &more);
                self.cursor += more.len();
            }
            if !ins.text.is_empty() {
                regs.typed(ins.text);
            }
        }
        self.recording = false;
        self.last_insert_end = Some(self.cursor);
        self.mode = Mode::Normal;
        let t: Text = self.buf.text();
        if self.cursor > t.line_start(t.line_of(self.cursor)) {
            self.cursor = t.prev(self.cursor);
        }
        self.want = t.col(self.cursor);
    }
}
