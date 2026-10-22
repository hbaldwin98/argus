//! The State cell of a row: only what needs a look, side by side (`.col-state`). A checkout says it
//! is detached, how many files changed, how far from its upstream, how many panes run on it (a
//! status dot each) and which feature it is on; a branch says its git string, its note and its
//! feature. The words and their order are `argus-desktop-core`'s (`checkouts/says`).

use argus_desktop_core::checkouts::says::{branch_cell, panes, state_of, state_words, Say};
use argus_desktop_core::checkouts::table::CheckoutsRow;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, AnyElement, SharedString, TestSupportExt as _};

use crate::kit::pill::status_word;
use crate::kit::status_dot::status_dot;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// `#42`, in the accent while the Feature stage has it under its cursor and muted otherwise.
fn feature(number: i64, viewed: bool, tokens: &Tokens) -> AnyElement {
    div()
        .font_family(SharedString::from(tokens.font_mono))
        .text_color(if viewed { tokens.accent } else { tokens.fg_muted })
        .child(SharedString::from(format!("#{number}")))
        .into_any_element()
}

fn say(say: &Say, row: &CheckoutsRow, tokens: &Tokens) -> AnyElement {
    let mono = SharedString::from(tokens.font_mono);
    match say {
        Say::Detached => div().font_family(mono).text_color(tokens.attention).child("detached").into_any_element(),
        Say::Changed(text) => div().text_color(tokens.attention).child(SharedString::from(text.clone())).into_any_element(),
        Say::Distance(text) => div().font_family(mono).text_color(tokens.fg_muted).child(SharedString::from(text.clone())).into_any_element(),
        Say::Panes(count) => h_flex()
            .gap(rem(tokens.sp_6))
            .items_center()
            .text_color(tokens.fg_muted)
            .children(row.running.iter().map(|pane| {
                let words = format!("{}: {}", pane.title, status_word(pane.status));
                div()
                    .id(("running-dot", pane.id))
                    .test_support()
                    .aria_label(SharedString::from(words.clone()))
                    .tooltip(tip(words))
                    .child(status_dot(format!("running-{}", pane.id), pane.status, tokens))
            }))
            .child(SharedString::from(panes(*count)))
            .into_any_element(),
        Say::Feature { number, viewed } => feature(*number, *viewed, tokens),
    }
}

/// The cell's contents for `row`.
pub fn cell(row: &CheckoutsRow, tokens: &Tokens) -> impl IntoElement {
    let said: Vec<AnyElement> = if row.name.is_some() {
        state_of(row).iter().map(|s| say(s, row, tokens)).collect()
    } else {
        let branch = branch_cell(row);
        let mono = SharedString::from(tokens.font_mono);
        let mut cells = Vec::new();
        if let Some(git) = branch.git {
            cells.push(div().font_family(mono).text_color(tokens.fg_muted).child(SharedString::from(git)).into_any_element());
        }
        if let Some(note) = branch.note {
            cells.push(div().typed(tokens.type_small).text_color(tokens.fg_muted).child(note).into_any_element());
        }
        if let Some((number, viewed)) = branch.feature {
            cells.push(feature(number, viewed, tokens));
        }
        cells
    };
    h_flex().id(SharedString::from(format!("state-{}", row.key))).test_support().aria_label(SharedString::from(state_words(row))).gap(rem(tokens.sp_8)).items_center().text_size(rem(tokens.fs_sm)).whitespace_nowrap().children(said)
}
