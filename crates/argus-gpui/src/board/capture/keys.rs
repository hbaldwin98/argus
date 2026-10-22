//! The Capture layer of the key walk: up while a note is open, hearing the window's own keys (the
//! stage switches) and holding every other, since the note's field takes the rest; Escape discards
//! the note (BOARD-CAPTURE-7, -12).
//!
//! The field answers `Create` and `Discard` itself while it has the keys (`Writing`), so this layer is
//! for the keys while something else on the stage has them.

use argus_desktop_core::keys::keymap::Place;
use gpui_kit::{App, WeakEntity, Window};

use super::CaptureNote;
use crate::keys::layer::{Handler, Press, Verdict};
use crate::keys::layers::stage::Stage;

pub struct NoteKeys {
    pub note: WeakEntity<CaptureNote>,
}

impl Handler for NoteKeys {
    fn open(&self, _: &Window, cx: &App) -> bool {
        self.note.upgrade().is_some_and(|note| note.read(cx).is_open(cx))
    }

    fn keys(&self, press: &Press, places: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        Stage.keys(press, places, window, cx)
    }

    fn escape(&self, window: &mut Window, cx: &mut App) -> bool {
        let Some(note) = self.note.upgrade() else { return false };
        if !note.read(cx).is_open(cx) {
            return false;
        }
        note.update(cx, |note, cx| note.discard(window, cx));
        true
    }
}
