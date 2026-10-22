//! A stats block: tiles of an already-formatted value under a label, with an optional delta in the
//! stat's tone, and a spark of its trend under them (the chart renderer's).

use argus_desktop_core::views::blocks::flow::{columns, Fill};
use argus_protocol::{Stat, Tone};
use gpui_kit::component::v_flex;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::chart::spark::spark;
use super::pen::Pen;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::chip::tone_colour;
use crate::views::ctx::ViewCtx;
use crate::views::opens::opens_mark;

/// `.vstats`: as many tiles per row as fit at `--vstat-min`, each stretched to share the width.
pub fn stats(list: &[Stat], ctx: &ViewCtx, pen: &Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}|stats", ctx.path);
    let n = columns(pen.width(&key), tokens.card_min, tokens.sp_8, list.len(), Fill::Fit);
    div()
        .relative()
        .grid()
        .grid_cols(n as u16)
        .gap(rem(tokens.sp_8))
        .child(pen.watch(key))
        .children(list.iter().enumerate().map(|(k, stat)| tile(stat, &format!("{}-stat{k}", ctx.path), ctx, pen)))
        .into_any_element()
}

fn tile(stat: &Stat, key: &str, ctx: &ViewCtx, pen: &Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let tone = tone_colour(stat.tone, tokens);
    v_flex()
        .id(SharedString::from(key.to_owned()))
        .test_support()
        .aria_label(stat.label.clone())
        .gap(rem(tokens.sp_2))
        .min_w_0()
        .px(rem(tokens.sp_16))
        .py(rem(tokens.sp_12))
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_md))
        .child(
            gpui_kit::component::h_flex()
                .text_size(rem(tokens.fs_sm))
                .text_color(tokens.fg_muted)
                .child(div().min_w_0().truncate().child(SharedString::from(stat.label.clone())))
                .children(opens_mark(stat.opens.as_deref(), key, ctx, pen)),
        )
        .child(div().typed(tokens.type_stat).whitespace_nowrap().child(SharedString::from(stat.value.clone())))
        .children(stat.delta.as_ref().map(|delta| {
            div().text_size(rem(tokens.fs_sm)).text_color(if stat.tone == Tone::Neutral { tokens.fg_muted } else { tone }).child(SharedString::from(delta.clone()))
        }))
        .children((!stat.trend.is_empty()).then(|| div().mt(rem(tokens.sp_4)).child(spark(&stat.trend, stat.tone, tokens.stat_spark_h, key, tokens))))
        .into_any_element()
}
