//! An open decision's options as cards to choose between (BOARD-RECORD-7): each says its number and
//! its words, the one chosen has the accent for its edge, and choosing only changes which is lit: the
//! choice is the window's until Take sends it.
//!
//! With none listed the card says to answer in words.

use argus_desktop_core::connection::page::DecisionDetail;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Entity, Role, SharedString, TestSupportExt as _};

use super::titled;
use crate::board::feature_view::OpenFeature;
use crate::kit::caption::{label, meta, note};
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `.option`: a ring and the option's number over its words, on a strong edge that is the accent while
/// it is the one chosen.
fn card(decision: &DecisionDetail, index: usize, on: bool, feature: &Entity<OpenFeature>, tokens: &Tokens) -> AnyElement {
    let (choosing, id) = (feature.clone(), decision.id);
    let ring = div()
        .flex_shrink_0()
        .mt(rem(tokens.sp_2))
        .size(rem(tokens.check_box))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(if on { tokens.accent } else { tokens.line_strong })
        .when(on, |ring| ring.child(div().size(rem(tokens.dot_sm)).rounded_full().bg(tokens.accent)));
    h_flex()
        .id(("option", index))
        .test_support()
        .role(Role::RadioButton)
        .aria_label(SharedString::from(format!("Option {}: {}", index + 1, decision.options[index])))
        .aria_selected(on)
        .items_start()
        .gap(rem(tokens.sp_12))
        .p(rem(tokens.sp_12))
        .rounded(rem(tokens.r_md))
        .border_1()
        .border_color(if on { tokens.accent } else { tokens.line_strong })
        .cursor_pointer()
        .child(ring)
        .child(
            v_flex()
                .gap(rem(tokens.sp_2))
                .child(meta(tokens).child(SharedString::from(format!("Option {}", index + 1))))
                .child(div().weighted(tokens.weight_semibold).text_color(tokens.fg).child(SharedString::from(decision.options[index].clone()))),
        )
        .on_click(move |_, _, cx| choosing.update(cx, |feature, cx| feature.choose(id, index, cx)))
        .into_any_element()
}

/// The group of cards under a label, or what to do when there are none.
pub fn options(decision: &DecisionDetail, feature: &Entity<OpenFeature>, chosen: Option<usize>, tokens: &Tokens) -> AnyElement {
    if decision.options.is_empty() {
        return titled("Options", note("options-note", "No options listed: answer it in words.", tokens), tokens);
    }
    v_flex()
        .id("options")
        .test_support()
        .role(Role::RadioGroup)
        .aria_label("Options")
        .gap(rem(tokens.sp_8))
        .child(label("Options", tokens))
        .children((0..decision.options.len()).map(|index| card(decision, index, chosen == Some(index), feature, tokens)))
        .into_any_element()
}
