//! A columns block: blocks side by side while there is room, one under the other when the box is
//! narrow. Which it is, is the box's measured width (core's `blocks::flow`), not the window's.

use argus_desktop_core::views::blocks::flow::{columns as fit, Fill};
use argus_protocol::Block;
use gpui_kit::{div, prelude::*, AnyElement};

use super::dispatch::blocks;
use super::pen::Pen;
use crate::theme::scale::rem;
use crate::views::ctx::ViewCtx;

pub fn columns(all: &[Vec<Block>], ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}|columns", ctx.path);
    let n = fit(pen.width(&key), tokens.col_min, tokens.sp_16, all.len(), Fill::Fit);
    let cells: Vec<AnyElement> = all.iter().enumerate().map(|(k, column)| div().min_w_0().child(blocks(column, &ctx.at(k), pen)).into_any_element()).collect();
    div().relative().grid().grid_cols(n as u16).gap(rem(tokens.sp_16)).child(pen.watch(key)).children(cells).into_any_element()
}
