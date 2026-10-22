//! What fills the Checkouts stage before there is a table to draw: five loading bars before the
//! first snapshot, a card saying no repository is chosen, and a line for a repository with nothing
//! to list. Which of them it is, and the words, are `argus-desktop-core`'s (`words::body`).

use argus_desktop_core::checkouts::words::LOADING;
use gpui_kit::component::v_flex;
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use crate::kit::ghosts::ghosts;
use crate::kit::icon_button::Node;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// Five loading bars of the page's widths.
pub fn loading(tokens: &Tokens) -> impl IntoElement {
    div().id("checkouts-loading").test_support().aria_label("Loading").size_full().child(ghosts("checkouts-ghosts", &LOADING, tokens))
}

/// `.empty > .empty-body`: what is said, centred in the room there is.
fn body(id: &'static str, tokens: &Tokens) -> Node {
    v_flex().id(id).test_support().size_full().items_center().justify_center().gap(rem(tokens.sp_16)).p(rem(tokens.sp_24)).text_center()
}

/// No repository is chosen.
pub fn none_chosen(tokens: &Tokens) -> impl IntoElement {
    body("checkouts-none", tokens)
        .child(div().text_color(tokens.fg_faint).child(Glyph::Folder.at(tokens.icon_lg)))
        .child(div().typed(tokens.type_title).text_color(tokens.fg).id("none-title").test_support().aria_label("No repository").child("No repository"))
        .child(div().text_color(tokens.fg_muted).id("none-hint").test_support().aria_label("Choose one on the left.").child("Choose one on the left."))
}

/// A repository with nothing to list, and why (`words::Body::Nothing`).
pub fn nothing(words: &str, tokens: &Tokens) -> impl IntoElement {
    let words = SharedString::from(words.to_owned());
    body("checkouts-nothing", tokens).child(div().id("checkouts-nothing-words").test_support().aria_label(words.clone()).text_color(tokens.fg_muted).child(words))
}
