//! A section block: a quiet heading over its blocks; and the divider that separates two.

use argus_protocol::Block;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::dispatch::blocks;
use super::pen::Pen;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::ctx::ViewCtx;
use crate::views::opens::opens_mark;

/// `.vsection`: the heading in the label type, upper case, and the saved view it opens, over blocks
/// 8px apart. (The page tracks the label's letters out by `0.07em`; GPUI text has no letter
/// spacing, so the port's label is set tight.)
pub fn section(heading: &str, opens: Option<&str>, inner: &[Block], ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = ctx.path.clone();
    v_flex()
        .id(SharedString::from(format!("{key}-section")))
        .test_support()
        .aria_label(heading.to_owned())
        .gap(rem(tokens.sp_8))
        .child(
            h_flex()
                .gap(rem(tokens.sp_8))
                .typed(tokens.type_label)
                .text_color(tokens.fg_faint)
                .child(SharedString::from(heading.to_uppercase()))
                .children(opens_mark(opens, &key, ctx, pen)),
        )
        .child(blocks(inner, ctx, pen))
        .into_any_element()
}

/// `.vdivider`: a hairline.
pub fn divider(tokens: &crate::theme::Tokens) -> AnyElement {
    div().h(rem(tokens.hairline)).w_full().bg(tokens.line).into_any_element()
}
