//! The history's filter field: what the person types to narrow the commits.
//!
//! Each edit sends `FilterHistory` with the text as it stands and no debounce,
//! as the page does (HISTORY-12); the session filters, so the rows change when
//! its next `History` event says so. The text is the field's own while the
//! history is open: the session's echo of a letter can land after the next is
//! typed, so the page's filter is set only as the history opens, and the field
//! is emptied when it shuts (SHARED-4).
//!
//! Enter and Escape in the field give the keys back to the stage, filtered, and
//! Escape does not close the history from here (HISTORY-13): both are caught
//! ahead of the input's own handling of them.

use argus_desktop_core::connection::page::Call;
use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState};
use gpui_kit::component::Sizable as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Context, Entity, FocusHandle, SharedString, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::theme::Tokens;

/// `.filter`: the field's width.
const FIELD_W: f32 = 220.0;

pub struct Filter {
    heard: Entity<Heard>,
    caller: Caller,
    field: Entity<InputState>,
    /// The stage's focus, which the keys go back to when the field lets go.
    stage: FocusHandle,
    /// What the field held when the session was last told, so the empty it is
    /// put back to when the history shuts says nothing.
    known: SharedString,
    open: bool,
    _subscriptions: [Subscription; 1],
}

impl Filter {
    pub fn new(heard: Entity<Heard>, caller: Caller, stage: FocusHandle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder("Filter commits"));
        let typing = cx.subscribe(&field, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = input.read(cx).value();
                if text != this.known && !this.heard.read(cx).held() {
                    // Only a text the session was told is `known`, so a held
                    // daemon's typing is sent by the next edit, not lost.
                    this.known = text.clone();
                    this.caller.call(Call::FilterHistory { filter: text.to_string() });
                }
            }
        });
        Self { heard, caller, field, stage, known: SharedString::default(), open: false, _subscriptions: [typing] }
    }

    /// Follows the session: a history that shuts lets the text go, and one that
    /// opens fills the field from its own filter, as the page fills a fresh
    /// field, so a history core parked in Code and restored shows what its rows
    /// are filtered by (HISTORY-7, SHARED-4). An open history is never set.
    pub fn follow(&mut self, shown: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let open = shown.is_some();
        let text = if self.open && !open {
            Some("")
        } else if !self.open {
            shown.filter(|filter| !filter.is_empty())
        } else {
            None
        };
        if let Some(text) = text {
            // Known first, so the field's change event is not an edit to send.
            self.known = SharedString::from(text.to_owned());
            self.field.update(cx, |field, cx| field.set_value(text, window, cx));
        }
        self.open = open;
    }

    /// `/`: the field takes the keys.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.field.update(cx, |field, cx| field.focus(window, cx));
    }

    /// What the field says now.
    pub fn text(&self, cx: &gpui_kit::App) -> SharedString {
        self.field.read(cx).value()
    }
}

impl Render for Filter {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let leave = |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
            cx.stop_propagation();
            window.focus(&this.stage, cx);
        };
        // `.field.filter`: a plain field, 220px wide and `--control-h` tall,
        // with no mark in it: the `/` that takes it is in the footer's hints.
        div()
            .id("history-filter")
            .test_support()
            .w(px(FIELD_W))
            .flex_shrink_0()
            .capture_action(cx.listener(move |this, _: &Enter, window, cx| leave(this, window, cx)))
            .capture_action(cx.listener(move |this, _: &Escape, window, cx| leave(this, window, cx)))
            .child(
                Input::new(&self.field)
                    .id("history-filter-field")
                    .small()
                    .h(px(tokens.control_h))
                    .aria_label("Filter commits by subject, hash or author"),
            )
    }
}
