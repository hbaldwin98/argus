//! The keys the table answers to, quietly under it (`.kbar`): a key cap and what it does, `Enter
//! open`, `R review` and the rest. Which are listed is `argus-desktop-core`'s (`words::key_bar`).

use argus_desktop_core::checkouts::words::key_bar;
use gpui_kit::component::h_flex;
use gpui_kit::{prelude::*, SharedString, TestSupportExt as _};

use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub fn bar(files_offered: bool, tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    h_flex()
        .id("checkouts-keys")
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
        .children(key_bar(files_offered).into_iter().map(|(key, what)| {
            h_flex()
                .id(SharedString::from(format!("keybar-{key}")))
                .test_support()
                .aria_label(SharedString::from(format!("{key} {what}")))
                .items_center()
                .gap(rem(tokens.sp_6))
                .whitespace_nowrap()
                .child(keycap(key, mono.clone(), tokens))
                .child(what)
        }))
}
