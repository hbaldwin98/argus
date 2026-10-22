//! The right side of the shortcuts sheet: a section's rows, each an action and its caps
//! (SHELL-SHEET-4, 5).

use argus_desktop_core::keys::sheet::{Row, Section};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, ElementId, SharedString, TestSupportExt as _};

use crate::kit::keycap::{keycap, keycaps};
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::{rem, Type};
use crate::theme::Tokens;
use argus_desktop_core::keys::chords::sentence;

/// One row: the action in sentence case, and its caps. With more than two alternatives only the first
/// shows and the whole list is the tooltip.
fn row(id: impl Into<ElementId>, row: &Row, tokens: &Tokens) -> impl IntoElement {
    let many = row.keys.len() > 2;
    let shown = if many { &row.keys[..1] } else { &row.keys[..] };
    let all: Vec<String> = row.keys.iter().map(|alternative| alternative.join(" ")).collect();
    let caps = div().id("keys").flex_shrink_0().whitespace_nowrap().child(keycaps(shown, tokens));
    let label = sentence(&row.label);
    h_flex()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(format!("{label} {}", all.join(" / "))))
        .items_start()
        .gap(rem(tokens.sp_12))
        .py(rem(tokens.sp_6))
        .border_b_1()
        .border_color(tokens.line)
        .typed(tokens.type_small)
        .child(div().flex_1().min_w_0().child(SharedString::from(label)))
        .child(if many { caps.tooltip(tip(all.join(" / "))).into_any_element() } else { caps.into_any_element() })
}

/// Rows in two columns, filled across, as the page's grid is; or in one.
fn rows(name: &'static str, section: usize, list: &[Row], two: bool, tokens: &Tokens) -> impl IntoElement {
    let across = if two { 2 } else { 1 };
    v_flex().children(list.chunks(across).enumerate().map(|(line, pair)| {
        h_flex().gap(rem(tokens.sp_32)).children((0..across).map(|cell| {
            let at = line * across + cell;
            match pair.get(cell) {
                Some(found) => div().flex_1().min_w_0().child(row((name, section * 1000 + at), found, tokens)).into_any_element(),
                None => div().flex_1().into_any_element(),
            }
        }))
    }))
}

/// A section: its title when it is one of several, the pane's one note, the rows, and what the pane
/// keeps without its leader.
pub fn section(at: usize, section: &Section, titled: bool, leader: &str, tokens: &Tokens) -> impl IntoElement {
    let note = (section.chorded && !section.rows.is_empty()).then(|| {
        h_flex()
            .id("sheet-note")
            .flex_wrap()
            .items_center()
            .gap(rem(tokens.sp_4))
            .mb(rem(tokens.sp_8))
            .typed(tokens.type_small)
            .text_color(tokens.fg_muted)
            .child("Keys go to the program. For Argus, press")
            .child(keycap(leader.to_owned(), SharedString::from(tokens.font_mono), tokens))
            .child("then one key:")
    });
    let without = (!section.without.is_empty()).then(|| {
        v_flex()
            .id("sheet-without")
            .mt(rem(tokens.sp_16))
            .pt(rem(tokens.sp_12))
            .border_t_1()
            .border_color(tokens.line)
            .child(div().pb(rem(tokens.sp_2)).typed(Type::new(tokens.weight_semibold, tokens.fs_sm, 1.4)).text_color(tokens.fg_muted).child(SharedString::from(format!("Without {leader}"))))
            .child(rows("sheet-row-without", at, &section.without, true, tokens))
    });
    v_flex()
        .id(("sheet-section", at))
        .test_support()
        .aria_label(SharedString::from(format!("{} shortcuts", section.title)))
        .children(titled.then(|| div().mb(rem(tokens.sp_8)).typed(Type::new(tokens.weight_semibold, tokens.fs_lg, 1.3)).text_color(tokens.fg).child(SharedString::from(section.title.clone()))))
        .children(note)
        .child(rows("sheet-row", at, &section.rows, section.chorded, tokens))
        .children(without)
}
