//! The small line over the title: the repository, then the branch or the stage's trail, between
//! faint slashes (`.head-stack .crumbs`).

use argus_desktop_core::chrome::strip::Part;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, px, SharedString};

use crate::theme::Tokens;

/// The crumb of `parts`; nothing for an empty one. Every stage that has a crumb has one the same
/// height, so every title sits on one baseline.
pub fn crumb(parts: &[Part], tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    let mut line = h_flex()
        .h(px(tokens.sp_16))
        .gap(px(tokens.sp_4))
        .items_center()
        .text_size(px(tokens.fs_xs))
        .text_color(tokens.fg_faint)
        .whitespace_nowrap()
        .overflow_hidden();
    for (at, part) in parts.iter().enumerate() {
        if at > 0 {
            line = line.child("/");
        }
        line = match part {
            Part::Text(text) => line.child(SharedString::from(text.clone())),
            Part::Mono(text) => line.child(div().font_family(mono.clone()).child(SharedString::from(text.clone()))),
        };
    }
    line
}
