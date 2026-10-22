//! Running a command that has been read: a motion moves the cursor, an
//! operator is given the text its target names, and the rest are the
//! commands of their own. A command that changes the text is kept for `.`.

use super::command::{Action, At, Command, Operator, Target, Visual};
use super::key::{Code, Key};
use super::motion::{self, Aim, Motion, Reach};
use super::region::{self, Region};
use super::registers::Registers;
use super::text::{Class, Text};
use super::{object, word, Dot, Effect, Mode, Vim};

/// Whether the command, run in `visual` or not, changes the text and so is
/// what `.` repeats.
fn repeatable(action: Action, visual: bool) -> bool {
    match action {
        Action::Operate { op, .. } => op != Operator::Yank,
        Action::Insert(_) | Action::Replace | Action::ReplaceChar(_) | Action::ToggleCase => !visual,
        Action::Join { .. } | Action::Paste { .. } | Action::Increment { .. } => !visual,
        _ => false,
    }
}

/// `cw` is `ce` on a word, and on a word's last character it changes just that.
fn change_word(t: Text, cursor: usize, count: usize, big: bool) -> Reach {
    let mut p = cursor;
    for i in 0..count {
        let at_end = t.class_at(t.next(p), big) != t.class_at(p, big) || t.next(p) >= t.line_end(t.line_of(p));
        if i == 0 && at_end {
            continue;
        }
        p = word::end(t, p, big);
    }
    Reach { to: p, linewise: false, inclusive: true, far: false, vertical: false }
}

impl Vim {
    pub(super) fn run(&mut self, regs: &mut Registers, cmd: Command) {
        if !self.replaying && repeatable(cmd.action, self.is_visual()) {
            self.dot = Some(Dot { command: cmd, typed: Vec::new() });
        }
        let n = cmd.count.unwrap_or(1);
        match cmd.action {
            Action::Move(m) => self.move_by(m, cmd.count),
            Action::Operate { op, target } => self.operate_on(regs, op, target, cmd),
            Action::OnSelection { op, lines } => self.operate_selection(regs, op, lines, n, cmd.register),
            Action::OtherEnd => std::mem::swap(&mut self.cursor, &mut self.anchor),
            Action::Select { object, inner } => self.select(object, inner, n),
            Action::Insert(at) => self.insert_at(at, n),
            Action::Replace => {
                self.start_insert(n, false);
                if self.mode == Mode::Insert {
                    self.mode = Mode::Replace;
                }
            }
            Action::ReplaceChar(ch) => self.replace_chars(ch, n),
            Action::ToggleCase => self.toggle_case(n),
            Action::Join { spaces } => self.join(spaces, cmd.count),
            Action::Paste { before } => self.paste(regs, cmd.register, before, n),
            Action::Undo | Action::Redo => self.undo_redo(cmd.action == Action::Undo, n),
            Action::Repeat => self.repeat(regs, cmd.count),
            Action::Visual(kind) => self.visual_mode(kind),
            Action::CommandLine => {
                self.cmdline = if self.is_visual() { "'<,'>".into() } else { String::new() };
                self.exit_visual();
                self.mode = Mode::Command;
            }
            Action::NextChange => self.effects.push(Effect::NextChange),
            Action::PreviousChange => self.effects.push(Effect::PreviousChange),
            Action::Cancel => self.exit_visual(),
            Action::Increment { up } => self.increment(up, n),
            Action::Swallow => {}
        }
    }

    fn move_by(&mut self, m: Motion, count: Option<usize>) {
        let t = self.buf.text();
        let mut aim = Aim { want: self.want, last_find: &mut self.last_find, operator: false };
        let Some(reach) = motion::reach(m, t, self.cursor, count, &mut aim) else { return };
        self.cursor = reach.to;
        if m == Motion::LineEnd {
            self.want = usize::MAX;
        } else if !reach.vertical {
            self.want = self.buf.text().col(reach.to);
        }
    }

    fn operate_on(&mut self, regs: &mut Registers, op: Operator, target: Target, cmd: Command) {
        let t = self.buf.text();
        let n = cmd.count.unwrap_or(1);
        let (region, far) = match target {
            Target::Line => {
                let line = t.line_of(self.cursor);
                (region::of_lines(t, line, (line + n - 1).min(t.last_line())), false)
            }
            Target::Object { object, inner } => {
                let Some(span) = object::span(object, inner, t, self.cursor, n) else { return };
                (region::of_span(span), false)
            }
            Target::Motion(m) => {
                let reach = match m {
                    Motion::WordForward { big }
                        if op == Operator::Change && t.class_at(self.cursor, big) != Class::Space =>
                    {
                        Some(change_word(t, self.cursor, n, big))
                    }
                    _ => {
                        let mut aim = Aim { want: self.want, last_find: &mut self.last_find, operator: true };
                        motion::reach(m, t, self.cursor, cmd.count, &mut aim)
                    }
                };
                let Some(reach) = reach else { return };
                (region::of_reach(self.buf.text(), self.cursor, reach), reach.far)
            }
        };
        self.operate(regs, op, region, cmd.register, far, 1);
    }

    fn insert_at(&mut self, at: At, count: usize) {
        if self.buf.read_only() {
            return;
        }
        let t = self.buf.text();
        let line = t.line_of(self.cursor);
        match at {
            At::Cursor => {}
            At::After => {
                if t.char_at(self.cursor).is_some_and(|c| c != '\n') {
                    self.cursor = t.next(self.cursor);
                }
            }
            At::LineStart => self.cursor = t.first_nonblank(line),
            At::LineEnd => self.cursor = t.line_end(line),
            At::Column0 => self.cursor = t.line_start(line),
            At::LastInsert => self.cursor = self.last_insert_end.unwrap_or(self.cursor).min(t.len()),
            At::Below | At::Above => {
                let indent = t.slice(t.line_start(line), t.first_nonblank(line).min(t.line_end(line)));
                if at == At::Below {
                    let end = t.line_end(line);
                    self.buf.replace(end, end, &format!("\n{indent}"));
                    self.cursor = end + 1 + indent.len();
                } else {
                    let start = t.line_start(line);
                    self.buf.replace(start, start, &format!("{indent}\n"));
                    self.cursor = start + indent.len();
                }
                self.start_insert(count, true);
                return;
            }
        }
        self.start_insert(count, false);
    }

    pub(super) fn undo_redo(&mut self, undo: bool, n: usize) {
        let mut first = None;
        for _ in 0..n {
            let done = if undo { self.buf.undo() } else { self.buf.redo() };
            let Some(set) = done else { break };
            first = set.changes().first().map(|c| c.from).or(first);
        }
        if let Some(at) = first {
            self.cursor = at.min(self.buf.doc().len());
            self.want = self.buf.text().col(self.cursor);
        }
    }

    /// `.`: the last change again, with a count of its own if one is given.
    fn repeat(&mut self, regs: &mut Registers, count: Option<usize>) {
        let Some(mut dot) = self.dot.clone() else { return };
        if count.is_some() {
            dot.command.count = count;
            if let Some(stored) = &mut self.dot {
                stored.command.count = count;
            }
        }
        self.replaying = true;
        self.run(regs, dot.command);
        if matches!(self.mode, Mode::Insert | Mode::Replace) {
            for key in dot.typed {
                self.insert_key(regs, key);
            }
            self.insert_key(regs, Key::of(Code::Esc));
        }
        self.replaying = false;
    }

    fn visual_mode(&mut self, kind: Visual) {
        let (to, other) = match kind {
            Visual::Char => (Mode::Visual, Mode::VisualLine),
            Visual::Line => (Mode::VisualLine, Mode::Visual),
            Visual::Reselect => {
                if let Some((mode, anchor, cursor)) = self.last_visual {
                    self.mode = mode;
                    self.anchor = anchor;
                    self.cursor = cursor;
                }
                return;
            }
        };
        match self.mode {
            m if m == to => self.exit_visual(),
            m if m == other => self.mode = to,
            _ => {
                self.anchor = self.cursor;
                self.mode = to;
            }
        }
    }

    pub(super) fn exit_visual(&mut self) {
        if self.is_visual() {
            self.last_visual = Some((self.mode, self.anchor, self.cursor));
            self.mode = Mode::Normal;
        }
    }

    pub(super) fn selection_region(&self) -> Region {
        region::of_selection(self.buf.text(), self.anchor, self.cursor, self.mode == Mode::VisualLine)
    }
}
