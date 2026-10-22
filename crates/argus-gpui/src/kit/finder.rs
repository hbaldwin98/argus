//! The field over a list of files, with a lens at its left and, at its right, the key that takes
//! it faint or whatever else the surface puts there: the Files panel's filter (`/`) and the Code
//! stage's find (`Ctrl P`, or the search's options), the page's `.cf-search` with `.cf-filter` in it.
//!
//! What the field means by its text, and what its keys do, are the surface's; this is how it looks.

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{div, prelude::*, AnyElement, ElementId, Entity, SharedString};

use crate::kit::icons::Glyph;
use crate::kit::keycap::keycap;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The key cap that stands at the field's right while it is not busy with something else.
pub fn cap(text: &'static str, tokens: &Tokens) -> AnyElement {
    keycap(text, SharedString::from(tokens.font_mono), tokens).into_any_element()
}

/// The input as the page's `.field` draws it: its type is the body's (`font: inherit`) and its height the control's.
pub fn field_input(input: &Entity<InputState>, id: impl Into<ElementId>, label: &'static str, tokens: &Tokens) -> Input {
    Input::new(input).id(id).h(rem(tokens.control_h)).text_size(rem(tokens.type_body.size)).aria_label(label)
}

/// The field named `id` and `label` to a reader, with `suffix` at its right while it is given.
pub fn finder(id: impl Into<ElementId>, input: &Entity<InputState>, label: &'static str, suffix: Option<AnyElement>, tokens: &Tokens) -> impl IntoElement {
    let field = field_input(input, id, label, tokens).prefix(div().text_color(tokens.fg_faint).child(Glyph::Search.at(tokens.icon_sm)));
    let field = match suffix {
        Some(suffix) => field.suffix(suffix),
        None => field,
    };
    div().flex_1().min_w_0().child(field)
}
