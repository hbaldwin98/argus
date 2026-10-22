//! A checklist block: what is done and what is not. Read-only: the agent keeps the list, the person
//! only reads it.

use argus_desktop_core::views::blocks::checklist::{state_words, tally};
use argus_protocol::CheckItem;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use super::pen::Pen;
use crate::kit::icons::Glyph;
use crate::theme::scale::rem;
use crate::views::ctx::ViewCtx;
use crate::views::opens::opens_mark;

pub fn checklist(items: &[CheckItem], ctx: &ViewCtx, pen: &Pen) -> AnyElement {
    let tokens = pen.draw.tokens;
    v_flex()
        .child(div().mb(rem(tokens.sp_8)).text_color(tokens.fg_muted).text_size(rem(tokens.fs_sm)).child(tally(items)))
        .child(v_flex().gap(rem(tokens.sp_6)).children(items.iter().enumerate().map(|(k, item)| {
            let key = format!("{}-check{k}", ctx.path);
            let tick = div()
                .flex_shrink_0()
                .size(rem(tokens.check_box))
                .flex()
                .items_center()
                .justify_center()
                .rounded(rem(tokens.r_xs))
                .border(rem(tokens.check_edge))
                .border_color(if item.done { tokens.ok } else { tokens.line_strong })
                .when(item.done, |tick| tick.bg(tokens.ok).text_color(tokens.fg_on_accent).child(Glyph::Tick.at(tokens.icon_sm)));
            h_flex()
                .id(SharedString::from(key.clone()))
                .test_support()
                .aria_label(format!("{}{}", state_words(item.done), item.label))
                .gap(rem(tokens.sp_8))
                .child(tick)
                .child(div().when(item.done, |label| label.text_color(tokens.fg_muted)).child(SharedString::from(item.label.clone())))
                .children(opens_mark(item.opens.as_deref(), &key, ctx, pen))
                .children(
                    item.note.as_ref().map(|note| div().text_color(tokens.fg_faint).text_size(rem(tokens.fs_sm)).child(SharedString::from(note.clone()))),
                )
        })))
        .into_any_element()
}
