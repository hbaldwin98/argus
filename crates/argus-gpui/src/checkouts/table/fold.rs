//! The row that stands for the branches no checkout is on: a count until it is opened, then the
//! rows beneath it (`.ctable tr.fold`). It spans the table, and a press on it is the `B` key.

use argus_desktop_core::checkouts::partition::Fold;
use argus_desktop_core::checkouts::ways::Way;
use gpui_kit::component::h_flex;
use gpui_kit::{prelude::*, SharedString, TestSupportExt as _};

use super::Ctx;
use crate::checkouts::act;
use crate::kit::icons::Glyph;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

pub fn fold(fold: &Fold, ctx: &Ctx, tokens: &Tokens) -> impl IntoElement {
    let (caller, heard) = (ctx.caller.clone(), ctx.heard.clone());
    let (hover, rest) = (tokens.fg, tokens.fg_muted);
    h_flex()
        .id("branches-fold")
        .test_support()
        .aria_label(SharedString::from(fold.label.clone()))
        .aria_expanded(fold.shown)
        .tooltip(tip("B"))
        .w_full()
        .h(rem(tokens.ck_fold_h))
        .flex_shrink_0()
        .items_center()
        .gap(rem(tokens.sp_8))
        .px(rem(tokens.sp_12))
        .typed(tokens.type_small)
        .weighted(tokens.weight_semibold)
        .text_color(rest)
        .cursor_pointer()
        .hover(move |style| style.text_color(hover))
        .on_click(move |_, window, cx| act::table(&Way::Branches, &caller, &heard, window, cx))
        .child(if fold.shown { Glyph::Down } else { Glyph::Right }.at(tokens.icon_sm))
        .child(SharedString::from(fold.label.clone()))
}
