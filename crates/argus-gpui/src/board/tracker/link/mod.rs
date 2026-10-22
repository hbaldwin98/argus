//! The tracker link dialog (BOARD-LINK-1..15): the URL of a ticket read as its tracker and key, what
//! it is to the feature, the tickets already linked to open or unlink, and the repository's tracker map
//! with its editor.
//!
//! The session holds the dialog (`Heard::link`) and says everything on it: what the URL reads as,
//! the linked tickets and the one under the cursor, the map, whether a link waits on the board. This
//! draws that page and sends the gestures of `LinkGesture`; it predicts none of their results. What
//! it keeps is the person's typing: the two fields and the map's box are its own entities, so the
//! dialog layer building its content again loses nothing, and Rust's echo of a field is written only
//! while the field has not the keys. Each keystroke sends both fields' text (BOARD-LINK-4); the fields
//! empty once the page says the link is on the board and nothing waits (BOARD-LINK-6).
//!
//! The dialog is an overlay of the host that takes the keys from a pane. It is up exactly while the
//! session holds it: a click on the scrim does nothing (BOARD-LINK-13), and Escape, Cancel and the
//! close button send `Close`, which the session answers by sending the dialog closed.

mod body;
mod map;
mod rows;

use std::rc::Rc;

use argus_desktop_core::board::tracker::link_dialog::{LinkGesture, LinkPage};
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Place};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{prelude::*, App, Context, Entity, FocusHandle, Focusable as _, ScrollHandle, Subscription, Window};

use super::keys::Sink;
use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::kit::field::Field;
use crate::kit::overlay::host::{Dismiss, Overlays, Spec};
use crate::kit::overlay::kind::Overlay;
use crate::kit::textarea::Area;
use crate::theme::scale::{pixels, rem};
use crate::theme::Tokens;

/// What the dialog showed last, to tell a change in it from a redraw.
#[derive(Default, Clone, Copy)]
struct Was {
    open: bool,
    pending: bool,
    editing: bool,
}

pub struct LinkView {
    heard: Entity<Heard>,
    caller: Caller,
    url: Field,
    role: Field,
    /// The map's box, while its editor is open.
    map: Area,
    /// The dialog's own place for the keys, which no field has: a click in it rests them here.
    card: FocusHandle,
    /// The body's scroll, which a push does not reset.
    body: ScrollHandle,
    was: Was,
    _subscriptions: Vec<Subscription>,
    _keys: Vec<Registered>,
}

impl LinkView {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let url = Field::new("https://linear.app/\u{2026}/issue/ENG-88/\u{2026}", window, cx);
        let role = Field::new("the work, the PM\u{2019}s ticket\u{2026} (optional)", window, cx);
        let map = Area::new("Linear: a feature is an issue, a task a sub-issue\u{2026}", 12, window, cx);
        let mut subscriptions = vec![cx.observe_in(&heard, window, |this, _, window, cx| this.follow(window, cx))];
        for field in [&url, &role] {
            subscriptions.push(cx.subscribe(field.input(), |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.send(LinkGesture::Fields { url: this.url.text(cx).to_string(), role: this.role.text(cx).to_string() });
                }
            }));
        }
        subscriptions.push(cx.subscribe(map.state(), |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.send(LinkGesture::MapText { text: this.map.text(cx).to_string() });
            }
        }));
        // In a field: Enter links, Escape closes (BOARD-LINK-5); in the map's box, Ctrl+Enter saves
        // and Escape puts the editor away, not the dialog (BOARD-LINK-10).
        let mut keys = Vec::new();
        for field in [url.clone(), role.clone()] {
            let me = cx.weak_entity();
            keys.push(Fields::of(cx).register_place(window, field.input().read(cx).focus_handle(cx), Place::Writing, move |action, _, cx| {
                me.update(cx, |this, cx| match action {
                    Action::Choose => {
                        this.link(cx);
                        true
                    }
                    Action::CloseLayer => {
                        this.send(LinkGesture::Close);
                        true
                    }
                    _ => false,
                })
                .unwrap_or(false)
            }));
        }
        let me = cx.weak_entity();
        keys.push(Fields::of(cx).register_place(window, map.state().focus_handle(cx), Place::Writing, move |action, _, cx| {
            me.update(cx, |this, cx| match action {
                Action::Save => {
                    this.send(LinkGesture::SaveMap { text: this.map.text(cx).to_string() });
                    true
                }
                Action::CloseLayer => {
                    this.send(LinkGesture::CancelMap);
                    true
                }
                _ => false,
            })
            .unwrap_or(false)
        }));
        Self { heard, caller, url, role, map, card: cx.focus_handle(), body: ScrollHandle::new(), was: Was::default(), _subscriptions: subscriptions, _keys: keys }
    }

    /// The dialog's layer of the key walk (`board/tracker/install` registers it).
    pub fn sink(view: &Entity<Self>) -> Sink {
        let view = view.downgrade();
        Rc::new(move |action: Action, _: &mut Window, cx: &mut App| view.update(cx, |this, _| this.send(LinkGesture::Action { action })).is_ok())
    }


    /// What the session says the dialog is.
    pub fn page(&self, cx: &App) -> Option<LinkPage> {
        self.heard.read(cx).link.clone()
    }

    fn send(&self, gesture: LinkGesture) {
        self.caller.call(Call::Link { gesture });
    }

    /// Link, with what the fields say as it is pressed (BOARD-LINK-6).
    fn link(&self, cx: &App) {
        self.send(LinkGesture::Link { url: self.url.text(cx).to_string(), role: self.role.text(cx).to_string() });
    }

    /// Brings the dialog up and takes it away as the session holds it, and lets its fields follow what
    /// the page says of them.
    fn follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = Overlays::get(cx);
        let Some(page) = self.page(cx) else {
            if std::mem::take(&mut self.was).open {
                for field in [&self.url, &self.role] {
                    field.put("", window, cx);
                }
                self.map.put("", window, cx);
            }
            host.close(Overlay::Link, window, cx);
            return;
        };
        let opening = !self.was.open;
        // A link the board now holds frees the fields for the next, even while the keys are in one.
        if self.was.pending && !page.pending && page.url.is_empty() && page.role.is_empty() {
            self.url.put("", window, cx);
            self.role.put("", window, cx);
        } else if opening {
            self.url.put(&page.url, window, cx);
            self.role.put(&page.role, window, cx);
        } else {
            self.url.show(&page.url, window, cx);
            self.role.show(&page.role, window, cx);
        }
        // The map's editor newly open takes the keys, on the text it opened with (BOARD-LINK-4).
        if let Some(text) = &page.editing_map {
            if !self.was.editing {
                self.map.put(text, window, cx);
                self.map.focus_at_end(window, cx);
            }
        }
        self.was = Was { open: true, pending: page.pending, editing: page.editing_map.is_some() };
        if !host.is_open(Overlay::Link) {
            self.open(window, cx);
        }
        cx.notify();
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (view, url, map) = (cx.entity(), self.url.clone(), self.map.clone());
        Overlays::get(cx).open(
            Spec::new(Overlay::Link).dismissed(Dismiss::Never),
            move |dialog: Dialog, window, cx, first| {
                // A dialog newly opened has the keys in the URL field, or in the map's box when it opens
                // straight on the editor (BOARD-LINK-4), once.
                if first {
                    if view.read(cx).page(cx).is_some_and(|page| page.editing_map.is_some()) {
                        map.focus_at_end(window, cx);
                    } else {
                        url.focus(window, cx);
                    }
                }
                let tokens = *Tokens::global(cx);
                let height = view.read(cx).height(&tokens, cx);
                dialog
                    .w(pixels(tokens.form_w, window.rem_size()))
                    .margin_top(((window.viewport_size().height - pixels(height, window.rem_size())) / 2.0).max(rem(tokens.sp_24).to_pixels(window.rem_size())))
                    .p_0()
                    // Enter in a field is no confirm of the dialog: only Link links.
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
