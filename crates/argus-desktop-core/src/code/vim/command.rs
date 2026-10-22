//! What the keys typed so far say: a register, a count, an operator and
//! what it acts on, or a command of its own. Reading is pure, so a half
//! typed command is just keys kept until the rest arrive, and a key that
//! can begin nothing is known at once.

use super::key::{Code, Key};
use super::keymap::{char_of, motion, Moved};
use super::motion::Motion;
use super::object::Object;
use super::registers::Registers;

/// The most a count counts to; a larger one would only repeat past the end
/// of the text.
const MOST: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Yank,
    Change,
    Indent,
    Outdent,
    Swap,
    Lower,
    Upper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Motion(Motion),
    Object { object: Object, inner: bool },
    /// The operator doubled (`dd`): whole lines.
    Line,
}

/// Where an insert begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    Cursor,
    After,
    LineStart,
    LineEnd,
    Below,
    Above,
    /// `gi`: where the last insert ended.
    LastInsert,
    /// `gI`: column 0.
    Column0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visual {
    Char,
    Line,
    /// `gv`
    Reselect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Move(Motion),
    Operate { op: Operator, target: Target },
    /// An operator on the visual selection; `lines` takes whole lines.
    OnSelection { op: Operator, lines: bool },
    /// `o` in visual mode.
    OtherEnd,
    /// `iw` and the like in visual mode.
    Select { object: Object, inner: bool },
    Insert(At),
    /// `R`
    Replace,
    ReplaceChar(char),
    /// `~` in normal mode.
    ToggleCase,
    Join { spaces: bool },
    Paste { before: bool },
    Undo,
    Redo,
    Repeat,
    Visual(Visual),
    CommandLine,
    NextChange,
    PreviousChange,
    /// Escape: leaves what is left.
    Cancel,
    /// `Ctrl-a` and `Ctrl-x`: the number at or after the cursor, by the count.
    Increment { up: bool },
    /// A chord Vim owns on main (EDITOR-VIM-21) whose work needs the window
    /// (paging, scrolling, the jump list, a block selection) or that the
    /// library leaves idle: taken, so the host's keymap never hears it.
    Swallow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub count: Option<usize>,
    pub register: Option<char>,
    pub action: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parse {
    /// More keys are wanted.
    Need,
    /// These keys are no command.
    Bad,
    Done(Command),
}

fn operator_of(first: char, second: Option<char>) -> Option<(Operator, char)> {
    Some(match (first, second) {
        ('d', _) => (Operator::Delete, 'd'),
        ('y', _) => (Operator::Yank, 'y'),
        ('c', _) => (Operator::Change, 'c'),
        ('>', _) => (Operator::Indent, '>'),
        ('<', _) => (Operator::Outdent, '<'),
        ('g', Some('~')) => (Operator::Swap, '~'),
        ('g', Some('u')) => (Operator::Lower, 'u'),
        ('g', Some('U')) => (Operator::Upper, 'U'),
        _ => return None,
    })
}

/// What follows an operator: a count of its own, then a motion, an object,
/// or the operator again.
fn target(rest: &[Key], again: char, g: bool) -> Result<(Target, Option<usize>), Parse> {
    let mut i = 0;
    let mut count: Option<usize> = None;
    while let Some(d) = char_of(rest.get(i)).filter(|c| c.is_ascii_digit() && (*c != '0' || count.is_some())) {
        count = Some((count.unwrap_or(0) * 10 + d as usize - '0' as usize).min(MOST));
        i += 1;
    }
    let rest = &rest[i..];
    let Some(first) = rest.first() else { return Err(Parse::Need) };
    // `dd`, and `g~~` or `g~g~`.
    if rest.len() == 1 && first.typed() == Some(again) {
        return Ok((Target::Line, count));
    }
    if g && rest.len() == 2 && first.typed() == Some('g') && rest[1].typed() == Some(again) {
        return Ok((Target::Line, count));
    }
    if matches!(first.typed(), Some('i' | 'a')) && !first.ctrl {
        return match rest.get(1) {
            None => Err(Parse::Need),
            Some(k) if rest.len() == 2 => match k.typed().and_then(Object::named) {
                Some(object) => Ok((Target::Object { object, inner: first.typed() == Some('i') }, count)),
                None => Err(Parse::Bad),
            },
            Some(_) => Err(Parse::Bad),
        };
    }
    match motion(rest) {
        Moved::Need => Err(Parse::Need),
        Moved::Bad => Err(Parse::Bad),
        Moved::Done(m) => Ok((Target::Motion(m), count)),
    }
}

fn mul(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some((a.unwrap_or(1) * b.unwrap_or(1)).min(MOST)),
    }
}

/// Reads `keys`; `visual` is whether a selection is up.
pub fn parse(keys: &[Key], visual: bool) -> Parse {
    let mut i = 0;
    let mut register = None;
    let mut count: Option<usize> = None;
    loop {
        match char_of(keys.get(i)) {
            Some('"') => match keys.get(i + 1) {
                None => return Parse::Need,
                Some(k) => match k.typed() {
                    Some(r) if Registers::valid(r) => {
                        register = Some(r);
                        i += 2;
                    }
                    _ => return Parse::Bad,
                },
            },
            Some(d) if d.is_ascii_digit() && (d != '0' || count.is_some()) => {
                count = Some((count.unwrap_or(0) * 10 + d as usize - '0' as usize).min(MOST));
                i += 1;
            }
            _ => break,
        }
    }
    let rest = &keys[i..];
    let Some(first) = rest.first() else { return Parse::Need };
    let done = |count, action| Parse::Done(Command { count, register, action });

    if first.ctrl {
        let alone = rest.len() == 1;
        match first.ctrl_char() {
            Some('r') if alone => return done(count, Action::Redo),
            Some('a') if alone => {
                return done(count, if visual { Action::Swallow } else { Action::Increment { up: true } })
            }
            Some('x') if alone => {
                return done(count, if visual { Action::Swallow } else { Action::Increment { up: false } })
            }
            Some('f' | 'b' | 'd' | 'u' | 'e' | 'y' | 'v' | 'q' | 'o' | 'i' | 'w') if alone => {
                return done(count, Action::Swallow)
            }
            // Ctrl-p and Ctrl-n are motions, read below.
            Some('p' | 'n') => {}
            _ => return Parse::Bad,
        }
    }
    if first.code == Code::Esc {
        return if rest.len() == 1 { done(count, Action::Cancel) } else { Parse::Bad };
    }
    if first.code == Code::Insert && rest.len() == 1 {
        return done(count, Action::Insert(At::Cursor));
    }
    if first.code == Code::Delete && rest.len() == 1 {
        return if visual {
            done(count, Action::OnSelection { op: Operator::Delete, lines: false })
        } else {
            done(count, Action::Operate { op: Operator::Delete, target: Target::Motion(Motion::Right) })
        };
    }
    let c = first.typed();
    let second = char_of(rest.get(1));
    if let Some((op, again)) = c.and_then(|c| operator_of(c, second)) {
        let g = c == Some('g');
        if visual {
            let rest_len = if g { 2 } else { 1 };
            return match rest.len().cmp(&rest_len) {
                std::cmp::Ordering::Less => Parse::Need,
                std::cmp::Ordering::Equal => done(count, Action::OnSelection { op, lines: false }),
                std::cmp::Ordering::Greater => Parse::Bad,
            };
        }
        let after = &rest[if g { 2 } else { 1 }..];
        return match target(after, again, g) {
            Ok((t, c2)) => done(mul(count, c2), Action::Operate { op, target: t }),
            Err(e) => e,
        };
    }
    if c == Some('g') && rest.len() == 1 {
        return Parse::Need;
    }
    if visual {
        if let Some(p) = visual_command(rest, count, register) {
            return p;
        }
    } else if let Some(p) = normal_command(rest, count, register) {
        return p;
    }
    match motion(rest) {
        Moved::Need => Parse::Need,
        Moved::Bad => Parse::Bad,
        Moved::Done(m) => done(count, Action::Move(m)),
    }
}

/// A command that only normal mode has.
fn normal_command(rest: &[Key], count: Option<usize>, register: Option<char>) -> Option<Parse> {
    let c = char_of(rest.first())?;
    let g = (c == 'g').then(|| char_of(rest.get(1))).flatten();
    let done = |action| Some(Parse::Done(Command { count, register, action }));
    let alone = rest.len() == 1;
    let op = |op, motion| Action::Operate { op, target: Target::Motion(motion) };
    let line = |op| Action::Operate { op, target: Target::Line };
    match (c, g) {
        ('x', _) if alone => done(op(Operator::Delete, Motion::Right)),
        ('X', _) if alone => done(op(Operator::Delete, Motion::Left)),
        ('D', _) if alone => done(op(Operator::Delete, Motion::LineEnd)),
        ('C', _) if alone => done(op(Operator::Change, Motion::LineEnd)),
        ('s', _) if alone => done(op(Operator::Change, Motion::Right)),
        ('S', _) if alone => done(line(Operator::Change)),
        ('Y', _) if alone => done(line(Operator::Yank)),
        ('~', _) if alone => done(Action::ToggleCase),
        ('J', _) if alone => done(Action::Join { spaces: true }),
        ('p', _) if alone => done(Action::Paste { before: false }),
        ('P', _) if alone => done(Action::Paste { before: true }),
        ('i', _) if alone => done(Action::Insert(At::Cursor)),
        ('a', _) if alone => done(Action::Insert(At::After)),
        ('I', _) if alone => done(Action::Insert(At::LineStart)),
        ('A', _) if alone => done(Action::Insert(At::LineEnd)),
        ('o', _) if alone => done(Action::Insert(At::Below)),
        ('O', _) if alone => done(Action::Insert(At::Above)),
        ('R', _) if alone => done(Action::Replace),
        ('u', _) if alone => done(Action::Undo),
        ('.', _) if alone => done(Action::Repeat),
        ('v', _) if alone => done(Action::Visual(Visual::Char)),
        ('V', _) if alone => done(Action::Visual(Visual::Line)),
        (':', _) if alone => done(Action::CommandLine),
        ('r', _) => match rest {
            [_] => Some(Parse::Need),
            [_, k] => k.typed().map_or(Some(Parse::Bad), |ch| done(Action::ReplaceChar(ch))),
            _ => Some(Parse::Bad),
        },
        (']' | '[', _) => match rest {
            [_] => Some(Parse::Need),
            [_, k] if k.typed() == Some('c') => {
                done(if c == ']' { Action::NextChange } else { Action::PreviousChange })
            }
            _ => Some(Parse::Bad),
        },
        ('g', Some('J')) if rest.len() == 2 => done(Action::Join { spaces: false }),
        ('g', Some('i')) if rest.len() == 2 => done(Action::Insert(At::LastInsert)),
        ('g', Some('I')) if rest.len() == 2 => done(Action::Insert(At::Column0)),
        ('g', Some('v')) if rest.len() == 2 => done(Action::Visual(Visual::Reselect)),
        _ => None,
    }
}

/// A command that only a selection has.
fn visual_command(rest: &[Key], count: Option<usize>, register: Option<char>) -> Option<Parse> {
    let c = char_of(rest.first())?;
    let g = (c == 'g').then(|| char_of(rest.get(1))).flatten();
    let done = |action| Some(Parse::Done(Command { count, register, action }));
    let alone = rest.len() == 1;
    let on = |op, lines| Action::OnSelection { op, lines };
    match (c, g) {
        ('x', _) if alone => done(on(Operator::Delete, false)),
        ('X' | 'D', _) if alone => done(on(Operator::Delete, true)),
        ('s', _) if alone => done(on(Operator::Change, false)),
        ('C' | 'S' | 'R', _) if alone => done(on(Operator::Change, true)),
        ('Y', _) if alone => done(on(Operator::Yank, true)),
        ('~', _) if alone => done(on(Operator::Swap, false)),
        ('u', _) if alone => done(on(Operator::Lower, false)),
        ('U', _) if alone => done(on(Operator::Upper, false)),
        ('J', _) if alone => done(Action::Join { spaces: true }),
        ('p', _) if alone => done(Action::Paste { before: false }),
        ('P', _) if alone => done(Action::Paste { before: true }),
        ('o' | 'O', _) if alone => done(Action::OtherEnd),
        ('v', _) if alone => done(Action::Visual(Visual::Char)),
        ('V', _) if alone => done(Action::Visual(Visual::Line)),
        (':', _) if alone => done(Action::CommandLine),
        ('r', _) => match rest {
            [_] => Some(Parse::Need),
            [_, k] => k.typed().map_or(Some(Parse::Bad), |ch| done(Action::ReplaceChar(ch))),
            _ => Some(Parse::Bad),
        },
        ('i' | 'a', _) => match rest {
            [_] => Some(Parse::Need),
            [_, k] => match k.typed().and_then(Object::named) {
                Some(object) => done(Action::Select { object, inner: c == 'i' }),
                None => Some(Parse::Bad),
            },
            _ => Some(Parse::Bad),
        },
        ('g', Some('J')) if rest.len() == 2 => done(Action::Join { spaces: false }),
        ('g', Some('v')) if rest.len() == 2 => done(Action::Visual(Visual::Reselect)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
