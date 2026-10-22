//! A code block with its language and a Copy button in a header. Plain text: no highlighter here,
//! as the page has none.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::pen::Pen;
use crate::kit::wheel::Wheel;
use crate::theme::scale::rem;

/// `.vcode`: a header `--row-h` tall holding the language and Copy, over the text in monospace on the
/// sunken colour, which scrolls past `--code-max-h` and sideways past the width, never wrapping.
pub fn code(lang: Option<&str>, text: &str, key: &str, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let copy = pen.copy(&format!("{key}|copy"), text);
    let scroll = pen.scroll(&format!("{key}-code-body"));
    let wheel = Wheel::new(&scroll);
    v_flex()
        .id(SharedString::from(format!("{key}-code")))
        .test_support()
        .min_w_0()
        .overflow_hidden()
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_sm))
        .child(
            h_flex()
                .h(rem(tokens.row_h))
                .px(rem(tokens.sp_12))
                .bg(tokens.bg_chrome)
                .border_b_1()
                .border_color(tokens.line)
                .font_family(tokens.font_mono)
                .text_size(rem(tokens.fs_xs))
                .text_color(tokens.fg_faint)
                .child(SharedString::from(lang.unwrap_or_default().to_owned()))
                .child(div().ml_auto().child(copy)),
        )
        .child(
            div()
                .id(SharedString::from(format!("{key}-code-body")))
                .test_support()
                .max_h(rem(tokens.code_max_h))
                .overflow_scroll()
                .track_scroll(&scroll)
                .on_scroll_wheel(wheel.keep())
                .child(wheel.probe())
                .bg(tokens.bg_sunken)
                .font_family(tokens.font_mono)
                .text_size(rem(tokens.fs_sm))
                .line_height(rem(tokens.code_line_h))
                .text_color(tokens.fg_muted)
                .child(h_flex().items_start().px(rem(tokens.sp_12)).py(rem(tokens.sp_8)).child(div().flex_shrink_0().whitespace_nowrap().child(SharedString::from(text.to_owned())))),
        )
        .into_any_element()
}
