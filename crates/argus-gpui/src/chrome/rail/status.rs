//! How a status looks on the rail's rows: the dot a status earns, and the one word a row says.

use gpui_kit::{div, prelude::*, Hsla};

use crate::theme::scale::rem;
use crate::theme::Tokens;

pub(super) fn dot_of(colour: Hsla, tokens: &Tokens) -> impl IntoElement {
    div().size(rem(tokens.dot_md)).flex_shrink_0().rounded_full().bg(colour)
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

/// The one status a pane row says in words, `needs you`: every other is its dot alone, so a column
/// of rows has one loud word at most per row (`project/Blend.dc.html`).
pub(super) fn pane_word(status: &str) -> Option<&'static str> {
    matches!(status, "waiting" | "needs-review").then_some("needs you")
}
