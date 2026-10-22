//! The column beside the table about the selected row: where it is, what it tracks, what runs on it
//! and what can be done to it (`.cside`). It follows the selection, and every button is a key the
//! table answers to as well.
//!
//! What it says is `argus-desktop-core`'s `side`; this draws it. It gives way to the Files panel,
//! which takes its place, and to a narrow window (`ck_side_hides_below`), where the overflow on
//! each row holds what it does.

pub mod facts;
pub mod foot;
pub mod running;

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::checkouts::side::{side, Act};
use argus_desktop_core::checkouts::table::{Checkouts, CheckoutsRow};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, App, SharedString, TestSupportExt as _};

use super::act;
use super::table::Ctx;
use super::tag::tag;
use crate::kit::button::{button, Look};
use crate::kit::keycap::keycap;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// One of the buttons under the title: its words and, when the table binds one, the key.
fn act_button(act: &Act, key: &str, ctx: &Ctx, tokens: &Tokens) -> impl IntoElement {
    let (caller, heard, way, row) = (ctx.caller.clone(), ctx.heard.clone(), act.way.clone(), key.to_owned());
    let mut button = button(SharedString::from(format!("side-{}", act.label.to_lowercase().replace(' ', "-"))), act.label, Look::Plain, true, tokens).child(act.label);
    if let Some(cap) = act.key {
        button = button.child(keycap(cap, SharedString::from(tokens.font_mono), tokens));
    }
    button.on_click(move |_, window, cx| act::way(&way, &row, &caller, &heard, window, cx))
}

/// The column for `row`, the selected one, in the table `table`.
pub fn column(table: &Checkouts, row: &CheckoutsRow, ctx: &Ctx, hover: &Rc<Cell<bool>>, tokens: &Tokens, cx: &App) -> impl IntoElement {
    let said = side(table, row);
    let mono = SharedString::from(tokens.font_mono);
    let title = h_flex()
        .gap(rem(tokens.sp_8))
        .items_center()
        .min_w_0()
        .child(div().id("side-title").test_support().aria_label(SharedString::from(said.title.clone())).min_w_0().truncate().typed(tokens.type_title).text_color(tokens.fg).when(said.mono, |title| title.font_family(mono.clone())).child(SharedString::from(said.title.clone())))
        .when(said.primary, |title| title.child(tag("primary", tokens)));
    v_flex()
        .id("side")
        .test_support()
        .aria_label(SharedString::from(format!("{}, selected", said.title)))
        .flex_shrink_0()
        .w(rem(tokens.ck_side_w))
        .h_full()
        .overflow_y_scroll()
        .border_l_1()
        .border_color(tokens.line)
        .bg(tokens.bg_chrome)
        .p(rem(tokens.sp_20))
        .gap(rem(tokens.sp_16))
        .child(
            v_flex()
                .child(title)
                .when_some(said.path.clone(), |column, (short, whole)| {
                    column.child(
                        div()
                            .id("side-path")
                            .test_support()
                            .aria_label(SharedString::from(short.clone()))
                            .tooltip(tip(whole))
                            .mt(rem(tokens.sp_4))
                            .truncate()
                            .font_family(mono.clone())
                            .text_size(rem(tokens.fs_xs))
                            .text_color(tokens.fg_faint)
                            .child(SharedString::from(short)),
                    )
                })
                .when_some(said.note, |column, note| column.child(div().id("side-note").test_support().aria_label(note).typed(tokens.type_small).text_color(tokens.fg_muted).child(note))),
        )
        .child(h_flex().id("side-acts").test_support().flex_wrap().gap(rem(tokens.sp_6)).children(said.acts.iter().map(|act| act_button(act, &row.key, ctx, tokens))))
        .when(!said.facts.is_empty(), |column| column.child(facts::facts(&said.facts, tokens)))
        .when_some(said.running.clone(), |column, heading| column.child(running::running(&heading, &row.running, ctx, tokens)))
        .child(div().flex_1())
        .child(foot::foot(&said.foot, &said.title, &row.key, ctx, hover, tokens, cx))
}
