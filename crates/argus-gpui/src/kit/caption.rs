//! The quiet type a page puts over a part of itself: the tracked capital `.label`, what it says at
//! its far end (`.label-aside`), and the small muted `.meta` line.
//!
//! An open feature's panes, the note and its reading, a step and a decision's record all caption
//! their parts the same way, so the look is here once. GPUI has no letter spacing, so the label is
//! the capitals alone.

use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Div, Hsla, SharedString};

use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.label`: `--type-label` in capitals on the faint colour, its children on one line 8px apart.
pub fn label(words: impl Into<SharedString>, tokens: &Tokens) -> Div {
    h_flex().items_center().gap(rem(tokens.sp_8)).typed(tokens.type_label).text_color(tokens.fg_faint).child(words.into().to_uppercase())
}

/// `.label-aside`: what a label says at its far end, in the sentence case of the body, `colour` its
/// tone (the label's faint when none is asked for).
pub fn aside(words: impl Into<SharedString>, colour: Option<Hsla>, tokens: &Tokens) -> Div {
    div().weighted(tokens.weight_regular).text_color(colour.unwrap_or(tokens.fg_faint)).child(words.into())
}

/// `.meta`: `--type-small` in the muted colour.
pub fn meta(tokens: &Tokens) -> Div {
    div().typed(tokens.type_small).text_color(tokens.fg_muted)
}

/// `.meta` that a reader and a test can find by what it says: a line of explanation, a note under a
/// field, what a list says when it has nothing.
pub fn note(id: impl Into<gpui_kit::ElementId>, words: impl Into<SharedString>, tokens: &Tokens) -> crate::kit::icon_button::Node {
    use gpui_kit::TestSupportExt as _;
    let words = words.into();
    meta(tokens).id(id).test_support().aria_label(words.clone()).child(words)
}
