//! What one key's cap looks like wherever a key is named: the page's `.kbd`.

use gpui_kit::{div, prelude::*, px, SharedString};

use crate::theme::Tokens;

/// `.kbd`: 10px mono on a hairline box, faint. `mono` is the theme's code face.
pub fn keycap(text: impl Into<SharedString>, mono: SharedString, tokens: &Tokens) -> gpui_kit::Div {
    div()
        .flex_shrink_0()
        .font_family(mono)
        .text_size(px(tokens.fs_2xs))
        // `line-height: 14px` and a 1px border make the 16px cap.
        .line_height(px(tokens.key_leading))
        .px(px(tokens.sp_4))
        .border_1()
        .border_color(tokens.line_strong)
        .rounded(px(tokens.r_xs))
        .text_color(tokens.fg_faint)
        .child(text.into())
}
