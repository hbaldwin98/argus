//! The small line over the title: the repository, then the branch or the stage's trail, between
//! faint slashes (`.head-stack .crumbs`).

use argus_desktop_core::chrome::strip::Part;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, SharedString};

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The crumb of `parts`; nothing for an empty one. Every stage that has a crumb has one the same
/// height, so every title sits on one baseline.
pub fn crumb(parts: &[Part], tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    let mut line = h_flex()
        .h(rem(tokens.sp_16))
        .gap(rem(tokens.sp_4))
        .items_center()
        .text_size(rem(tokens.fs_xs))
        .text_color(tokens.fg_faint)
        .whitespace_nowrap()
        .overflow_hidden();
    for (at, part) in parts.iter().enumerate() {
        if at > 0 {
            line = line.child("/");
        }
        line = match part {
            Part::Mono(text) => line.child(div().font_family(mono.clone()).child(SharedString::from(text.clone()))),
            other => line.child(SharedString::from(other.words().to_owned())),
        };
    }
    line
}
