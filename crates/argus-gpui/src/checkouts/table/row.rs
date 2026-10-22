//! One row of the table: a checkout or a branch with no directory, as `.ctable td` draws it.
//!
//! A checkout row is its name with a `primary` tag and the branch beside it when the folder's name
//! does not say it, its State, what was last committed there, and its buttons; a branch row names
//! its branch where a checkout names itself, so the first column is never blank. The selected row
//! is highlighted by the theme's mark. A click picks it and a second click opens it; what a click
//! changes is drawn when the session says so.

use argus_desktop_core::checkouts::says::branch_beside;
use argus_desktop_core::checkouts::table::CheckoutsRow;
use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, SharedString, TestSupportExt as _};

use super::columns::Widths;
use super::{actions, state, Ctx};
use crate::checkouts::tag::tag;
use crate::kit::icons::Glyph;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What a row is drawn with besides itself.
pub struct Setting {
    pub selected: bool,
    pub widths: Widths,
    /// Whether the Last commit column is drawn: it gives way when the stage is narrow.
    pub last: bool,
}

/// The Checkout cell: the name in semibold mono, the tag and the branch beside it; or, for a branch
/// row, its icon and its name in the reference colour.
fn name(row: &CheckoutsRow, tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    match &row.name {
        Some(name) => h_flex()
            .items_center()
            .gap(rem(tokens.sp_6))
            .child(div().font_family(mono.clone()).weighted(tokens.weight_semibold).child(SharedString::from(name.clone())))
            .when(row.primary, |cell| cell.child(tag("primary", tokens)))
            .when_some(branch_beside(row), |cell, branch| {
                cell.child(div().ml(rem(tokens.sp_2)).font_family(mono).text_size(rem(tokens.fs_xs)).text_color(tokens.fg_faint).child(SharedString::from(branch.to_owned())))
            }),
        None => h_flex()
            .items_center()
            .gap(rem(tokens.sp_6))
            .font_family(mono)
            .text_color(tokens.ref_)
            .child(Glyph::Branch.at(tokens.icon_sm))
            .child(SharedString::from(row.branch.clone())),
    }
}

/// What was last committed here: the short hash in faint mono, then the summary, muted.
fn last(row: &CheckoutsRow, tokens: &Tokens) -> impl IntoElement {
    div().flex_1().min_w_0().truncate().when_some(row.last.as_ref(), |cell, last| {
        cell.child(
            h_flex()
                .gap(rem(tokens.sp_4))
                .child(div().font_family(SharedString::from(tokens.font_mono)).text_color(tokens.fg_faint).child(SharedString::from(last.short.clone())))
                .child(div().min_w_0().truncate().text_color(tokens.fg_muted).child(SharedString::from(last.summary.clone()))),
        )
    })
}

pub fn row(row: &CheckoutsRow, setting: &Setting, ctx: &Ctx, tokens: &Tokens, cx: &gpui_kit::App) -> impl IntoElement {
    let group = SharedString::from(format!("row-{}", row.key));
    let (key, caller, heard) = (row.key.clone(), ctx.caller.clone(), ctx.heard.clone());
    let pad = rem(tokens.sp_12);
    let cell = |cell: gpui_kit::Div| cell.flex_shrink_0().px(pad).flex().items_center().h_full();
    h_flex()
        .id(SharedString::from(format!("row-{}", row.key)))
        .test_support()
        .aria_selected(setting.selected)
        .aria_label(SharedString::from(row.name.clone().unwrap_or_else(|| row.branch.clone())))
        .group(group.clone())
        .relative()
        .w_full()
        .h(rem(tokens.ck_row_h))
        .flex_shrink_0()
        .items_center()
        .border_b_1()
        .border_color(tokens.line)
        .cursor_pointer()
        .text_size(rem(tokens.fs_sm))
        .text_color(tokens.fg)
        .when(setting.selected, |row| row.bg(tokens.bg_selected).border_color(gpui_kit::transparent_black()))
        .when(!setting.selected, |row| {
            let fill = tokens.fill;
            row.hover(move |style| style.bg(fill))
        })
        .when_some(super::actions::tooltip(row), |element, tip| element.tooltip(tip))
        .on_click(move |click, _, cx| {
            // A held window (the daemon gone) sends nothing.
            if heard.read(cx).held() {
                return;
            }
            caller.call(Call::SelectCheckoutRow { key: key.clone() });
            if click.click_count() >= 2 {
                caller.call(Call::CheckoutsAction { action: Action::Choose });
            }
        })
        // The mark sits on the first cell, as the page's inset shadow does.
        .when(setting.selected, |row| row.child(div().absolute().left_0().top_0().bottom_0().w(rem(tokens.mark_bar_w)).bg(tokens.mark_sel_edge)))
        .child(cell(div().w(rem(setting.widths.name))).child(name(row, tokens)))
        .child(cell(div().w(rem(setting.widths.state))).child(state::cell(row, tokens)))
        .child(if setting.last { cell(div().flex_1().min_w_0()).child(last(row, tokens)).into_any_element() } else { div().flex_1().into_any_element() })
        .child(cell(div().w(rem(setting.widths.actions))).justify_end().child(actions::cell(row, group, setting.selected, ctx, tokens, cx)))
}
