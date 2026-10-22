//! The `:` line: typed key by key, then run. The commands built are write,
//! undo and redo, a line number, `:d`, `:y`, `:pu`, `:j` over a range, and
//! the two that mean nothing to a buffer with no search or insert mode to
//! leave (EDITOR-VIM-4, -13). Anything else is the library's own error.
//! (`:s`, `:g`, `:v`, `:norm`, `:sor` and `:reg` are not built.)

use super::command::Operator;
use super::key::{Code, Key};
use super::range;
use super::region;
use super::registers::Registers;
use super::{Effect, Mode, Vim};

impl Vim {
    pub(super) fn ex_key(&mut self, regs: &mut Registers, key: Key) -> bool {
        match (key.code, key.ctrl) {
            (Code::Esc, _) => {
                self.cmdline.clear();
                self.mode = Mode::Normal;
            }
            (Code::Enter, false) => {
                let line = std::mem::take(&mut self.cmdline);
                self.mode = Mode::Normal;
                self.ex_run(regs, line.trim());
            }
            (Code::Backspace, false) => {
                if self.cmdline.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            (Code::Char(c), false) => self.cmdline.push(c),
            _ => return false,
        }
        true
    }

    fn ex_run(&mut self, regs: &mut Registers, line: &str) {
        let line = line.trim_start_matches(':').trim();
        let t = self.buf.text();
        let selection = self.last_visual.map(|(_, a, c)| (t.line_of(a.min(c)), t.line_of(a.max(c))));
        let (range, cmd) = range::parse(t, t.line_of(self.cursor), selection, line);
        if cmd.is_empty() {
            if let Some((_, last)) = range {
                self.cursor = t.first_nonblank(last);
                self.want = t.col(self.cursor);
            }
            return;
        }
        let (name, arg) = match cmd.find(|c: char| !c.is_ascii_alphabetic()) {
            Some(i) => (&cmd[..i], cmd[i..].trim()),
            None => (cmd, ""),
        };
        let (first, last) = range.unwrap_or_else(|| {
            let line = t.line_of(self.cursor);
            (line, line)
        });
        // A register named after `:d`, `:y` and `:pu`, such as `:y a`.
        let register = arg.chars().next().filter(|c| arg.chars().count() == 1 && Registers::valid(*c));
        match if arg.is_empty() || matches!(name, "d" | "de" | "del" | "delete" | "y" | "ya" | "yank" | "pu" | "put") { name } else { "" } {
            // These take no argument: `:w file` is not the editor's write.
            "noh" | "nohlsearch" if arg.is_empty() => {}
            "w" | "write" if arg.is_empty() => self.effects.push(Effect::Write),
            "u" | "undo" if arg.is_empty() => self.undo_redo(true, 1),
            "red" | "redo" if arg.is_empty() => self.undo_redo(false, 1),
            "start" | "startinsert" if arg.is_empty() => self.start_insert(1, false),
            "d" | "de" | "del" | "delete" => {
                if !self.buf.read_only() {
                    let r = region::of_lines(t, first, last);
                    self.operate(regs, Operator::Delete, r, register, false, 1);
                    let t = self.buf.text();
                    self.cursor = t.first_nonblank(first.min(t.last_line()));
                }
            }
            "y" | "ya" | "yank" => {
                let keep = self.cursor;
                self.operate(regs, Operator::Yank, region::of_lines(t, first, last), register, false, 1);
                self.cursor = keep;
            }
            "j" | "join" => {
                self.cursor = t.line_start(first);
                let n = if last > first { last - first + 1 } else { 2 };
                self.join(true, Some(n));
            }
            "pu" | "put" => self.ex_put(regs, register, last, arg.starts_with('!')),
            _ => self.message = Some(format!("Not an editor command: {cmd}")),
        }
        self.want = self.buf.text().col(self.cursor.min(self.buf.text().len()));
    }

    /// `:pu`: the register as lines below `line`, `:pu!` above it.
    fn ex_put(&mut self, regs: &mut Registers, register: Option<char>, line: usize, above: bool) {
        let Some(reg) = regs.get(register) else { return };
        if self.buf.read_only() {
            return;
        }
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
