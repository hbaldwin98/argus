//! How the kit's markdown view is dressed for a file read as a document.
//!
//! The kit derives colours, link and heading sizes from its theme, which the tokens fill
//! (`theme/kit`), so only what a document wants of its own is said here: the gap between paragraphs
//! and code, in the mono face, on the sunken ground the lines of a file are read on.

use gpui_kit::component::text::TextViewStyle;
use gpui_kit::{prelude::*, HighlightStyle, StyleRefinement};

use crate::theme::scale::rem;
use crate::theme::Tokens;

pub fn document(tokens: &Tokens) -> TextViewStyle {
    let code = StyleRefinement::default()
        .bg(tokens.bg_sunken)
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_sm))
        .px(rem(tokens.sp_12))
        .py(rem(tokens.sp_8))
        .font_family(tokens.font_mono)
        .text_size(rem(tokens.fs_sm));
    let mut style = TextViewStyle::default()
        .paragraph_gap(rem(tokens.sp_12))
        .code_block(code)
        .inline_code(HighlightStyle { background_color: Some(tokens.bg_sunken), ..Default::default() });
    style.is_dark = tokens.dark;
    style
}
