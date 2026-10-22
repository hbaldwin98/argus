//! Which keys name a motion: `h`, `gg`, `fx`, the arrows and the rest, read
//! from exactly the keys given, so a half typed motion is known as one.

use super::key::{Code, Key};
use super::motion::{Find, Motion};

pub enum Moved {
    Need,
    Bad,
    Done(Motion),
}

pub fn char_of(k: Option<&Key>) -> Option<char> {
    k.and_then(|k| k.typed())
}

/// A motion that `keys` spell exactly.
pub fn motion(keys: &[Key]) -> Moved {
    let Some(first) = keys.first() else { return Moved::Need };
    let rest = &keys[1..];
    let one = |m: Motion| if rest.is_empty() { Moved::Done(m) } else { Moved::Bad };
    // Vim hears these chords before any other keymap in normal and visual
    // mode (EDITOR-VIM-6, -21, EDITOR-KEYS-8); every other chord is not a
    // motion.
    if first.ctrl {
        if first.code == Code::Backspace {
            return one(Motion::WordBack { big: true });
        }
        return match first.ctrl_char() {
            Some('p') => one(Motion::Up),
            Some('n') => one(Motion::Down),
            Some('f') => one(Motion::Page { forward: true }),
            Some('b') => one(Motion::Page { forward: false }),
            Some('d') => one(Motion::HalfPage { forward: true }),
            Some('u') => one(Motion::HalfPage { forward: false }),
            Some(' ') => one(Motion::WordForward { big: true }),
            _ => Moved::Bad,
        };
    }
    match first.code {
        Code::Left => return one(Motion::Left),
        Code::Right => return one(Motion::Right),
        Code::Up => return one(Motion::Up),
        Code::Down => return one(Motion::Down),
        Code::Backspace if first.shift => return one(Motion::WordBack { big: false }),
        Code::Backspace => return one(Motion::Left),
        Code::Home => return one(Motion::LineStart),
        Code::End => return one(Motion::LineEnd),
        Code::PageUp => return one(Motion::Page { forward: false }),
        Code::PageDown => return one(Motion::Page { forward: true }),
        Code::Enter => return one(Motion::Plus),
        Code::Char(' ') if first.shift => return one(Motion::WordForward { big: false }),
        _ => {}
    }
    let Some(c) = first.typed() else { return Moved::Bad };
    match c {
        'h' => one(Motion::Left),
        'l' | ' ' => one(Motion::Right),
        'j' => one(Motion::Down),
        'k' => one(Motion::Up),
        '0' => one(Motion::LineStart),
        '^' => one(Motion::FirstNonBlank),
        '$' => one(Motion::LineEnd),
        '|' => one(Motion::Column),
        'w' => one(Motion::WordForward { big: false }),
        'W' => one(Motion::WordForward { big: true }),
        'e' => one(Motion::WordEnd { big: false }),
        'E' => one(Motion::WordEnd { big: true }),
        'b' => one(Motion::WordBack { big: false }),
        'B' => one(Motion::WordBack { big: true }),
        'G' => one(Motion::FileEnd),
        ';' => one(Motion::RepeatFind { reverse: false }),
        ',' => one(Motion::RepeatFind { reverse: true }),
        '{' => one(Motion::ParagraphBack),
        '}' => one(Motion::ParagraphForward),
        '(' => one(Motion::Sentence { forward: false }),
        ')' => one(Motion::Sentence { forward: true }),
        '%' => one(Motion::Matching),
        '+' => one(Motion::Plus),
        '-' => one(Motion::Minus),
        '_' => one(Motion::Underscore),
        'H' => one(Motion::Top),
        'M' => one(Motion::Middle),
        'L' => one(Motion::Bottom),
        'n' => one(Motion::SearchNext { forward: true }),
        'N' => one(Motion::SearchNext { forward: false }),
        '*' => one(Motion::Star { forward: true, whole: true }),
        '#' => one(Motion::Star { forward: false, whole: true }),
        'f' | 'F' | 't' | 'T' => {
            let kind = match c {
                'f' => Find::To,
                'F' => Find::ToBack,
                't' => Find::Till,
                _ => Find::TillBack,
            };
            match rest {
                [] => Moved::Need,
                [k] => k.typed().map_or(Moved::Bad, |ch| Moved::Done(Motion::Find(kind, ch))),
                _ => Moved::Bad,
            }
        }
        '\'' | '`' => match rest {
            [] => Moved::Need,
            [k] => match k.typed() {
                Some(name) => Moved::Done(Motion::Mark { name, linewise: c == '\'' }),
                None => Moved::Bad,
            },
            _ => Moved::Bad,
        },
        ']' | '[' => match rest {
            [] => Moved::Need,
            [k] => match k.typed() {
                Some(m @ ('\'' | '`')) => Moved::Done(Motion::JumpMark { forward: c == ']', linewise: m == '\'' }),
                Some(ch) => Moved::Done(Motion::Symbol { forward: c == ']', ch }),
                None => Moved::Bad,
            },
            _ => Moved::Bad,
        },
        'g' => match rest {
            [] => Moved::Need,
            [k] if k.code == Code::Up => Moved::Done(Motion::Display { forward: false }),
            [k] if k.code == Code::Down => Moved::Done(Motion::Display { forward: true }),
            [k] => match k.typed() {
                Some('g') => Moved::Done(Motion::FileStart),
                Some('e') => Moved::Done(Motion::WordEndBack { big: false }),
                Some('E') => Moved::Done(Motion::WordEndBack { big: true }),
                Some('j') => Moved::Done(Motion::Display { forward: true }),
                Some('k') => Moved::Done(Motion::Display { forward: false }),
                Some('0' | '^') => Moved::Done(Motion::DisplayStart),
                Some('$') => Moved::Done(Motion::DisplayEnd),
                Some('n') => Moved::Done(Motion::Match { forward: true }),
                Some('N') => Moved::Done(Motion::Match { forward: false }),
                Some('*') => Moved::Done(Motion::Star { forward: true, whole: false }),
                Some('#') => Moved::Done(Motion::Star { forward: false, whole: false }),
                _ => Moved::Bad,
            },
            _ => Moved::Bad,
        },
        _ => Moved::Bad,
    }
}

#[cfg(test)]
mod tests;
