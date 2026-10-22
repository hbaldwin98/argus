//! A new feature written as one note (BOARD-CAPTURE-1 to -13): the note on the left with its line
//! numbers, and on the right what the daemon's own grammar reads it as.
//!
//! The reading is Rust's and comes back whole on every change (`CapturePage`); this never parses the
//! note. The text field is held for as long as the note is open, so the caret and the text survive every
//! reading (BOARD-CAPTURE-13); only the head, the numbers and the reading are redrawn. Create sends the
//! note as the field holds it when pressed, not the last one echoed, and the note stays up until the
//! pushed board holds the feature, which then opens; a refusal leaves the text to be fixed and sent again.
//!
//! The field answers the keymap's Writing place before it types: Ctrl or Cmd Enter creates and Escape
//! discards. The Capture layer holds the window's own keys while the field does not have the keys.

pub mod gutter;
pub mod head;
pub mod keys;
pub mod legend;
pub mod reads;

use argus_desktop_core::board::capture_note::CaptureGesture;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::{Action, Layer, Place};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, px, Context, Entity, FocusHandle, Focusable as _, SharedString, Subscription, TestSupportExt as _, Window};
use std::rc::Rc;

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::keys::field::{Fields, Registered};
use crate::keys::layer::Layers;
use crate::kit::field;
use crate::kit::caption::label;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The placeholder of an empty note: how it starts, as the page says it.
const PLACEHOLDER: &str = "The title on the first line\n\nThe brief under it.\n\n- a task\n  - a subtask\n? an open question\n  - an option";

pub struct CaptureNote {
    heard: Entity<Heard>,
    caller: Caller,
    note: Entity<TextareaState>,
    /// Where the keys rest while the field does not have them.
    focus: FocusHandle,
    /// Whether the note was open at the last update, to take the keys once as it opens.
    was_open: bool,
    _typing: Subscription,
    _following: [Subscription; 2],
    _keys: Registered,
}

impl CaptureNote {
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let note = cx.new(|cx| TextareaState::new(window, cx).rows(3).soft_wrap(true).placeholder(PLACEHOLDER));
        let typing = cx.subscribe(&note, |this, note, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) && !this.heard.read(cx).held() {
                let text = note.read(cx).value().to_string();
                this.caller.call(Call::Capture { gesture: CaptureGesture::Note { note: text } });
            }
        });
        // The numbers follow the field: its text, its wrapping and its scroll are the field's.
        let watching = cx.observe(&note, |_, _, cx| cx.notify());
        let following = cx.observe_in(&heard, window, |this, heard, window, cx| this.follow(&heard, window, cx));
        let asked = cx.weak_entity();
        let keys = Fields::of(cx).register_place(window, note.focus_handle(cx), Place::Writing, move |action, window, cx| {
            asked
                .update(cx, |note, cx| match action {
                    Action::Save => {
                        note.create(window, cx);
                        true
                    }
                    Action::CloseLayer => {
                        note.discard(window, cx);
                        true
                    }
                    _ => false,
                })
                .unwrap_or(false)
        });
        Layers::of(cx).register(Layer::Capture, Rc::new(keys::NoteKeys { note: cx.weak_entity() }));
        Self { heard, caller, note, focus: cx.focus_handle(), was_open: false, _typing: typing, _following: [watching, following], _keys: keys }
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Whether the session holds a note open.
    pub fn is_open(&self, cx: &gpui_kit::App) -> bool {
        self.heard.read(cx).capture.is_some()
    }

    /// Follows the session: the note says what the session last heard of it unless it is being typed
    /// in, and it takes the keys once as it opens.
    fn follow(&mut self, heard: &Entity<Heard>, window: &mut Window, cx: &mut Context<Self>) {
        let page = heard.read(cx).capture.clone();
        let open = page.is_some();
        if let Some(page) = page {
            let typing = self.note.focus_handle(cx).contains_focused(window, cx);
            if !typing && self.note.read(cx).value() != page.note {
                field::fill_area(&self.note, &page.note, window, cx);
            }
        }
        if open && !self.was_open {
            self.note.update(cx, |note, cx| note.focus(window, cx));
        } else if !open && self.was_open {
            self.note.update(cx, |note, cx| note.set_value("", window, cx));
        }
        self.was_open = open;
        cx.notify();
    }

    /// Sends the note as the field holds it now: calls need not arrive in the order sent, so the last
    /// echoed note may be an edit behind.
    pub fn create(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.heard.read(cx).held() {
            return;
        }
        let text = self.note.read(cx).value().to_string();
        self.caller.call(Call::Capture { gesture: CaptureGesture::Create { note: text } });
    }

    /// Puts the note away unsent; the index comes back.
    pub fn discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.heard.read(cx).held() {
            return;
        }
        self.caller.call(Call::Capture { gesture: CaptureGesture::Discard });
        window.focus(&self.focus, cx);
    }
}

impl Render for CaptureNote {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let root = div().id("capture").test_support().key_context("Capture").track_focus(&self.focus).size_full();
        let Some(page) = self.heard.read(cx).capture.clone() else { return root };
        let me = cx.entity();
        let text = self.note.read(cx).value();
        let rows = gutter::rows(&self.note, gutter::count(&text), cx);
        let leading = self.note.read(cx).line_height().unwrap_or_else(|| tokens.type_mono.line_height().to_pixels(window.rem_size()));
        let scrolled = self.note.read(cx).scroll_offset().y;
        let typing = self.note.focus_handle(cx).contains_focused(window, cx);
        let mono = SharedString::from(tokens.font_mono);
        let note_box = h_flex()
            .id("capture-box")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(tokens.bg_sunken)
            .border_1()
            .border_color(if typing { tokens.accent } else { tokens.line })
            .rounded(rem(tokens.r_md))
            .child(gutter::gutter(&rows, &page.unread, leading, scrolled, &tokens))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .ml(rem(-tokens.note_inset))
                    .font_family(mono)
                    .typed(tokens.type_mono)
                    .child(Textarea::new(&self.note).appearance(false).bordered(false).h_full().font_family(SharedString::from(tokens.font_mono)).typed(tokens.type_mono).aria_label("Feature note")),
            );
        root.child(
            v_flex()
                .size_full()
                .child(head::head(&page, &me, &tokens))
                .child(
                    h_flex()
                        .items_stretch()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .gap(rem(tokens.sp_20))
                        .px(rem(tokens.sp_20))
                        .py(rem(tokens.sp_16))
                        .child(
                            v_flex()
                                .id("capture-note")
                                .test_support()
                                .aria_label("The note")
                                .flex_1()
                                .min_w_0()
                                .min_h_0()
                                .gap(rem(tokens.sp_8))
                                .child(label("Note", &tokens))
                                .child(note_box)
                                .child(legend::legend(&tokens)),
                        )
                        .child(reads::reads(&page, &tokens)),
                ),
        )
        .text_color(tokens.fg)
        .text_size(rem(tokens.fs_md))
        .min_h(px(0.0))
    }
}

#[cfg(test)]
mod tests;
