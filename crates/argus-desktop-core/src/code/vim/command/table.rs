//! The commands that are not a motion or an operator with a target: `r`,
//! `m`, `q`, `@`, the `z` family and the search and `:` lines, then those of
//! normal mode alone and of visual mode alone. Each says "not mine" with
//! none, so the parser goes on to the motions.

use super::super::key::{Code, Key};
use super::super::keymap::char_of;
use super::super::motion::Motion;
use super::super::object::Object;
use super::super::registers::Registers;
use super::super::screen::Place;
use super::{Action, At, Command, Operator, Parse, Target, Visual};

/// The character a key stands for as the argument of `r`, `m`, `q` and `@`.
fn argument(k: &Key) -> Option<char> {
    match k.code {
        Code::Enter => Some('\n'),
        _ if k.ctrl => None,
        _ => k.typed(),
    }
}

/// A command that normal and visual mode have alike: `r`, `m`, `q`, `@`, the
/// `z` family, the search and the `:` line.
pub(super) fn common(rest: &[Key], count: Option<usize>, register: Option<char>) -> Option<Parse> {
    let c = char_of(rest.first())?;
    let done = |action| Some(Parse::Done(Command { count, register, action }));
    let alone = rest.len() == 1;
    match c {
        ':' if alone => done(Action::CommandLine),
        '/' if alone => done(Action::Search { forward: true }),
        '?' if alone => done(Action::Search { forward: false }),
        'r' => match rest {
            [_] => Some(Parse::Need),
            [_, k] => argument(k).map_or(Some(Parse::Bad), |ch| done(Action::ReplaceChar(ch))),
            _ => Some(Parse::Bad),
        },
        'm' => match rest {
            [_] => Some(Parse::Need),
            [_, k] => argument(k).map_or(Some(Parse::Bad), |ch| done(Action::SetMark(ch))),
            _ => Some(Parse::Bad),
        },
        'q' => match rest {
            [_] => Some(Parse::Need),
            [_, k] => match argument(k) {
                Some(ch) if Registers::valid(ch) => done(Action::Record(ch)),
                _ => Some(Parse::Bad),
            },
            _ => Some(Parse::Bad),
        },
        '@' => match rest {
            [_] => Some(Parse::Need),
            [_, k] => argument(k).map_or(Some(Parse::Bad), |ch| done(Action::Play(ch))),
            _ => Some(Parse::Bad),
        },
        'z' => match rest {
            [_] => Some(Parse::Need),
            [_, k] => {
                let (at, first_nonblank) = match (k.code, k.typed()) {
                    (_, Some('z')) => (Place::Center, false),
                    (_, Some('.')) => (Place::Center, true),
                    (_, Some('t')) => (Place::Top, false),
                    (Code::Enter, _) => (Place::Top, true),
                    (_, Some('b')) => (Place::Bottom, false),
                    (_, Some('-')) => (Place::Bottom, true),
                    _ => return Some(Parse::Bad),
                };
                done(Action::Reveal { at, first_nonblank })
            }
            _ => Some(Parse::Bad),
        },
        _ => None,
    }
}

/// A command that only normal mode has.
pub(super) fn normal_command(rest: &[Key], count: Option<usize>, register: Option<char>) -> Option<Parse> {
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
        ('p', _) if alone => done(Action::Paste { before: false, match_indent: false }),
        ('P', _) if alone => done(Action::Paste { before: true, match_indent: false }),
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
        (']' | '[', _) => match rest {
            [_] => Some(Parse::Need),
            [_, k] if k.typed() == Some('c') => {
                done(if c == ']' { Action::NextChange } else { Action::PreviousChange })
            }
            [_, k] if k.typed() == Some('p') => done(Action::Paste { before: c == '[', match_indent: true }),
            // Any other symbol is a motion.
            _ => None,
        },
        ('g', Some('J')) if rest.len() == 2 => done(Action::Join { spaces: false }),
        ('g', Some('i')) if rest.len() == 2 => done(Action::Insert(At::LastInsert)),
        ('g', Some('I')) if rest.len() == 2 => done(Action::Insert(At::Column0)),
        ('g', Some('v')) if rest.len() == 2 => done(Action::Visual(Visual::Reselect)),
        _ => None,
    }
}

/// A command that only a selection has.
pub(super) fn visual_command(rest: &[Key], count: Option<usize>, register: Option<char>) -> Option<Parse> {
    let c = char_of(rest.first())?;
    let g = (c == 'g').then(|| char_of(rest.get(1))).flatten();
    let done = |action| Some(Parse::Done(Command { count, register, action }));
    let alone = rest.len() == 1;
    let on = |op, lines| Action::OnSelection { op, lines };
    match (c, g) {
        ('x', _) if alone => done(on(Operator::Delete, false)),
        ('X' | 'D', _) if alone => done(on(Operator::Delete, true)),
        ('s', _) if alone => done(on(Operator::Change, false)),
        ('C', _) if alone => done(on(Operator::Change, true)),
        ('S' | 'R', _) if alone => done(Action::ChangeLines),
        ('Y', _) if alone => done(on(Operator::Yank, true)),
        ('~', _) if alone => done(on(Operator::Swap, false)),
        ('u', _) if alone => done(on(Operator::Lower, false)),
        ('U', _) if alone => done(on(Operator::Upper, false)),
        ('J', _) if alone => done(Action::Join { spaces: true }),
        ('p', _) if alone => done(Action::Paste { before: false, match_indent: false }),
        ('P', _) if alone => done(Action::Paste { before: true, match_indent: false }),
        ('o', _) if alone => done(Action::OtherEnd { same_line: false }),
        ('O', _) if alone => done(Action::OtherEnd { same_line: true }),
        ('I', _) if alone => done(Action::Insert(At::SelectionStart)),
        ('A', _) if alone => done(Action::Insert(At::SelectionEnd)),
        ('v', _) if alone => done(Action::Visual(Visual::Char)),
        ('V', _) if alone => done(Action::Visual(Visual::Line)),
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
