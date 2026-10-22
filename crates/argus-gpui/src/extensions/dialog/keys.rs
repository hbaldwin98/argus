//! The extension question's layer of the key walk: up only while something in the card has the keys
//! (EXTDIALOG-8), so a question raised on its own never holds the keys of a terminal being typed in.
//! While it is up it holds every key and runs the few that are the card's (EXTDIALOG-5, -6).
//!
//! Escape cancels; Enter and Ctrl-Enter answer, except that Enter with the keys on Cancel is a press
//! on Cancel; in a pick, the palette's up and down move the lit choice. A text field of the card
//! answers its own Enter and Escape (`keys/field`), before this layer hears them.

use argus_desktop_core::extensions::ask::{pressed, Pressed};
use argus_desktop_core::keys::keymap::{Action, Place};
use argus_protocol::ExtDialogKind;
use gpui_kit::{App, Window};

use super::Asking;
use crate::keys::layer::{action_for, Handler, Press, Verdict};

pub struct CardKeys;

fn card(cx: &App) -> Option<gpui_kit::Entity<super::card::AskView>> {
    cx.try_global::<Asking>().and_then(|asking| asking.card.clone())
}

impl Handler for CardKeys {
    fn open(&self, window: &Window, cx: &App) -> bool {
        card(cx).is_some_and(|card| card.read(cx).has_keys(window, cx))
    }

    fn keys(&self, press: &Press, _: &[Place], window: &mut Window, cx: &mut App) -> Verdict {
        let Some(card) = card(cx) else { return Verdict::Held };
        let (pick, on_cancel) = {
            let card = card.read(cx);
            (matches!(card.dialog.kind, ExtDialogKind::Pick { .. }), card.cancel.is_focused(window))
        };
        let bound = action_for(&[Place::Writing], press.page).or_else(|| action_for(&[Place::Palette], press.page).filter(|action| matches!(action, Action::Next | Action::Previous)));
        match pressed(bound, on_cancel, pick) {
            Pressed::Cancel => card.update(cx, |card, cx| card.cancel(window, cx)),
            Pressed::Submit => card.update(cx, |card, cx| card.submit(window, cx)),
            Pressed::Move(by) => card.update(cx, |card, cx| card.step(by, window, cx)),
            // Enter on a button is that button's click: Cancel's is the one that is not an answer.
            Pressed::Pass if on_cancel && bound == Some(Action::Choose) => card.update(cx, |card, cx| card.cancel(window, cx)),
            Pressed::Pass => return Verdict::Held,
        }
        Verdict::Taken
    }
}
