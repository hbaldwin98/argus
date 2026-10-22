//! A key the page reports, as Vim's key: what `KeyboardEvent.key` and the modifiers are to the
//! library the page's editor runs. A window whose keys come as the page's (`keys::page_key`) feeds
//! Vim through this, so the two windows read a keystroke alike.

use super::key::{Code, Key};
use crate::keys::page_key::PageKey;

/// Vim's key for a page key, or `None` for one Vim is never sent: a bare modifier, a function
/// key, a dead key, and anything held with Alt or the command key, which the editor's own keymap
/// and the window answer (EDITOR-KEYS-3, -4).
pub fn vim_key(page: &PageKey) -> Option<Key> {
    if page.alt || page.meta {
        return None;
    }
    let named = |code| Some(Key { code, ctrl: page.ctrl, shift: false });
    // Space and Backspace are the keys a Shift tells apart (`<S-Space>` is `w`, `<S-BS>` is `b`).
    let told = |code| Some(Key { code, ctrl: page.ctrl, shift: page.shift });
    match page.key.as_str() {
        "Escape" => Some(Key::of(Code::Esc)),
        "Enter" => named(Code::Enter),
        "Tab" => named(Code::Tab),
        "Delete" => named(Code::Delete),
        "Insert" => named(Code::Insert),
        "ArrowLeft" => named(Code::Left),
        "ArrowRight" => named(Code::Right),
        "ArrowUp" => named(Code::Up),
        "ArrowDown" => named(Code::Down),
        "Home" => named(Code::Home),
        "End" => named(Code::End),
        "PageUp" => named(Code::PageUp),
        "PageDown" => named(Code::PageDown),
        "Backspace" => told(Code::Backspace),
        " " if page.ctrl => Some(Key::ctrl(' ')),
        " " => told(Code::Char(' ')),
        text => {
            let mut chars = text.chars();
            let c = chars.next().filter(|_| chars.next().is_none())?;
            if c.is_control() {
                return None;
            }
            if !page.ctrl {
                return Some(Key::ch(c));
            }
            // Ctrl-Shift and a letter is no key of the library's, so the editor's own keys have
            // it (`Mod-Shift-f` is the checkout's search, not a page down).
            (!(page.shift && c.is_alphabetic())).then(|| Key::ctrl(c))
        }
    }
}

#[cfg(test)]
mod tests;
