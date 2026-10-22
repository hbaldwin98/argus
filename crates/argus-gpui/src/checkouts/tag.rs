//! The small `primary` pill set beside a checkout's name, in the table and in the side column
//! (`.ctag`).

use gpui_kit::{div, prelude::*, SharedString};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.ctag`: the small type, muted, on a fill, a pill with 8px either side of its words.
pub fn tag(words: impl Into<SharedString>, tokens: &Tokens) -> gpui_kit::Div {
    div()
        .flex_shrink_0()
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted)
        .bg(tokens.fill)
        .rounded(rem(tokens.r_pill))
        .px(rem(tokens.sp_8))
        .child(words.into())
}
