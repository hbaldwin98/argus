//! A dialog that answers its own keys: up while its overlay is, and it holds every key and runs
//! none (SHELL-KEYS-12), so typing in its fields never fires an action of the window's.
//!
//! The settings, the add-project dialog and an extension's question are these. Their Escape is the
//! dialog's own (the host closes it, `kit/overlay`), so no hook is put in the ladder for them. The
//! settings alone also answer their own chord, which puts them away (SHELL-SETTINGS-1).

use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::{App, Window};

use crate::keys::layer::{action_for, Handler, Press, Verdict};
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;

pub struct Own(pub Overlay);

impl Handler for Own {
    fn open(&self, _: &Window, cx: &App) -> bool {
        Overlays::get(cx).is_open(self.0)
    }

    fn keys(&self, press: &Press, _: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        if self.0 == Overlay::Settings && action_for(&[Place::Anywhere], press.page) == Some(Action::Settings) {
            Overlays::get(cx).close(Overlay::Settings, window, cx);
            return Verdict::Taken;
        }
        Verdict::Held
    }
}
