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

/// The keys an item or tooltip names, each press a cap, the alternatives apart by a slash:
/// one list of presses per alternative, as `keys::caps::presses` gives them.
pub fn keycaps(keys: &[Vec<String>], tokens: &Tokens) -> gpui_kit::Div {
    let mono = SharedString::from(tokens.font_mono);
    let mut caps = gpui_kit::div().flex().items_center().gap(px(tokens.sp_4)).text_size(px(tokens.fs_xs)).text_color(tokens.fg_muted);
    for (n, presses) in keys.iter().enumerate() {
        if n > 0 {
            caps = caps.child("/");
        }
        for press in presses {
            caps = caps.child(keycap(press.clone(), mono.clone(), tokens));
        }
    }
    caps
}
