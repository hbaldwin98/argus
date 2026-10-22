//! What a form is made of on screen: a named field over its words, the multi-line field, the
//! primary button with its cap beside Cancel, and the note under a field (BOARD-FORM-9, -12).
//!
//! Where a form sits is its lens's to say (the outline's add form, the detail's edit form, the header's
//! brief); these are the parts they share, so the buttons and the fields look the same everywhere.

use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Entity, SharedString};

use super::edit::{save_label, Editing};
use super::Form;
use crate::kit::button::{button, Look};
use crate::kit::caption::{label, meta};
use crate::kit::keycap::keycap;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The cap the primary button carries: the kit's secondary key is Cmd on macOS and Ctrl elsewhere.
pub fn chord() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd Enter"
    } else {
        "Ctrl Enter"
    }
}

/// `.field`: the one-line field, named `name` for a reader and a test.
pub fn line_field(form: &Entity<Form>, name: &'static str, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    div()
        .id(name)
        .w_full()
        .child(Input::new(form.read(cx).line().input()).id((name, 1usize)).h(rem(tokens.control_h)).aria_label(name))
        .into_any_element()
}

/// `.field.area`: the multi-line field, at least `--area-min-h` tall, or `tall` for a record and a
/// source, which are set in mono.
pub fn area_field(form: &Entity<Form>, name: &'static str, tall: bool, tokens: &Tokens, cx: &gpui_kit::App) -> AnyElement {
    let area = form.read(cx).area().clone();
    div()
        .id(name)
        .w_full()
        .h(rem(if tall { tokens.area_tall_h } else { tokens.area_min_h }))
        .when(tall, |field| field.font_family(SharedString::from(tokens.font_mono)).text_size(rem(tokens.fs_sm)))
        .child(Textarea::new(&area).h_full().aria_label(name))
        .into_any_element()
}

/// A label over what it names, 4px between (`.stack-4` with a `.label`).
pub fn titled(words: impl Into<SharedString>, body: impl IntoElement, tokens: &Tokens) -> AnyElement {
    v_flex().gap(rem(tokens.sp_4)).child(label(words, tokens)).child(body).into_any_element()
}

/// The note under a form's field, in the small muted type.
pub fn note(words: impl Into<SharedString>, tokens: &Tokens) -> impl IntoElement {
    meta(tokens).child(words.into())
}

/// `.actions`: the primary button, named for what it does with the cap beside it, and Cancel.
pub fn buttons(form: &Entity<Form>, editing: Editing, tokens: &Tokens) -> AnyElement {
    let (saving, cancelling) = (form.clone(), form.clone());
    let words = save_label(editing);
    let mono = SharedString::from(tokens.font_mono);
    h_flex()
        .gap(rem(tokens.sp_8))
        .flex_wrap()
        .child(
            button("form-save", words, Look::Primary, false, tokens)
                .child(words)
                // `.btn.primary .kbd`: the cap in the on-accent colour, its edge at 45%.
                .child(keycap(chord(), mono, tokens).text_color(tokens.fg_on_accent).border_color(tokens.fg_on_accent.opacity(tokens.share_edge)))
                .on_click(move |_, window, cx| saving.update(cx, |form, cx| form.save(window, cx))),
        )
        .child(button("form-cancel", "Cancel", Look::Plain, false, tokens).child("Cancel").on_click(move |_, window, cx| cancelling.update(cx, |form, cx| form.cancel(window, cx))))
        .into_any_element()
}
