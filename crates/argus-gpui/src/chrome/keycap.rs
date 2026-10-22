//! What one key's cap looks like wherever a key is named: the page's `.kbd`.

use gpui_kit::{div, prelude::*, px, SharedString};

use crate::theme::Ledger;
use crate::tokens::{fs, radius, size, sp};

/// `.kbd`: 10px mono on a hairline box, faint. `mono` is the theme's code face.
pub fn keycap(text: impl Into<SharedString>, mono: SharedString, ledger: &Ledger) -> gpui_kit::Div {
    div()
        .flex_shrink_0()
        .font_family(mono)
        .text_size(px(fs::XS2))
        // `line-height: 14px` and a 1px border make the 16px cap.
        .line_height(px(size::KEY_LEADING))
        .px(px(sp::S4))
        .border_1()
        .border_color(ledger.line_strong)
        .rounded(px(radius::XS))
        .text_color(ledger.faint)
        .child(text.into())
}
