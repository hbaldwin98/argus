//! A card block: blocks in a bordered box with a heading, a chip and buttons of its own.
//!
//! A non-neutral tone only tints the border: the card stays readable in every theme, and the tone
//! still says what the card is about.

use std::rc::Rc;

use argus_protocol::{Block, Chip, Tone, ViewAction};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::dispatch::blocks;
use super::pen::Pen;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::actions::{action_button, PressWith};
use crate::views::chip::{chip as chip_view, tone_colour};
use crate::views::ctx::ViewCtx;
use crate::views::opens::opens_mark;

pub struct Card<'a> {
    pub title: Option<&'a str>,
    pub chip: Option<&'a Chip>,
    pub tone: Tone,
    pub blocks: &'a [Block],
    pub actions: &'a [ViewAction],
    pub opens: Option<&'a str>,
}

pub fn card(block: Card, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = ctx.path.clone();
    let edge = if block.tone == Tone::Neutral { tokens.line } else { tokens.line.blend(tone_colour(block.tone, tokens).opacity(tokens.share_card_edge)) };
    let head = (block.title.is_some() || block.chip.is_some()).then(|| {
        h_flex()
            .gap(rem(tokens.sp_8))
            .min_w_0()
            .children(block.title.map(|title| div().min_w_0().weighted(tokens.weight_semibold).child(SharedString::from(title.to_owned()))))
            .children(opens_mark(block.opens, &key, ctx, pen))
            .children(block.chip.map(|chip| chip_view(chip, tokens)))
    });
    let inner = blocks(block.blocks, ctx, pen);
    let foot = (!block.actions.is_empty()).then(|| {
        h_flex()
            .flex_wrap()
            .gap(rem(tokens.sp_8))
            .children(block.actions.iter().map(|action| action_button(action, ctx, Rc::new(PressWith::default), false, pen)))
    });
    v_flex()
        .id(SharedString::from(format!("{key}-card")))
        .test_support()
        .gap(rem(tokens.sp_12))
        .min_w_0()
        .p(rem(tokens.sp_12))
        .bg(tokens.bg_chrome)
        .border_1()
        .border_color(edge)
        .rounded(rem(tokens.r_md))
        .children(head)
        .child(inner)
        .children(foot)
        .into_any_element()
}
