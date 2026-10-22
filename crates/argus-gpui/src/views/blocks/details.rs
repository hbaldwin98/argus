//! A details block: a summary line that opens to its blocks. Which are open is the window's alone and
//! outlives a redraw: the agent's `open` is only how one starts, so the one being read does not close
//! under the reader when the view comes again.
//!
//! The summary has no marker, as the page's has none (its flex display drops the browser's).

use argus_protocol::{Block, Chip};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use super::dispatch::blocks;
use super::pen::Pen;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::chip::chip as chip_view;
use crate::views::ctx::ViewCtx;
use crate::views::opens::opens_mark;

pub struct Details<'a> {
    pub summary: &'a str,
    pub chip: Option<&'a Chip>,
    /// How the agent says it starts.
    pub open: bool,
    pub opens: Option<&'a str>,
    pub blocks: &'a [Block],
}

pub fn details(block: Details, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let open = pen.memory.borrow().open(&ctx.path, block.open);
    let (memory, path, repaint) = (pen.memory.clone(), ctx.path.clone(), pen.repaint);
    let key = ctx.path.clone();
    let summary = h_flex()
        .id(SharedString::from(format!("{key}-summary")))
        .role(Role::Button)
        .test_support()
        .aria_label(block.summary.to_owned())
        .gap(rem(tokens.sp_8))
        .cursor_pointer()
        .weighted(tokens.weight_semibold)
        .on_click(move |_, _, cx| {
            memory.borrow_mut().set_open(&path, !open);
            cx.notify(repaint);
        })
        .child(div().min_w_0().child(SharedString::from(block.summary.to_owned())))
        .children(opens_mark(block.opens, &key, ctx, pen))
        .children(block.chip.map(|chip| chip_view(chip, tokens)));
    let body = open.then(|| div().pt(rem(tokens.sp_8)).child(blocks(block.blocks, ctx, pen)));
    v_flex().child(summary).children(body).into_any_element()
}
