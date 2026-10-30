//! The path before the title: the repository, then the branch or the stage's trail, between faint
//! slashes and with one after it, so the title reads as the path's last part (`.head-stack .crumbs`).

use argus_desktop_core::chrome::strip::Part;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, SharedString};

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The crumb of `parts`; nothing for an empty one.
pub fn crumb(parts: &[Part], tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    let mut line = h_flex()
        .flex_shrink_0()
        .gap(rem(tokens.sp_4))
        .items_center()
        .text_size(rem(tokens.fs_md))
        .text_color(tokens.fg_muted)
        .whitespace_nowrap()
        .overflow_hidden();
    for (at, part) in parts.iter().enumerate() {
        if at > 0 {
            line = line.child(slash(tokens));
        }
        line = match part {
            Part::Mono(text) => line.child(div().font_family(mono.clone()).text_size(rem(tokens.fs_sm)).child(SharedString::from(text.clone()))),
            other => line.child(SharedString::from(other.words().to_owned())),
        };
    }
    line.child(slash(tokens))
}

/// The slash between two parts, a step quieter than the words.
fn slash(tokens: &Tokens) -> impl IntoElement {
    div().text_color(tokens.fg_fainter).child("/")
}
