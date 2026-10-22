//! A row block: values and buttons on one line, wrapping when narrow.

use std::rc::Rc;

use argus_protocol::{RowAlign, RowItem};
use gpui_kit::component::h_flex;
use gpui_kit::{prelude::*, AnyElement};

use super::pen::Pen;
use crate::theme::scale::rem;
use crate::views::actions::{action_button, PressWith};
use crate::views::ctx::ViewCtx;
use crate::views::values::value;

pub fn row(align: RowAlign, items: &[RowItem], ctx: &ViewCtx, pen: &Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let draw = pen.at(ctx);
    h_flex()
        .flex_wrap()
        .gap(rem(tokens.sp_8))
        .when(align == RowAlign::Spread, |row| row.justify_between())
        .when(align == RowAlign::End, |row| row.justify_end())
        .children(items.iter().enumerate().map(|(k, item)| match item {
            RowItem::Value { value: v } => value(v, &format!("{}.{k}", ctx.path), &draw),
            RowItem::Action { action } => action_button(action, &ctx.at(k), Rc::new(PressWith::default), false, pen),
        }))
        .into_any_element()
}
