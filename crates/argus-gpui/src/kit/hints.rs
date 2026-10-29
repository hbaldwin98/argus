//! The key bar at a stage's foot, the one every stage draws (`.kbar`): each hint as its caps and
//! what it does, and at the far end whatever the stage puts there. It stands as tall as the
//! sidebar's foot beside it (`foot_pad` over and under a row as tall as the foot's icon buttons, under a
//! hairline), so the two feet line up across the window.
//!
//! The words are the stage's own; what is bound and unhinted stays out of it, as in the page.

use gpui_kit::component::h_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The caps of one hint, then its words.
pub type Hint<'a> = (&'a [&'a str], &'a str);

/// The bar named `id`, whose label is every hint said in order, each hint `id-KEYS` by its caps.
/// `end`'s hints and then `note` sit at the far end.
pub fn bar(id: &'static str, hints: &[Hint], end: &[Hint], note: Option<AnyElement>, tokens: &Tokens) -> impl IntoElement {
    let said: Vec<String> = hints.iter().chain(end).map(|hint| words(*hint)).collect();
    h_flex()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(said.join(", ")))
        .flex_shrink_0()
        .py(rem(tokens.foot_pad))
        .px(rem(tokens.sp_20))
        .border_t_1()
        .border_color(tokens.line)
        .typed(tokens.type_small)
        .text_color(tokens.fg_faint)
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .min_h(rem(tokens.icon_btn))
                .gap(rem(tokens.sp_16))
                .items_center()
                .overflow_hidden()
                .children(hints.iter().map(|hint| one(id, *hint, tokens)))
                .when(!end.is_empty() || note.is_some(), |row| row.child(div().flex_1()))
                .children(end.iter().map(|hint| one(id, *hint, tokens)))
                .children(note),
        )
}

fn words((keys, words): Hint) -> String {
    format!("{} {words}", keys.join(" "))
}

fn one(id: &'static str, hint: Hint, tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    h_flex()
        .id(SharedString::from(format!("{id}-{}", hint.0.join("-"))))
        .test_support()
        .aria_label(SharedString::from(words(hint)))
        .flex_shrink_0()
        .gap(rem(tokens.sp_6))
        .items_center()
        .whitespace_nowrap()
        .children(hint.0.iter().map(|key| keycap(SharedString::from((*key).to_owned()), mono.clone(), tokens)))
        .child(SharedString::from(hint.1.to_owned()))
}
