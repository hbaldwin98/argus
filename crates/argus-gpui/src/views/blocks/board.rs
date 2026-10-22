//! A board block: columns of cards, each column a fixed width, the whole scrolling sideways.

use argus_protocol::{BoardCard, BoardColumn};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::pen::Pen;
use crate::style::Typed as _;
use crate::kit::wheel::Wheel;
use crate::theme::scale::rem;
use crate::views::chip::chip;
use crate::views::ctx::ViewCtx;
use crate::views::opens::opens_mark;
use crate::views::values::value;

pub fn board(columns: &[BoardColumn], ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let scroll = pen.scroll(&format!("{}-board", ctx.path));
    let wheel = Wheel::new(&scroll);
    let drawn: Vec<AnyElement> = columns.iter().enumerate().map(|(k, column)| lane(column, k, ctx, pen)).collect();
    div()
        .id(SharedString::from(format!("{}-board", ctx.path)))
        .test_support()
        .overflow_x_scroll()
        .track_scroll(&scroll)
        .on_scroll_wheel(wheel.keep())
        .child(wheel.probe())
        .child(h_flex().items_start().gap(rem(tokens.sp_12)).pb(rem(tokens.sp_8)).children(drawn))
        .into_any_element()
}

/// `.vboard-col`: a title, how many cards and a chip over the cards.
fn lane(column: &BoardColumn, k: usize, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}-col{k}", ctx.path);
    let cards: Vec<AnyElement> = column.cards.iter().enumerate().map(|(c, card)| face(card, &format!("{key}-card{c}"), ctx, pen)).collect();
    v_flex()
        .id(SharedString::from(key))
        .test_support()
        .aria_label(column.title.clone())
        .w(rem(tokens.board_col_w))
        .flex_shrink_0()
        .gap(rem(tokens.sp_8))
        .p(rem(tokens.sp_8))
        .bg(tokens.bg_sunken)
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_md))
        .child(
            h_flex()
                .gap(rem(tokens.sp_8))
                .weighted(tokens.weight_semibold)
                .child(SharedString::from(column.title.clone()))
                .child(div().text_color(tokens.fg_muted).weighted(tokens.weight_regular).text_size(rem(tokens.fs_sm)).child(column.cards.len().to_string()))
                .children(column.chip.as_ref().map(|c| div().ml_auto().child(chip(c, tokens)))),
        )
        .child(v_flex().gap(rem(tokens.sp_8)).children(cards))
        .into_any_element()
}

/// `.vboard-card`: the title and a chip, a value and some Markdown under it.
fn face(card: &BoardCard, key: &str, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let body = card.body.as_ref().filter(|body| !body.is_empty()).map(|body| {
        let md = pen.markdown(&format!("{key}|body"), body, true);
        div().text_color(tokens.fg_muted).text_size(rem(tokens.fs_sm)).child(md)
    });
    v_flex()
        .id(SharedString::from(key.to_owned()))
        .test_support()
        .aria_label(card.title.clone())
        .min_w_0()
        .gap(rem(tokens.sp_4))
        .p(rem(tokens.sp_8))
        .border_1()
        .border_color(tokens.line)
        .rounded(rem(tokens.r_sm))
        .child(
            h_flex()
                .items_start()
                .justify_between()
                .gap(rem(tokens.sp_8))
                .child(div().min_w_0().weighted(tokens.weight_semibold).child(SharedString::from(card.title.clone())))
                .children(opens_mark(card.opens.as_deref(), key, ctx, pen))
                .children(card.chip.as_ref().map(|c| chip(c, tokens))),
        )
        .children(card.value.as_ref().map(|v| div().text_color(tokens.fg_muted).text_size(rem(tokens.fs_sm)).child(value(v, &format!("{key}-value"), &pen.at(ctx)))))
        .children(body)
        .into_any_element()
}
