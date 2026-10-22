//! What the file area says when there are no lines to show: why the file could not be read, the note
//! on a file that is binary or too large, and the loading rows (CODE-35).

use gpui_kit::{div, prelude::*, AnyElement, SharedString, TestSupportExt as _};

use crate::kit::ghosts::ghosts;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The widths of the five loading rows of a file being read.
const BARS: [f32; 5] = [0.5, 0.9, 0.8, 0.7, 0.6];

/// `.review-empty`: 24px about a line in the body type, muted, on the file's ground.
pub(super) fn said(id: &'static str, words: String, tokens: &Tokens) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(words.clone()))
        .size_full()
        .p(rem(tokens.sp_24))
        .bg(tokens.bg_sunken)
        .typed(tokens.type_body)
        .text_color(tokens.fg_muted)
        .child(SharedString::from(words))
        .into_any_element()
}

pub(super) fn loading(tokens: &Tokens) -> AnyElement {
    div().id("code-loading-file").test_support().aria_label("Loading").size_full().bg(tokens.bg_sunken).child(ghosts("code-file-ghosts", &BARS, tokens)).into_any_element()
}
