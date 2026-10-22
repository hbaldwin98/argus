//! Running a command that has been read: a motion moves the cursor, an
//! operator is given the text its target names, and the rest are the
//! commands of their own. A command that changes the text is kept for `.`.

use super::command::{Action, At, Command, Operator, Target};
use super::key::{Code, Key};
use super::motion::aim::Aim;
use super::motion::{self, Motion, Reach};
use super::prompt::Then;
use super::region;
use super::screen::Place;
use super::text::{Class, Text};
use super::{object, word, Dot, Effect, Mode, Shape, Shared, Vim};

/// Whether the command, run in `visual` or not, is what `.` repeats. The
/// library keeps every operator, a yank too, and the actions that edit.
fn repeatable(action: Action, visual: bool) -> bool {
    match action {
        Action::Operate { target: Target::Search { .. }, .. } => false,
        Action::Operate { .. } | Action::OnSelection { .. } | Action::ChangeLines => true,
        Action::Join { .. } | Action::Paste { .. } | Action::ReplaceChar(_) => true,
        Action::Insert(At::SelectionStart | At::SelectionEnd) => false,
        Action::Insert(_) | Action::Replace | Action::ToggleCase | Action::Increment { .. } => !visual,
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

/// What a motion is given to know of the editor.
macro_rules! aim {
    ($vim:expr, $sh:expr, $operator:expr, $count:expr) => {
        Aim {
            want: $vim.want,
            last_find: &mut $sh.find,
            operator: $operator,
            view: $vim.view,
            wrap: $vim.wrap,
            marks: &$vim.buf.marks,
            search: $sh.search.query.as_ref(),
            reversed: $sh.search.reversed,
            pcre: $vim.locals.pcre.unwrap_or($sh.options.pcre),
            message: &mut $vim.message,
            explicit: $count.is_some(),
        }
    };
}

impl Vim {
    pub(super) fn run(&mut self, sh: &mut Shared, cmd: Command) {
        if !self.replaying && !sh.macros.playing() && repeatable(cmd.action, self.is_visual()) {
            self.dot = Some(Dot { command: cmd, typed: Vec::new(), shape: self.shape_of(cmd.action) });
        }
        let n = cmd.count.unwrap_or(1);
        match cmd.action {
            Action::Move(m) => self.move_by(sh, m, cmd.count),
            Action::Operate { op, target } => self.operate_on(sh, op, target, cmd),
            Action::OnSelection { op, lines } => self.operate_selection(sh, op, lines, n, cmd.register),
            Action::ChangeLines => self.change_lines(sh, n, cmd.register),
            Action::OtherEnd { same_line } => self.other_end(same_line),
            Action::Select { object, inner } => self.select(object, inner, n),
            Action::SelectMatch { forward } => self.select_match(sh, forward, n),
            Action::Insert(at) => self.insert_at(sh, at, n),
            Action::Replace => {
                self.start_insert(n, false);
                if self.mode == Mode::Insert {
                    self.mode = Mode::Replace;
                }
            }
            Action::ReplaceChar(ch) => self.replace_chars(ch, n),
            Action::ToggleCase => self.toggle_case(n),
            Action::Join { spaces } => self.join(spaces, cmd.count),
            Action::Paste { before, match_indent } => self.paste(sh, cmd.register, before, match_indent, n),
            Action::Undo | Action::Redo => self.undo_redo(cmd.action == Action::Undo, n),
            Action::Repeat => self.repeat(sh, cmd.count),
            Action::Visual(kind) => self.visual_mode(kind),
            Action::CommandLine => {
                let text = if self.is_visual() {
                    "'<,'>".to_owned()
                } else if n > 1 {
                    format!(".,.+{}", n - 1)
                } else {
                    String::new()
                };
                self.exit_visual();
                self.open_ex(&text);
            }
            Action::NextChange => self.effects.push(Effect::NextChange),
            Action::PreviousChange => self.effects.push(Effect::PreviousChange),
            Action::Cancel => {
                // Escape in normal mode takes the matches' marks away.
                if !self.is_visual() {
                    sh.search.highlight = false;
                }
                self.exit_visual();
            }
            Action::Increment { up } => self.increment(up, n),
            Action::SetMark(name) => self.set_mark(name),
            Action::Record(name) => self.start_recording(sh, name),
            Action::Play(name) => self.play(sh, name, n),
            Action::Jump { forward } => self.jump_walk(forward, n),
            Action::Scroll { forward } => self.scroll_lines(forward, n),
            Action::Reveal { at, first_nonblank } => self.reveal(at, first_nonblank),
            Action::Search { forward } => self.open_search(sh, forward, Then::Move { count: cmd.count }),
            Action::Swallow => {}
        }
    }

    /// A motion that `*` is, once the word is the pattern: the one that finds
    /// the next match. Any other is itself.
    fn resolved(&mut self, sh: &mut Shared, m: Motion) -> Option<Motion> {
        match m {
            Motion::Star { forward, whole } => self.star(sh, forward, whole),
            m => Some(m),
        }
    }

    pub(super) fn move_by(&mut self, sh: &mut Shared, m: Motion, count: Option<usize>) {
        let Some(m) = self.resolved(sh, m) else { return };
        let t = self.buf.text();
        let from = self.cursor;
        let mut aim = aim!(self, sh, false, count);
        let reach = motion::reach(m, t, self.cursor, count, &mut aim);
        let origin = self.jump_from.take().unwrap_or(from);
        let Some(reach) = reach else { return };
        if m.jumps() {
            self.jumped(origin, reach.to);
        }
        self.cursor = reach.to;
        if m == Motion::LineEnd {
            self.want = usize::MAX;
        } else if !reach.vertical {
            self.want = self.buf.text().col(reach.to);
        }
        if let Motion::HalfPage { forward } = m {
            // The view goes as far as the cursor did.
            let t = self.buf.text();
            let moved = t.line_of(from).abs_diff(t.line_of(reach.to));
            let view = self.view.within(t.last_line());
            let line = if forward { view.top + moved } else { view.top.saturating_sub(moved) };
            self.effects.push(Effect::Reveal { line, at: Place::Top });
        }
    }

    fn operate_on(&mut self, sh: &mut Shared, op: Operator, target: Target, cmd: Command) {
        let n = cmd.count.unwrap_or(1);
        let (region, far) = match target {
            Target::Search { forward } => {
                self.open_search(sh, forward, Then::Operate(cmd));
                return;
            }
            Target::Line => {
                let t = self.buf.text();
                let line = t.line_of(self.cursor);
                (region::of_lines(t, line, (line + n - 1).min(t.last_line())), false)
            }
            Target::Object { object, inner } => {
                let t = self.buf.text();
                let Some(span) = object::span(object, inner, t, self.cursor, n) else { return };
                (region::of_span(span), false)
            }
            Target::Match { forward } => {
                let Some(region) = self.match_region(sh, forward, n) else { return };
                (region, false)
            }
            Target::Motion(m) => {
                let Some(m) = self.resolved(sh, m) else { return };
                let t = self.buf.text();
                let from = self.cursor;
                let reach = match m {
                    Motion::WordForward { big }
                        if op == Operator::Change && t.class_at(self.cursor, big) != Class::Space =>
                    {
                        Some(change_word(t, self.cursor, n, big))
                    }
                    _ => {
                        let mut aim = aim!(self, sh, true, cmd.count);
                        motion::reach(m, t, self.cursor, cmd.count, &mut aim)
                    }
                };
                let origin = self.jump_from.take().unwrap_or(from);
                let Some(reach) = reach else { return };
                if m.jumps() {
                    self.jumped(origin, reach.to);
                }
                (region::of_reach(self.buf.text(), self.cursor, reach), reach.far)
            }
        };
        self.operate(sh, op, region, cmd.register, far, 1);
    }

    fn insert_at(&mut self, sh: &mut Shared, at: At, count: usize) {
        let _ = sh;
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
            At::LastInsert => {
                self.cursor = self.buf.marks.last_edit().unwrap_or(self.cursor).min(t.len());
            }
            At::SelectionStart | At::SelectionEnd => {
                self.insert_at_selection(at == At::SelectionEnd);
                return;
            }
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
            // A selection's other end moves with the text, as the rest of the text does.
            self.anchor = set.map_pos(self.anchor, -1);
            first = set.changes().first().map(|c| c.from).or(first);
        }
        if let Some(at) = first {
            self.cursor = at.min(self.buf.doc().len());
            self.want = self.buf.text().col(self.cursor);
        }
    }

    /// How big the selection an operator is about to act on is.
    fn shape_of(&self, action: Action) -> Option<Shape> {
        if !self.is_visual() || !matches!(action, Action::OnSelection { .. } | Action::ChangeLines) {
            return None;
        }
        let t = self.buf.text();
        let ((la, ca), (lc, cc)) = (t.pos(self.anchor), t.pos(self.cursor));
        Some(Shape { mode: self.mode, lines: la.abs_diff(lc), cols: ca.abs_diff(cc) })
    }

    /// A selection of `shape`, made from the cursor as the library makes
    /// one to repeat an operator on: the same lines, and in a block or on
    /// one line the same columns.
    fn reselect_like(&mut self, shape: Shape) {
        let t = self.buf.text();
        let (line, col) = t.pos(self.cursor);
        let head_line = (line + shape.lines).min(t.last_line());
        let head_col = match shape.mode {
            Mode::VisualBlock => col + shape.cols,
            Mode::Visual if shape.lines == 0 => col + shape.cols,
            _ => col,
        };
        self.anchor = self.cursor;
        self.cursor = t.at_col(head_line, head_col);
        self.mode = shape.mode;
    }

    /// `.`: the last change again, with a count of its own if one is given.
    fn repeat(&mut self, sh: &mut Shared, count: Option<usize>) {
        let Some(mut dot) = self.dot.clone() else { return };
        if count.is_some() {
            dot.command.count = count;
            if let Some(stored) = &mut self.dot {
                stored.command.count = count;
            }
        }
        self.replaying = true;
        if let Some(shape) = dot.shape {
            self.reselect_like(shape);
        }
        self.run(sh, dot.command);
        if matches!(self.mode, Mode::Insert | Mode::Replace) {
            for key in dot.typed {
                self.insert_key_plain(sh, key);
            }
            self.insert_key_plain(sh, Key::of(Code::Esc));
        }
        self.replaying = false;
    }
}
