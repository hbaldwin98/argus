//! The keys the index answers to, quietly under it (`.kbar`): a key cap and what it does, and on
//! the board how a card is set aside. Which are listed is `index_words`'.

use argus_desktop_core::board::index_words::{key_bar, BOARD_HINT};
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub fn bar(layout: &str, tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    h_flex()
        .id("feature-keys")
        .test_support()
        .flex_shrink_0()
        .items_center()
        .gap(rem(tokens.sp_16))
        .min_h(rem(tokens.ck_bar_h))
        .px(rem(tokens.sp_20))
        .border_t_1()
        .border_color(tokens.line)
        .typed(tokens.type_small)
        .text_color(tokens.fg_faint)
        .children(key_bar(layout).into_iter().map(|(key, what)| {
            h_flex()
                .id(SharedString::from(format!("feature-key-{key}")))
                .test_support()
                .aria_label(SharedString::from(format!("{key} {what}")))
                .items_center()
                .gap(rem(tokens.sp_6))
                .whitespace_nowrap()
                .child(keycap(key, mono.clone(), tokens))
                .child(what)
        }))
        .child(div().flex_1())
        .when(layout == "board", |bar| bar.child(div().id("feature-board-hint").test_support().aria_label(BOARD_HINT).child(BOARD_HINT)))
}
