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
    // Vim hears `Ctrl-n` and `Ctrl-p` before any other keymap in normal and
    // visual mode (EDITOR-VIM-6, EDITOR-KEYS-8); every other chord is not a motion.
    if first.ctrl {
        return match first.ctrl_char() {
            Some('p') => one(Motion::Up),
            Some('n') => one(Motion::Down),
            _ => Moved::Bad,
        };
    }
    match first.code {
        Code::Left => return one(Motion::Left),
        Code::Right => return one(Motion::Right),
        Code::Up => return one(Motion::Up),
        Code::Down => return one(Motion::Down),
        Code::Backspace => return one(Motion::Left),
        Code::Home => return one(Motion::LineStart),
        Code::End => return one(Motion::LineEnd),
        Code::Enter => return one(Motion::Plus),
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
        '%' => one(Motion::Matching),
        '+' => one(Motion::Plus),
        '-' => one(Motion::Minus),
        '_' => one(Motion::Underscore),
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
        'g' => match char_of(rest.first()) {
            None if rest.is_empty() => Moved::Need,
            Some(g) if rest.len() == 1 => match g {
                'g' => Moved::Done(Motion::FileStart),
                'e' => Moved::Done(Motion::WordEndBack { big: false }),
                'E' => Moved::Done(Motion::WordEndBack { big: true }),
                'j' => Moved::Done(Motion::Down),
                'k' => Moved::Done(Motion::Up),
                _ => Moved::Bad,
            },
            _ => Moved::Bad,
        },
        _ => Moved::Bad,
    }
}

#[cfg(test)]
mod tests;
