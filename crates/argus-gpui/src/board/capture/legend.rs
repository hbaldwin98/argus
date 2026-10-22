//! The grammar as the foot of the note says it (BOARD-CAPTURE-5): what a first line, a paragraph and the
//! three marks mean, in two columns that line up.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What each way of starting a line means; the mark is a code mark when it is one character of the grammar.
pub const LEGEND: [(&str, bool, &str); 5] = [
    ("first line", false, "the title"),
    ("a paragraph", false, "the brief"),
    ("-", true, "a task; indent it for a subtask"),
    ("?", true, "an open decision; indented lines are its options"),
    ("!", true, "a decision already made; the indented line is why"),
];

/// `.legend`: a raised card of small muted type, marks in one column and meanings in the next.
pub fn legend(tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    h_flex()
        .id("capture-legend")
        .test_support()
        .aria_label("What the first characters of a line mean")
        .flex_shrink_0()
        .items_start()
        .gap(rem(tokens.sp_16))
        .px(rem(tokens.sp_16))
        .py(rem(tokens.sp_12))
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_md))
        .bg(tokens.bg_raised)
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted)
        .child(v_flex().gap(rem(tokens.sp_4)).children(LEGEND.iter().map(|(mark, code, _)| {
            div().when(*code, |mark| mark.font_family(mono.clone()).text_color(tokens.ref_)).child(*mark)
        })))
        .child(v_flex().gap(rem(tokens.sp_4)).children(LEGEND.iter().map(|(_, _, means)| div().child(*means))))
}
