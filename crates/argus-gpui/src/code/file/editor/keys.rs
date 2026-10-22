//! Who hears which key while the editor has the keys: the editor is a text field, so the key walk's
//! `Field` layer holds every key for it and the window's own bindings (`j`, `]`, `E`, the stage
//! digits) are letters here. What the walk lets through is Zoom, which the window hears before any
//! field, and the few chords the editor adds to its own: write now, go to a file, search the
//! checkout, comment on the selection and step between changes (EDITOR-KEYS-1 to -4). With Vim on
//! it hears every key first, and the chords are the ones it leaves.

use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::keys::page_key::PageKey;
use gpui_kit::{Context, Window};

use super::CodeEditor;
use crate::keys::layer::{Press, Verdict};

/// What an editor chord asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chord {
    /// `Mod-s`: write now.
    Write,
    /// `Mod-p`: the stage's go-to-file field.
    GoToFile,
    /// `Mod-Shift-f`: the stage's search of the checkout. Plain `Ctrl F` stays the editor's find.
    Search,
    /// `Alt-c`: comment on the selection.
    Comment,
    /// `Alt-]` and `Alt-[`: the next and previous change.
    Step { forward: bool },
}

/// `Mod` is Ctrl on Windows and Linux, Cmd on macOS.
fn modified(key: &PageKey) -> bool {
    if cfg!(target_os = "macos") {
        key.meta && !key.ctrl
    } else {
        key.ctrl && !key.meta
    }
}

/// The editor chord `key` is, if it is one.
pub fn chord(key: &PageKey) -> Option<Chord> {
    let letter = |c: &str| key.key.eq_ignore_ascii_case(c);
    if modified(key) && !key.alt {
        return match (letter("s"), letter("p"), letter("f")) {
            (true, _, _) if !key.shift => Some(Chord::Write),
            (_, true, _) if !key.shift => Some(Chord::GoToFile),
            (_, _, true) if key.shift => Some(Chord::Search),
            _ => None,
        };
    }
    if key.alt && !key.ctrl && !key.meta && !key.shift {
        return match key.key.as_str() {
            "c" | "C" => Some(Chord::Comment),
            "]" => Some(Chord::Step { forward: true }),
            "[" => Some(Chord::Step { forward: false }),
            _ => None,
        };
    }
    None
}

impl CodeEditor {
    /// A key while the editor has the keys. A chord of the editor's is run and taken, so the
    /// editor's own handling never sees it; any other key is held, which is the editor's to type
    /// or bind.
    pub(super) fn key(&mut self, press: &Press, window: &mut Window, cx: &mut Context<Self>) -> Verdict {
        // Vim hears keys before any other keymap (EDITOR-VIM-1): what it takes is its own, and a
        // chord it does not own, such as `Mod-s`, goes on to the editor's.
        if self.vim_on {
            if let Some(verdict) = self.vim_key(press, window, cx) {
                return verdict;
            }
        }
        let Some(chord) = chord(press.page) else { return Verdict::Held };
        match chord {
            Chord::Write => {
                if let Some(key) = self.current.clone() {
                    self.write(&key, cx);
                }
            }
            Chord::GoToFile => self.stage_asks(Action::Filter),
            Chord::Search => self.stage_asks(Action::Search),
            Chord::Comment => {
                self.comment(cx);
            }
            Chord::Step { forward } => {
                self.step(forward, cx);
            }
        }
        Verdict::Taken
    }
}

#[cfg(test)]
mod tests;
