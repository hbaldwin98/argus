//! A list block: comments, events, commits. An item with no `who` is an event, drawn as one muted
//! line under a dot so it never reads as a comment.

use argus_desktop_core::views::values::ago;
use argus_protocol::Item;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::pen::Pen;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::chip::chip;
use crate::views::ctx::ViewCtx;
use crate::views::opens::opens_mark;
use crate::views::values::person;

/// `.vlist`: items one under another, a hairline between them; "Nothing yet" for none.
pub fn list(items: &[Item], ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    if items.is_empty() {
        return div().py(rem(tokens.sp_8)).text_color(tokens.fg_muted).child("Nothing yet").into_any_element();
    }
    let last = items.len() - 1;
    let rows: Vec<AnyElement> = items.iter().enumerate().map(|(k, item)| row(item, k, k == last, ctx, pen)).collect();
    v_flex().children(rows).into_any_element()
}

fn row(item: &Item, k: usize, last: bool, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}-item{k}", ctx.path);
    let event = item.who.is_none();
    let title = item.title.as_ref().map(|title| {
        div()
            .when(event, |title| title.text_color(tokens.fg_muted))
            .when(!event, |title| title.ml(rem(tokens.list_indent)).weighted(tokens.weight_semibold))
            .child(SharedString::from(title.clone()))
    });
    let meta = h_flex()
        .flex_wrap()
        .gap(rem(tokens.sp_6))
        .text_size(rem(tokens.fs_sm))
        .text_color(tokens.fg_muted)
        .children(title.filter(|_| event))
        .children(item.who.as_ref().map(|who| person(who, tokens)))
        .children(item.at.as_ref().map(|at| {
            div().id(SharedString::from(format!("{key}-at"))).tooltip(tip(at.clone())).child(ago(pen.draw.clock, at))
        }))
        .children(item.chip.as_ref().map(|c| chip(c, tokens)))
        .children(opens_mark(item.opens.as_deref(), &key, ctx, pen));
    let title = item.title.as_ref().filter(|_| !event).map(|title| {
        div().ml(rem(tokens.list_indent)).weighted(tokens.weight_semibold).child(SharedString::from(title.clone()))
    });
    let body = item.body.as_ref().filter(|body| !body.is_empty()).map(|body| {
        let md = pen.markdown(&format!("{key}|body"), body, true);
        div().min_w_0().text_color(tokens.fg_muted).when(!event, |body| body.ml(rem(tokens.list_indent))).child(md)
    });
    let main = v_flex().min_w_0().flex_1().gap(rem(tokens.sp_2)).child(meta).children(title).children(body);
    h_flex()
        .id(SharedString::from(key))
        .test_support()
        .items_start()
        .gap(rem(tokens.sp_8))
        .py(rem(tokens.sp_8))
        .border_color(tokens.line)
        .when(!last, |row| row.border_b_1())
        .when(event, |row| {
            row.child(
                div()
                    .w(rem(tokens.list_lead_w))
                    .flex_shrink_0()
                    .text_center()
                    .text_color(tokens.fg_faint)
                    .text_size(rem(tokens.fs_xs))
                    .line_height(rem(tokens.list_lead_w))
                    .child("\u{25cf}"),
            )
        })
        .child(main)
        .into_any_element()
}
