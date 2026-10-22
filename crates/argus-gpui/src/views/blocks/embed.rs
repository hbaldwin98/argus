//! A saved view drawn inside another view: its blocks in a bordered frame under its title, with a
//! link to open it. The embedding block carries only the id; the body is the one this window holds
//! for it, so the frame follows the view when it is saved again. A view that loops back on itself,
//! one nested past the depth cap and one not saved (yet) are a line saying so rather than a frame
//! (core's `blocks::embed` decides which).

use argus_desktop_core::views::blocks::embed::{verdict, words, Verdict};
use argus_protocol::Chip;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, Role, SharedString};

use super::dispatch::blocks;
use super::pen::Pen;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::views::chip::chip;
use crate::views::ctx::ViewCtx;
use crate::views::opens::open_call;
use crate::views::refs::ref_box;
use argus_protocol::Tone;

pub fn embed(id: &str, ctx: &ViewCtx, pen: &mut Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    let key = format!("{}-embed", ctx.path);
    let judged = verdict(id, &ctx.chain, ctx.depth, |held| pen.held(held).is_some());
    let target = pen.held(id).filter(|_| judged == Verdict::Draw);
    let Some(target) = target else {
        let said = words(judged).unwrap_or("view not found");
        return div()
            .id(SharedString::from(key))
            .test_support()
            .aria_label(format!("{said}: {id}"))
            .min_w_0()
            .px(rem(tokens.sp_12))
            .py(rem(tokens.sp_6))
            .border_1()
            .border_dashed()
            .border_color(tokens.line_strong)
            .rounded(rem(tokens.r_sm))
            .bg(tokens.bg_sunken)
            .text_color(tokens.fg_muted)
            .child(SharedString::from(format!("{said}: {id}")))
            .into_any_element();
    };
    let mut chain = (*ctx.chain).clone();
    chain.push(id.to_owned());
    let inner = ViewCtx::saved(target, ctx.saved.clone(), chain, ctx.depth + 1, &ctx.path, pen.saved.is_some_and(|page| page.can_press));
    let (caller, asked) = (pen.draw.caller.clone(), open_call(id, ctx));
    let stale = Chip { label: "stale".into(), tone: Tone::Warn };
    let head = h_flex()
        .flex_wrap()
        .gap(rem(tokens.sp_8))
        .child(div().mr_auto().weighted(tokens.weight_semibold).child(SharedString::from(target.view.title.clone())))
        .children(target.view.status.as_ref().map(|status| chip(status, tokens)))
        .children(target.stale.then(|| chip(&stale, tokens)))
        .child(
            ref_box(SharedString::from(format!("{key}-open")), "Open", "Open this view in its own column", tokens).child("Open \u{2197}").on_click(move |_, _, cx| {
                cx.stop_propagation();
                caller.call(asked.clone());
            }),
        );
    let body = blocks(&target.view.blocks, &inner, pen);
    v_flex()
        .id(SharedString::from(key))
        .role(Role::Region)
        .test_support()
        .aria_label(target.view.title.clone())
        .min_w_0()
        .gap(rem(tokens.sp_8))
        .pt(rem(tokens.sp_8))
        .px(rem(tokens.sp_12))
        .pb(rem(tokens.sp_12))
        .border_1()
        .border_color(tokens.line_strong)
        .rounded(rem(tokens.r_sm))
        .bg(tokens.bg_sunken)
        .child(head)
        .child(body)
        .into_any_element()
}
