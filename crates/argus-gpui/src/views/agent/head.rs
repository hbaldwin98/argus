//! A view's header under the tab strip: its title, status and buttons on one row and, under them,
//! the subject as a link and the meta line (`header`, `views/agent.ts`).
//!
//! As a page the stage's strip carries the title, the status and the buttons, so only the meta line
//! is left (`popped`). The age in the meta line is said by the clock each draw reads, and the panel
//! draws again on a timer so it does not go stale.

use argus_desktop_core::connection::page::PageView;
use argus_desktop_core::system::clock::Clock;
use argus_desktop_core::views::head;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, AnyElement, SharedString};

use crate::connection::caller::Caller;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;
use crate::views::chip::chip;
use crate::views::ctx::Draw;
use crate::views::link::view_link;

/// `.vpanel-head`: 12px over 16, a hairline under it.
pub fn header(view: &PageView, popped: bool, acts: Vec<AnyElement>, clock: &Clock, caller: &Caller, tokens: &Tokens) -> impl IntoElement {
    let draw = Draw { tokens, clock, caller, saved: &[], view: None };
    let title = (!popped).then(|| {
        h_flex()
            .id("view-title")
            .test_support()
            .items_center()
            .gap(rem(tokens.sp_8))
            .child(div().min_w_0().typed(tokens.type_heading).text_color(tokens.fg).truncate().child(SharedString::from(view.view.title.clone())))
            .children(view.view.status.as_ref().map(|status| chip(status, tokens)))
            // `.vpanel-acts`: one group at the row's end.
            .child(h_flex().flex_none().items_center().gap(rem(tokens.sp_8)).ml_auto().children(acts))
    });
    let subject = view.subject.as_ref().map(|subject| {
        let words = format!("{} {}", subject.tracker, subject.key);
        view_link("view-subject", words, &subject.url, &draw)
            .gap(rem(tokens.sp_4))
            .child(SharedString::from(subject.tracker.clone()))
            .child(div().weighted(tokens.weight_bold).child(SharedString::from(subject.key.clone())))
    });
    let meta = SharedString::from(head::meta(view, clock));
    v_flex()
        .id("view-head")
        .test_support()
        .flex_none()
        .gap(rem(tokens.sp_6))
        .px(rem(tokens.sp_16))
        .py(rem(tokens.sp_12))
        .border_b_1()
        .border_color(tokens.line)
        .children(title)
        .child(
            h_flex()
                .id("view-meta")
                .test_support()
                .items_center()
                .gap(rem(tokens.sp_8))
                .flex_wrap()
                .children(subject)
                .child(
                    div()
                        .id("view-age")
                        .text_size(rem(tokens.fs_xs))
                        .text_color(tokens.fg_faint)
                        .tooltip(tip(head::meta_tip(view, clock)))
                        .child(meta),
                ),
        )
}
