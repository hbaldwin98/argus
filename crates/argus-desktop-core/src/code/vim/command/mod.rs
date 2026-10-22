//! What the keys typed so far say: a register, a count, an operator and
//! what it acts on, or a command of its own. Reading is pure, so a half
//! typed command is just keys kept until the rest arrive, and a key that
//! can begin nothing is known at once.

pub mod table;

use super::key::{Code, Key};
use super::keymap::{char_of, motion, Moved};
use super::motion::Motion;
use super::object::Object;
use super::registers::Registers;
use super::screen::Place;

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
    /// `g?`
    Rot13,
    /// `gq`: wrap lines to the text width, the cursor on the last.
    Wrap,
    /// `gw`: the same, the cursor kept.
    WrapKeep,
    /// `gc`: toggle line comments.
    Comment,
    /// `=`
    Reindent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Motion(Motion),
    Object { object: Object, inner: bool },
    /// The operator doubled (`dd`): whole lines.
    Line,
    /// `gn` and `gN`: the next match of the search.
    Match { forward: bool },
    /// `/` and `?`: up to where a pattern typed next is found.
    Search { forward: bool },
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
    /// `gi`: where the last edit ended.
    LastInsert,
    /// `gI`: column 0.
    Column0,
    /// `I` in visual mode: before the selection, or each line of a block.
    SelectionStart,
    /// `A` in visual mode: after it.
    SelectionEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visual {
    Char,
    Line,
    Block,
    /// `gv`
    Reselect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Move(Motion),
    Operate { op: Operator, target: Target },
    /// An operator on the visual selection; `lines` takes whole lines.
    OnSelection { op: Operator, lines: bool },
    /// `o` in visual mode, and `O`, which keeps to the same line in a block.
    OtherEnd { same_line: bool },
    /// `S` and `R` on a selection: its whole lines are changed, a block too.
    ChangeLines,
    /// `iw` and the like in visual mode.
    Select { object: Object, inner: bool },
    /// `gn` and `gN`: the next match, taken into the selection.
    SelectMatch { forward: bool },
    Insert(At),
    /// `R`
    Replace,
    ReplaceChar(char),
    /// `~` in normal mode.
    ToggleCase,
    Join { spaces: bool },
    /// `p` and `P`; `]p` and `[p` fit the indent to the line.
    Paste { before: bool, match_indent: bool },
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
    /// `m<x>`
    SetMark(char),
    /// `q<x>`: record keys into a register.
    Record(char),
    /// `@<x>`: play a register, `@@` the last, `@:` the last ex command.
    Play(char),
    /// `Ctrl-o` and `Ctrl-i`.
    Jump { forward: bool },
    /// `Ctrl-e` and `Ctrl-y`: the view moves by lines, the cursor stays in it.
    Scroll { forward: bool },
    /// `zz`, `zt`, `zb`, and `z.`, `z<CR>`, `z-` which also go to the first
    /// non-blank: the cursor's line put in the view.
    Reveal { at: Place, first_nonblank: bool },
    /// `/` and `?`.
    Search { forward: bool },
    /// A chord Vim owns on main (EDITOR-VIM-21) that does nothing here
    /// (`Ctrl-w` in normal mode, `Ctrl-a` on a selection): taken, so the
    /// host's keymap never hears it.
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
        ('=', _) => (Operator::Reindent, '='),
        ('g', Some('~')) => (Operator::Swap, '~'),
        ('g', Some('u')) => (Operator::Lower, 'u'),
        ('g', Some('U')) => (Operator::Upper, 'U'),
        ('g', Some('?')) => (Operator::Rot13, '?'),
        ('g', Some('q')) => (Operator::Wrap, 'q'),
        ('g', Some('w')) => (Operator::WrapKeep, 'w'),
        ('g', Some('c')) => (Operator::Comment, 'c'),
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
    if rest.len() == 1 && first.typed() == Some(again) && !first.ctrl {
        return Ok((Target::Line, count));
    }
    if g && rest.len() == 2 && first.typed() == Some('g') && rest[1].typed() == Some(again) {
        return Ok((Target::Line, count));
    }
    if matches!(first.typed(), Some('/' | '?')) && rest.len() == 1 {
        return Ok((Target::Search { forward: first.typed() == Some('/') }, count));
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
        Moved::Done(Motion::Match { forward }) => Ok((Target::Match { forward }, count)),
        Moved::Done(m) => Ok((Target::Motion(m), count)),
    }
}

fn mul(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some((a.unwrap_or(1) * b.unwrap_or(1)).min(MOST)),
    }
}

/// How many of the keys are a count and a register before the command: `3`,
/// `"a`, `2"b`.
pub fn prefix_len(keys: &[Key]) -> usize {
    let mut i = 0;
    let mut counting = false;
    loop {
        match char_of(keys.get(i)) {
            Some('"') if keys.get(i + 1).is_some() => i += 2,
            Some(d) if d.is_ascii_digit() && (d != '0' || counting) => {
                counting = true;
                i += 1;
            }
            _ => return i,
        }
    }
}

/// Whether the last of the keys comes after an operator that is waiting for
/// its target: `d` then the key, `g~` then the key.
pub fn operator_pending(keys: &[Key]) -> bool {
    let rest = &keys[prefix_len(keys)..];
    let Some(first) = char_of(rest.first()) else { return false };
    match operator_of(first, char_of(rest.get(1))) {
        Some(_) => rest.len() > if first == 'g' { 2 } else { 1 },
        None => false,
    }
}

/// Whether the next key is the character `f`, `F`, `t`, `T` or `r` waits
/// for, which is its own whatever a `langmap` says.
pub fn wants_literal(keys: &[Key], visual: bool) -> bool {
    let Some((last, before)) = keys.split_last() else { return false };
    matches!(last.typed(), Some('f' | 'F' | 't' | 'T' | 'r'))
        && !last.ctrl
        && (before.is_empty() || parse(before, visual) == Parse::Need)
        && parse(keys, visual) == Parse::Need
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

    if first.ctrl && first.code != Code::Backspace {
        let alone = rest.len() == 1;
        match first.ctrl_char() {
            Some('r') if alone => return done(count, Action::Redo),
            Some('a') if alone => {
                return done(count, if visual { Action::Swallow } else { Action::Increment { up: true } })
            }
            Some('x') if alone => {
                return done(count, if visual { Action::Swallow } else { Action::Increment { up: false } })
            }
            Some('e') if alone => return done(count, if visual { Action::Swallow } else { Action::Scroll { forward: true } }),
            Some('y') if alone => {
                return done(count, if visual { Action::Swallow } else { Action::Scroll { forward: false } })
            }
            Some('v' | 'q') if alone => return done(count, Action::Visual(Visual::Block)),
            Some('o') if alone => return done(count, if visual { Action::Swallow } else { Action::Jump { forward: false } }),
            Some('i') if alone => return done(count, if visual { Action::Swallow } else { Action::Jump { forward: true } }),
            Some('w') if alone && !visual => return done(count, Action::Swallow),
            // These are motions, read below.
            Some('p' | 'n' | 'f' | 'b' | 'd' | 'u' | ' ') => {}
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
    let c = if first.ctrl { None } else { first.typed() };
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
    if let Some(p) = table::common(rest, count, register) {
        return p;
    }
    if visual {
        if let Some(p) = table::visual_command(rest, count, register) {
            return p;
        }
    } else if let Some(p) = table::normal_command(rest, count, register) {
        return p;
    }
    match motion(rest) {
        Moved::Need => Parse::Need,
        Moved::Bad => Parse::Bad,
        Moved::Done(Motion::Match { forward }) => done(count, Action::SelectMatch { forward }),
        Moved::Done(m) => done(count, Action::Move(m)),
    }
}

#[cfg(test)]
mod tests;
