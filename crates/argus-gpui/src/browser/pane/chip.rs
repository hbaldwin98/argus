//! Who has a browser, as a chip in a cell's strip (SHELL-BROWSER-9): a dot and core's word, the
//! colour of whoever has it.
//!
//! Core's ladder picks the word and the class (`browser::status`); this only paints them, so the
//! tile's head and a nav row can say it the same way.

use argus_desktop_core::browser::status::Status;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Hsla, Role, SharedString, TestSupportExt as _};

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The colour of `.bstatus.agent`, `.person` and `.held`: also the nav dot's.
pub fn colour(status: Status, tokens: &Tokens) -> Hsla {
    match status {
        Status::Agent => tokens.accent,
        Status::Person => tokens.ok,
        Status::Held => tokens.attention,
    }
}

/// `.bstatus`: a pill on a strong edge, the dot and the words in the status's colour.
pub fn chip(status: Status, tokens: &Tokens) -> impl IntoElement {
    let ink = colour(status, tokens);
    h_flex()
        .id(SharedString::from(format!("browser-status-{}", status.class())))
        .test_support()
        .role(Role::Group)
        .aria_label(status.word())
        .flex_none()
        .items_center()
        .gap(rem(tokens.sp_4))
        .px(rem(tokens.sp_8))
        .rounded(rem(tokens.r_pill))
        .border_1()
        .border_color(tokens.line_strong)
        .text_size(rem(tokens.fs_xs))
        .text_color(ink)
        .whitespace_nowrap()
        .child(div().size(rem(tokens.dot_md)).rounded_full().bg(ink))
        .child(status.word())
}
