//! A timeline block: events in order on a rule their dots hang on, in lanes when they name one.
//!
//! Lanes are columns, one row per event, so row N of every lane is one step of the story (core's
//! `blocks::timeline` says which lane each event is in). Narrow (the box it is drawn in, not the window), the lanes stack into one column and
//! each event carries its lane's name instead.

use argus_desktop_core::views::blocks::timeline::{lane_of, lanes};
use argus_desktop_core::views::values::ago;
use argus_protocol::TimelineEvent;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::pen::Pen;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::chip::tone_colour;
use crate::views::ctx::ViewCtx;
use crate::views::opens::opens_mark;

/// Which lane's name an event wears and whether the rule under its dot stops there.
struct Place<'a> {
    lane: Option<&'a str>,
    last: bool,
}

pub fn timeline(events: &[TimelineEvent], ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let lanes = lanes(events);
    let key = format!("{}|timeline", ctx.path);
    let width = pen.width(&key);
    // Before the box has been measured (zero) it is drawn stacked, which fits any width.
    let narrow = width <= tokens.tl_stack_below;
    let last = events.len().saturating_sub(1);
    if lanes.is_empty() || narrow {
        let drawn: Vec<AnyElement> = events
            .iter()
            .enumerate()
            .map(|(k, e)| {
                let lane = (!lanes.is_empty()).then(|| lanes[lane_of(e, &lanes)]);
                event(e, Place { lane, last: k == last }, k, ctx, pen)
            })
            .collect();
        return v_flex().relative().child(pen.watch(key)).children(drawn).into_any_element();
    }
    let n = lanes.len();
    let heads: Vec<AnyElement> = lanes
        .iter()
        .map(|lane| {
            div()
                .pb(rem(tokens.sp_8))
                .mb(rem(tokens.sp_12))
                .border_b_1()
                .border_color(tokens.line)
                .text_size(rem(tokens.fs_xs))
                .weighted(tokens.weight_semibold)
                .text_color(tokens.fg_muted)
                .child(SharedString::from(lane.to_uppercase()))
                .into_any_element()
        })
        .collect();
    let mut cells: Vec<AnyElement> = Vec::new();
    for (k, e) in events.iter().enumerate() {
        let own = lane_of(e, &lanes);
        for lane in 0..n {
            cells.push(if lane == own {
                div().min_w_0().child(event(e, Place { lane: None, last: k == last && own == n - 1 }, k, ctx, pen)).into_any_element()
            } else {
                div().into_any_element()
            });
        }
    }
    div().relative().grid().grid_cols(n as u16).gap_x(rem(tokens.sp_16)).child(pen.watch(key)).children(heads).children(cells).into_any_element()
}

fn event(e: &TimelineEvent, place: Place, k: usize, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}-event{k}", ctx.path);
    let body = e.body.as_ref().filter(|body| !body.is_empty()).map(|body| {
        let md = pen.markdown(&format!("{key}|body"), body, true);
        div().text_color(tokens.fg_muted).child(md)
    });
    let rule_left = tokens.tl_at_w + tokens.sp_8 + (tokens.tl_dot - tokens.tl_rule_w) / 2.0;
    h_flex()
        .id(SharedString::from(key.clone()))
        .test_support()
        .aria_label(e.title.clone())
        .relative()
        .items_start()
        .gap(rem(tokens.sp_8))
        .pb(rem(tokens.sp_12))
        .when(!place.last, |row| {
            row.child(
                div().absolute().left(rem(rule_left)).top(rem(tokens.tl_rule_top)).bottom(rem(-tokens.tl_rule_bleed)).w(rem(tokens.tl_rule_w)).bg(tokens.line),
            )
        })
        .child(
            div()
                .w(rem(tokens.tl_at_w))
                .flex_shrink_0()
                .text_right()
                .text_size(rem(tokens.fs_sm))
                .text_color(tokens.fg_muted)
                .line_height(rem(tokens.icon_box))
                .child(SharedString::from(e.at.as_deref().map(|at| ago(pen.draw.clock, at)).unwrap_or_default())),
        )
        .child(div().size(rem(tokens.tl_dot)).flex_shrink_0().mt(rem(tokens.sp_4)).rounded_full().bg(tone_colour(e.tone, tokens)))
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .gap(rem(tokens.sp_2))
                .children(place.lane.map(|lane| {
                    div().text_size(rem(tokens.fs_xs)).text_color(tokens.fg_faint).child(SharedString::from(lane.to_uppercase()))
                }))
                .child(
                    h_flex()
                        .weighted(tokens.weight_semibold)
                        .line_height(rem(tokens.icon_box))
                        .child(SharedString::from(e.title.clone()))
                        .children(opens_mark(e.opens.as_deref(), &key, ctx, pen)),
                )
                .children(body),
        )
        .into_any_element()
}
