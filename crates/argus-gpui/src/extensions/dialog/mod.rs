//! The question an extension puts to this window: a confirm, a one-line prompt, a pick from a list or
//! a small form, over the window with the scrim every dialog has (EXTDIALOG-1 to -9).
//!
//! The session holds the questions and says the oldest (`Heard::extension_dialog`); the dialog is up
//! while it does. A different id draws a new card; the same id is left as it is (the card reads the
//! count waiting behind it itself), so what is typed stays. One that answers a command the person
//! just ran takes the keys; one raised on its own is drawn but leaves them where they were, so a
//! terminal being typed in keeps every key until the person clicks into the card. Escape, the scrim,
//! the close button and Cancel send `answer: null`. An answer closes the dialog at once, and the
//! same question heard again before the session has dropped it does not put it back.

pub mod body;
pub mod card;
pub mod keys;

use std::rc::Rc;

use argus_desktop_core::extensions::dialog::ExtDialogPage;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::{prelude::*, App, Entity, Global, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::theme::scale::pixels;
use crate::theme::Tokens;
use card::AskView;

/// The card up, and the question last answered or cancelled.
#[derive(Default)]
pub struct Asking {
    pub(crate) card: Option<Entity<AskView>>,
    settled: Option<u64>,
}

impl Global for Asking {}

/// The question `id` is answered or put away: it is not put up again, and its dialog goes unless the
/// host is already taking it away (`closes` false: Escape and the scrim).
pub(super) fn settle(id: u64, closes: bool, window: &mut Window, cx: &mut App) {
    let asking = cx.default_global::<Asking>();
    asking.settled = Some(id);
    asking.card = None;
    if closes {
        Overlays::get(cx).close(Overlay::Extension, window, cx);
    }
}

/// Puts the dialog up or takes it away to match the question the session holds.
pub fn follow(heard: &Entity<Heard>, caller: &Caller, window: &mut Window, cx: &mut App) {
    let page = heard.read(cx).extension_dialog.clone();
    let host = Overlays::get(cx);
    let Some(page) = page else {
        let asking = cx.default_global::<Asking>();
        asking.settled = None;
        if asking.card.take().is_some() || host.is_open(Overlay::Extension) {
            host.close(Overlay::Extension, window, cx);
        }
        return;
    };
    let (settled, up) = {
        let asking = cx.default_global::<Asking>();
        (asking.settled, asking.card.clone())
    };
    // The same question is not drawn again: its card reads what changes.
    if settled == Some(page.id) || up.is_some_and(|card| card.read(cx).id == page.id) {
        return;
    }
    // Another question replaces the one up.
    if host.is_open(Overlay::Extension) {
        host.close(Overlay::Extension, window, cx);
    }
    open(heard, caller, &page, window, cx);
}

fn open(heard: &Entity<Heard>, caller: &Caller, page: &ExtDialogPage, window: &mut Window, cx: &mut App) {
    let (heard, caller, made) = (heard.clone(), caller.clone(), page.clone());
    let card = cx.new(|cx| AskView::new(heard, caller, &made, window, cx));
    cx.default_global::<Asking>().card = Some(card.clone());
    let dismissing = card.clone();
    let mut spec = Spec::new(Overlay::Extension).dismissed(Dismiss::Answers(Rc::new(move |window, cx| {
        // Escape and the scrim: the question is put away unanswered, and the host closes it.
        dismissing.update(cx, |card, cx| card.answered(None, false, window, cx));
    })));
    if !page.prompted {
        spec = spec.keeping_focus();
    }
    Overlays::get(cx).open(
        spec,
        move |dialog: Dialog, window, cx, first| {
            if first {
                AskView::take_keys(&card, window, cx);
            }
            let tokens = *Tokens::global(cx);
            let (width, height) = card.read(cx).size(&tokens);
            dialog
                .w(pixels(width, window.rem_size()))
                .p_0()
                .margin_top((window.viewport_size().height - pixels(height, window.rem_size())) / 2.0)
                // Enter is the card's own (`keys`), never a confirm of the dialog.
                .on_ok(|_, _, _| false)
                .child(card.clone())
        },
        window,
        cx,
    );
}

#[cfg(test)]
mod tests;
