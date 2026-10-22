//! The small outlined words a row carries beside its text: a ticket's key (`.tkey`), a tag such as
//! an option or a step's binding (`.ktag`), and the dot that says a row changed since the last
//! reconcile (`.cdot`).

use gpui_kit::{div, prelude::*, Div, SharedString};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.tkey`: a mono key in the link colour on a hairline box.
pub fn tkey(words: impl Into<SharedString>, tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .font_family(SharedString::from(tokens.font_mono))
        .text_size(rem(tokens.fs_xs))
        .text_color(tokens.ref_)
        .border_1()
        .border_color(tokens.line_strong)
        .rounded(rem(tokens.r_xs))
        .px(rem(tokens.sp_4))
        .child(words.into())
}

/// `.ktag`: the small type, muted, on a hairline pill.
pub fn ktag(words: impl Into<SharedString>, tokens: &Tokens) -> Div {
    div()
        .flex_shrink_0()
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted)
        .border_1()
        .border_color(tokens.line_strong)
        .rounded(rem(tokens.r_pill))
        .px(rem(tokens.sp_8))
        .whitespace_nowrap()
        .child(words.into())
}

/// `.cdot`: a small dot in the changed colour, six pixels in from what it follows.
pub fn cdot(tokens: &Tokens) -> Div {
    div().flex_shrink_0().ml(rem(tokens.sp_6)).size(rem(tokens.dot_sm)).rounded_full().bg(tokens.changed)
}

/// `.cell`: a bordered card of the surface colour, 12 and 16 inside.
pub fn cell(tokens: &Tokens) -> Div {
    div().border_1().border_color(tokens.line).rounded(rem(tokens.r_md)).px(rem(tokens.sp_16)).py(rem(tokens.sp_12)).bg(tokens.bg_surface)
}
