//! The dialog's two layers of the key walk: the list's (`Sessions`) and, while the full transcript is
//! open in its place, the reader's (`Transcript`). Each is up while the dialog is, holds every key
//! and runs the ones the keymap binds in its place (SESSIONS-6, -11, -20, -21; TRANSCRIPT-7).
//!
//! Every action is sent to the session as `SessionsAction` and changes nothing on screen. The two
//! the window answers itself are `/` (the search field, or the reader's find field, takes the keys,
//! nothing is sent) and `?`, which opens the shortcuts sheet over the dialog. A filter menu drawn
//! over the dialog is the kit's: while one is open the key is passed on to the Menu layer, whose
//! Escape closes the menu only and leaves the dialog up, though `LAYERS` lists Sessions first
//! (SESSIONS-10); and the layer's own Escape gives way to it as well.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, Entity, WeakEntity, Window};

use super::SessionsView;
use crate::agents::ask::ask;
use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::keys::run;
use crate::kit::menu::stepping;
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;

pub struct SessionsKeys {
    pub view: WeakEntity<SessionsView>,
    /// Whether this is the reader's layer: the dialog is up with its full transcript open.
    pub reading: bool,
}

/// What a key is in the dialog: its place's action, or Escape and the sheet's `?` from `Anywhere`.
/// Nothing else of `Anywhere` is the dialog's (SHELL-KEYS-14).
pub fn action(reading: bool, press: &Press) -> Option<Action> {
    let place = if reading { Place::Transcript } else { Place::Sessions };
    action_for(&[place], press.page).or_else(|| action_for(&[Place::Anywhere], press.page).filter(|action| matches!(action, Action::CloseLayer | Action::Shortcuts)))
}

impl SessionsKeys {
    fn view(&self) -> Option<Entity<SessionsView>> {
        self.view.upgrade()
    }

    fn send(view: &Entity<SessionsView>, action: Action, cx: &mut App) {
        view.update(cx, |this, cx| ask(&this.heard, &this.caller, Call::SessionsAction { action, row: None }, cx));
    }
}

impl Handler for SessionsKeys {
    fn open(&self, _: &Window, cx: &App) -> bool {
        Overlays::get(cx).is_open(Overlay::Sessions) && self.view().is_some_and(|view| view.read(cx).reading(cx) == self.reading)
    }

    fn keys(&self, press: &Press, _: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        // A menu opened from the dialog hears the key first.
        if stepping::is_open(window) {
            return Verdict::Pass;
        }
        let Some(view) = self.view() else { return Verdict::Held };
        match action(self.reading, press) {
            Some(Action::Shortcuts) => {
                run::run(Action::Shortcuts, window, cx);
                Verdict::Taken
            }
            Some(Action::Filter) => {
                view.update(cx, |this, cx| if self.reading { this.focus_find(window, cx) } else { this.focus_search(window, cx) });
                Verdict::Taken
            }
            Some(action) => {
                Self::send(&view, action, cx);
                Verdict::Taken
            }
            None => Verdict::Held,
        }
    }

    /// Escape puts the reader away, then the dialog, as the session answers `closeLayer`; a menu
    /// that is open has it first.
    fn escape(&self, window: &mut Window, cx: &mut App) -> bool {
        if stepping::is_open(window) || !self.open(window, cx) {
            return false;
        }
        match self.view() {
            Some(view) => {
                Self::send(&view, Action::CloseLayer, cx);
                true
            }
            None => false,
        }
    }
}

