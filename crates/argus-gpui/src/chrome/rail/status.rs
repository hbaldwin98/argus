//! How a status looks on the rail's rows: the dot a status earns, and the words a few statuses
//! say (the pill they are said in is `kit/pill`).

use gpui_kit::{div, prelude::*, px, Hsla};

use crate::theme::Tokens;

pub(super) fn dot_of(colour: Hsla, tokens: &Tokens) -> impl IntoElement {
    div().size(px(tokens.dot_md)).flex_shrink_0().rounded_full().bg(colour)
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
