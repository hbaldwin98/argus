//! A block this window has no drawing for: one a newer daemon sent, which says so, quietly, as the
//! page does.

use gpui_kit::{div, prelude::*, AnyElement};

use crate::theme::Tokens;

/// A kind a newer Argus sends.
pub fn unknown(tokens: &Tokens) -> AnyElement {
    div().text_color(tokens.fg_muted).child("This needs a newer Argus to show.").into_any_element()
}
