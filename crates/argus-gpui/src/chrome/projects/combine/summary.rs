//! What ticking would do, line by line: a tick for what is gained and kept, a dash for what is
//! dropped. The words are core's (`combine_outcome::summary`); nothing is said until a project is
//! ticked.

use argus_desktop_core::chrome::combine_outcome::{summary, Line, Mark};
use argus_desktop_core::chrome::combine_page::CombinePage;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, IntoElement, SharedString, TestSupportExt as _};

use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub(super) fn lines(page: &CombinePage, tokens: &Tokens) -> impl IntoElement {
    v_flex().id("cp-summary").gap(rem(tokens.sp_6)).children(summary(page).into_iter().enumerate().map(|(at, line)| one(at, line, tokens)).collect::<Vec<_>>())
}

fn one(at: usize, line: Line, tokens: &Tokens) -> impl IntoElement {
    let (glyph, colour, words_colour) = match line.mark {
        Mark::Check => (Glyph::Tick, tokens.ok, tokens.fg),
        Mark::Dash => (Glyph::Minus, tokens.fg_faint, tokens.fg_muted),
    };
    let words = SharedString::from(line.words);
    h_flex()
        .id(("cp-line", at))
        .test_support()
        .aria_label(words.clone())
        .items_start()
        .gap(rem(tokens.sp_8))
        .typed(tokens.type_small)
        .child(div().flex_shrink_0().text_color(colour).child(glyph.at(tokens.icon_sm)))
        .child(div().flex_1().min_w_0().text_color(words_colour).child(words))
}
