//! The reconcile dialog (BOARD-RECONCILE-1..21): what was asked of an agent, its proposal grouped Into
//! Argus and Out to each tracker with every line unticked, the line under the cursor in full (its exact
//! text, editable before ticking, and the call the agent will make), what it noticed and never acts on,
//! and, with nothing under way, the ask.
//!
//! The session holds the dialog (`Heard::reconcile`) and says everything on it: the stage, the lines in
//! the order they are listed, which are ticked, the text each carries. This draws that page and sends the
//! gestures of `ReconcileGesture`; it predicts none of their results. What it keeps is the person's
//! typing (the ask, and the words of the line open, each its own box), which a push never overwrites
//! while the box has the keys or the same line stays open (BOARD-RECONCILE-10), and the lines list's
//! scroll, which a push does not reset (BOARD-RECONCILE-16): the list's handle is the view's own, and
//! the line under the cursor is brought into view by the least movement when the cursor moves.
//!
//! The dialog is an overlay of the host that takes the keys from a pane. It is up exactly while the
//! session holds it: a click on the scrim does nothing (BOARD-RECONCILE-17), and Escape, Later and the
//! close button send `Close`.

mod ask;
mod body;
mod detail;
mod head;
mod lines;

use std::rc::Rc;

use argus_desktop_core::board::tracker::reconcile_dialog::{ReconcileGesture, ReconcilePage};
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{prelude::*, App, Context, Entity, FocusHandle, Focusable as _, ScrollHandle, Subscription, Window};

use super::keys::Sink;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::kit::scroll::reveal;
use crate::kit::textarea::Area;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// What the dialog showed last, to tell a change in it from a redraw.
#[derive(Default, Clone, Copy)]
struct Was {
    open: bool,
    /// The line under the cursor.
    lit: Option<usize>,
}

pub struct ReconcileView {
    heard: Entity<Heard>,
    caller: Caller,
    /// What to reconcile, typed while nothing is under way.
    ask: Area,
    /// The words of the line open, word for word.
    text: Area,
    /// Which line `text` holds: its words are its own while the same line stays open.
    text_line: Option<usize>,
    /// The lines list, which keeps its scroll across every push.
    lines: ScrollHandle,
    /// The dialog's own place for the keys, which no box has: Escape in a box rests them here.
    card: FocusHandle,
    was: Was,
    _subscriptions: Vec<Subscription>,
    _keys: Vec<Registered>,
}

impl ReconcileView {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ask = Area::new("Pull ENG-88\u{2019}s new sub-issues, mark 2.1 done in Linear, put D2 on the Jira ticket\u{2026}", 4, window, cx);
        let text = Area::new("", 4, window, cx);
        let subscriptions = vec![
            cx.observe_in(&heard, window, |this, _, window, cx| this.follow(window, cx)),
            // The words of the line open are sent on each keystroke; Rust keeps them only when they differ.
            cx.subscribe(text.state(), |this, _, event: &InputEvent, cx| {
                if let (InputEvent::Change, Some(line)) = (event, this.text_line) {
                    this.send(ReconcileGesture::Text { line, text: this.text.text(cx).to_string() });
                }
            }),
        ];
        // `Ctrl+Enter` sends the ask; Escape in either box gives the dialog its keys back, and closes
        // nothing (BOARD-RECONCILE-5, -10).
        let mut keys = Vec::new();
        let me = cx.weak_entity();
        keys.push(Fields::of(cx).register_place(window, ask.state().focus_handle(cx), Place::Writing, move |action, window, cx| {
            me.update(cx, |this, cx| match action {
                Action::Save => {
                    this.send(ReconcileGesture::Ask { ask: this.ask.text(cx).to_string() });
                    true
                }
                Action::CloseLayer => {
                    window.focus(&this.card, cx);
                    true
                }
                _ => false,
            })
            .unwrap_or(false)
        }));
        let me = cx.weak_entity();
        keys.push(Fields::of(cx).register_place(window, text.state().focus_handle(cx), Place::Writing, move |action, window, cx| {
            me.update(cx, |this, cx| match action {
                Action::CloseLayer => {
                    window.focus(&this.card, cx);
                    true
                }
                _ => false,
            })
            .unwrap_or(false)
        }));
        Self { heard, caller, ask, text, text_line: None, lines: ScrollHandle::new(), card: cx.focus_handle(), was: Was::default(), _subscriptions: subscriptions, _keys: keys }
    }

    /// The dialog's layer of the key walk (`board/tracker/install` registers it). `e` puts the keys in
    /// the open line's words, with the caret at the end, and is the page's own: nothing is sent.
    pub fn sink(view: &Entity<Self>) -> Sink {
        let view = view.downgrade();
        Rc::new(move |action: Action, window: &mut Window, cx: &mut App| {
            view.update(cx, |this, cx| {
                if action == Action::Edit {
                    this.edit(window, cx);
                } else {
                    this.send(ReconcileGesture::Action { action });
                }
            })
            .is_ok()
        })
    }

    /// What the session says the dialog is.
    pub fn page(&self, cx: &App) -> Option<ReconcilePage> {
        self.heard.read(cx).reconcile.clone()
    }

    fn send(&self, gesture: ReconcileGesture) {
        self.caller.call(Call::Reconcile { gesture });
    }

    /// `e`: the line's words take the keys, only while they can be edited.
    fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page(cx).is_some_and(|page| page.stage == "proposed" && page.selected.is_some()) {
            self.text.focus_at_end(window, cx);
        }
    }

    /// Brings the dialog up and takes it away as the session holds it, and lets the words of the line
    /// open follow what the page says of them.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = Overlays::get(cx);
        let Some(page) = self.page(cx) else {
            if std::mem::take(&mut self.was).open {
                self.ask.put("", window, cx);
                self.text.put("", window, cx);
                self.text_line = None;
            }
            host.close(Overlay::Reconcile, window, cx);
            return;
        };
        match &page.selected {
            // Another line open shows its own words; the same one keeps what is being written (BOARD-RECONCILE-10).
            Some(line) if self.text_line != Some(line.line) => {
                let writing = self.text.typing(window, cx);
                self.text.put(&line.text, window, cx);
                self.text_line = Some(line.line);
                // Writing goes on after the other line's last word, as it did after this one's.
                if writing {
                    self.text.focus_at_end(window, cx);
                }
            }
            Some(line) => {
                self.text.show(&line.text, window, cx);
            }
            None => self.text_line = None,
        }
        let lit = page.selected.as_ref().map(|line| line.line);
        let moved = self.was.lit != lit;
        self.was = Was { open: true, lit };
        if !host.is_open(Overlay::Reconcile) {
            self.open(window, cx);
        }
        if moved {
            // After this frame is drawn, so the row has said where it was.
            let view = cx.entity();
            window.defer(cx, move |_, cx| {
                view.update(cx, |this, cx| this.reveal_lit(cx));
            });
        }
        cx.notify();
    }

    /// The line under the cursor scrolls into view by the least movement (BOARD-RECONCILE-16).
    fn reveal_lit(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.page(cx) else { return };
        if let Some(at) = lines::child_of(&page, page.selected.as_ref().map(|line| line.line)) {
            if reveal(&self.lines, at) {
                cx.notify();
            }
        }
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        Overlays::get(cx).open(
            Spec::new(Overlay::Reconcile).dismissed(Dismiss::Never),
            move |dialog: Dialog, window, cx, _| {
                let tokens = *Tokens::global(cx);
                let viewport = window.viewport_size();
                // The page centres the sheet (`.scrim`'s `align-items: center`); the kit's default is a tenth down.
                let height = pixels(tokens.settings_h, window.rem_size()).min(viewport.height * tokens.sheet_max_share);
                dialog
                    .w(pixels(tokens.settings_w, window.rem_size()).min(viewport.width * tokens.settings_max_w_share))
                    .margin_top(((viewport.height - height) / 2.0).max(rem(0.0).to_pixels(window.rem_size())))
                    .p_0()
                    // Enter is the dialog's own (`keys`): carrying out, never a confirm of the kit's dialog.
                    .on_ok(|_, _, _| false)
                    .child(view.clone())
            },
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests;
