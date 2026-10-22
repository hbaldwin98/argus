//! How a status looks on the rail's rows: the dot a status earns, the pill a few statuses say in
//! words, and the words.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, px, Hsla};

use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::Tokens;

pub(super) fn dot_of(colour: Hsla, tokens: &Tokens) -> impl IntoElement {
    div().size(px(tokens.dot_md)).flex_shrink_0().rounded_full().bg(colour)
}

/// `.spill`: the one status pill, a dot and a word on a tint of the status's
/// colour. What blocks on a person is louder than what does not.
pub(super) fn spill(id: u64, word: &'static str, status: &str, tokens: &Tokens) -> impl IntoElement {
    let (tone, loud, hollow) = match status {
        "waiting" | "needs-review" => (tokens.attention, true, false),
        "failed" => (tokens.danger, true, false),
        "done" => (tokens.ok, false, false),
        "working" => (tokens.accent, false, false),
        _ => (tokens.fg_faint, false, true),
    };
    let (fill, edge) = if loud { (0.22, 0.60) } else { (0.12, 0.40) };
    h_flex()
        .id(format!("pane-{id}-word"))
        .test_support()
        .aria_label(word)
        .h(px(tokens.chip_h))
        .px(px(tokens.sp_8))
        .gap(px(tokens.sp_6))
        .items_center()
        .border_1()
        .border_color(tone.opacity(edge))
        .rounded(px(tokens.r_pill))
        .bg(tone.opacity(fill))
        // Faint on faint fails 4.5:1 at 11px, so an ended or idle word is the muted one.
        .text_color(if hollow { tokens.fg_muted } else { tone })
        .text_size(px(tokens.fs_xs))
        .weighted(tokens.weight_semibold)
        .child(if hollow {
            Glyph::Ring6.at(tokens.dot_sm).text_color(tone).into_any_element()
        } else {
            div().size(px(tokens.dot_sm)).flex_shrink_0().rounded_full().bg(tone).into_any_element()
        })
        .child(word)
}

/// The dot a status earns: the attention colour for what asks for a look,
/// the accent for work under way, none otherwise.
pub(super) fn status_colour(word: &str, tokens: &Tokens) -> Option<Hsla> {
    match word {
        "waiting" | "needs-review" | "failed" => Some(tokens.attention),
        "working" => Some(tokens.accent),
        _ => None,
    }
}

/// The few statuses a pane row says in words: only what asks for a look, or
/// that it has ended. Working shows in the dot alone.
pub(super) fn pane_word(status: &str) -> Option<&'static str> {
    match status {
        "waiting" => Some("needs you"),
        "needs-review" => Some("review"),
        "failed" => Some("failed"),
        "exited" => Some("exited"),
        "done" => Some("done"),
        _ => None,
    }
}
