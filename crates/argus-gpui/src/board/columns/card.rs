//! One card of the board (`.kcard`): number and branch with a corner tag, the title, the work line,
//! the reason it is held, and a footer saying how it came to stand there.
//!
//! The words are `index_words`'; the look is the page's. The card is the same drawn in its column,
//! as the ghost that follows the pointer, and (in the ghost) lifted with the popup shadow. What a
//! press does is the board's to wire, since a press starts a drag (`drag`).

use argus_desktop_core::board::index_words::{self, Corner, Work};
use argus_desktop_core::connection::page::FeatureRow;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use crate::board::cells::{after, meta, progress, questions_cell};
use crate::kit::icon_button::Node;
use crate::kit::pill::{status_pill, status_word};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// How a card is dressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dress {
    Plain,
    /// The one under the cursor.
    Peeked,
    /// The copy that follows the pointer: raised, with a strong edge.
    Lifted,
}

fn corner(row: &FeatureRow, tokens: &Tokens) -> Option<gpui_kit::AnyElement> {
    let tag = |words: gpui_kit::Div| words.typed(tokens.type_small).text_color(tokens.fg_muted).whitespace_nowrap().px(rem(tokens.sp_8)).border_1().border_color(tokens.line_strong).rounded(rem(tokens.r_pill));
    Some(match index_words::corner(row)? {
        Corner::Agent(status) => status_pill(SharedString::from(format!("card-agent-{}", row.slug)), status_word(status).to_owned(), status, tokens).into_any_element(),
        Corner::After(_) => tag(h_flex().child("\u{2192} after ").child(after(row, tokens))).into_any_element(),
        Corner::New => tag(div().child("new")).into_any_element(),
    })
}

fn work(row: &FeatureRow, tokens: &Tokens) -> gpui_kit::AnyElement {
    match index_words::work(row) {
        Work::Words(words) => meta(words, tokens).into_any_element(),
        Work::Progress { done, total, .. } => h_flex()
            .items_center()
            .gap(rem(tokens.sp_8))
            .child(progress(&row.slug, done, total, tokens))
            .child(div().flex_1())
            .children(questions_cell(row, tokens))
            .into_any_element(),
    }
}

/// The two bars that are a pause, in the text colour.
fn pause(tokens: &Tokens) -> impl IntoElement {
    let bar = || div().w(rem(2.5)).h_full().bg(tokens.fg_muted);
    h_flex().flex_shrink_0().w(rem(8.0)).h(rem(10.0)).justify_between().child(bar()).child(bar())
}

/// The footer on one line: the account is cut short and the time keeps its own span.
fn footer(row: &FeatureRow, tokens: &Tokens) -> impl IntoElement {
    let (what, when) = index_words::footer(&row.footer);
    h_flex()
        .id(SharedString::from(format!("card-footer-{}", row.slug)))
        .test_support()
        .aria_label(SharedString::from(row.footer.clone()))
        .gap(rem(tokens.sp_4))
        .pt(rem(tokens.sp_8))
        .border_t_1()
        .border_color(tokens.line)
        .text_size(rem(tokens.fs_xs))
        .text_color(tokens.fg_muted)
        .child(div().min_w_0().truncate().child(SharedString::from(what.to_owned())))
        .children(when.map(|when| div().flex_shrink_0().whitespace_nowrap().child(SharedString::from(format!("\u{b7} {when}")))))
}

pub fn card(row: &FeatureRow, dress: Dress, tokens: &Tokens) -> Node {
    let mono = SharedString::from(tokens.font_mono);
    let raised = tokens.bg_raised;
    let over = raised.blend(tokens.fg.opacity(0.06));
    let strong = tokens.line_strong;
    v_flex()
        .id(SharedString::from(format!("feature-card-{}", row.slug)))
        .test_support()
        .relative()
        .aria_selected(dress == Dress::Peeked)
        .aria_label(SharedString::from(format!("#{} {}", row.number, row.title)))
        .w_full()
        .gap(rem(tokens.sp_8))
        .p(rem(tokens.sp_12))
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_md))
        .bg(raised)
        .cursor_grab()
        .map(|card| match dress {
            Dress::Plain => card.hover(move |style| style.border_color(strong).bg(over)),
            Dress::Peeked => card.bg(tokens.bg_selected).border_color(tokens.mark_sel_edge),
            Dress::Lifted => card.shadow(tokens.shadow_pop()).border_color(strong),
        })
        .child(
            h_flex()
                .items_center()
                .gap(rem(tokens.sp_8))
                .min_w_0()
                .child(div().flex_shrink_0().font_family(mono.clone()).text_size(rem(tokens.fs_sm)).text_color(tokens.fg_faint).child(SharedString::from(format!("#{}", row.number))))
                .children(row.branch.clone().map(|branch| {
                    div().min_w_0().truncate().font_family(mono.clone()).text_size(rem(tokens.fs_sm)).text_color(tokens.ref_).child(SharedString::from(branch))
                }))
                .child(div().flex_1())
                .children(corner(row, tokens)),
        )
        .child(div().weighted(tokens.weight_semibold).line_height(rem(tokens.fs_md * 1.35)).child(SharedString::from(row.title.clone())))
        .child(work(row, tokens))
        .children(row.held.clone().map(|reason| {
            h_flex()
                .items_center()
                .gap(rem(tokens.sp_8))
                .px(rem(tokens.sp_8))
                .py(rem(tokens.sp_6))
                .rounded(rem(tokens.r_sm))
                .bg(tokens.fill)
                .typed(tokens.type_small)
                .text_color(tokens.fg_muted)
                .child(pause(tokens))
                .child(div().flex_1().min_w_0().child(SharedString::from(reason)))
        }))
        .child(footer(row, tokens))
}

/// The placeholder left where a card was picked up, saying where it is going.
pub fn placeholder(row: &FeatureRow, words: String, tokens: &Tokens) -> Node {
    v_flex()
        .id(SharedString::from(format!("feature-placeholder-{}", row.slug)))
        .test_support()
        .aria_label(SharedString::from(words.clone()))
        .w_full()
        .min_h(rem(tokens.fx_card_min_h))
        .items_center()
        .justify_center()
        .p(rem(tokens.sp_12))
        .border_1()
        .border_dashed()
        .border_color(tokens.line_strong)
        .rounded(rem(tokens.r_md))
        .child(meta(words, tokens))
}

