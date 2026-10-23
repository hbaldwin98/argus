//! The keys the index answers to, quietly under it, in the one key bar (`kit::hints`), and on the
//! board how a card is set aside. Which are listed is `index_words`'.

use argus_desktop_core::board::index_words::{key_bar, BOARD_HINT};
use gpui_kit::{div, prelude::*, TestSupportExt as _};

use crate::kit::hints::{self, Hint};
use crate::theme::Tokens;

pub fn bar(layout: &str, tokens: &Tokens) -> impl IntoElement {
    let keys = key_bar(layout);
    let said: Vec<Hint> = keys.iter().map(|(key, what)| (std::slice::from_ref(key), *what)).collect();
    let board = (layout == "board").then(|| div().id("feature-board-hint").test_support().aria_label(BOARD_HINT).child(BOARD_HINT).into_any_element());
    hints::bar("feature-keys", &said, &[], board, tokens)
}
