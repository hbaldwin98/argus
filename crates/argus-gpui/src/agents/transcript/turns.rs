//! The reader's jump list: one row per turn of the person's, its first line and its time, the one
//! the reader is on lit (TRANSCRIPT-5, -6).
//!
//! A click sends `next` with the turn's place in the list; Rust moves there and the body scrolls so
//! that turn is at its top. Nothing is drawn lit until the first move (TRANSCRIPT-16).

use argus_desktop_core::connection::page::{Call, ReaderPage};
use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::system::clock::Clock;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, SharedString, TestSupportExt as _};

use super::ReaderView;
use crate::agents::ask::ask;
use crate::kit::selected::selected;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

impl ReaderView {
    /// `.reader-turns`: the aside under its label, with the note about the file it was read from at
    /// its foot.
    pub(super) fn turns(&self, page: &ReaderPage, clock: &Clock, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let session = &page.session;
        let empty = page.jumps.is_empty() && !session.loading;
        v_flex()
            .id("reader-turns")
            .test_support()
            .aria_label("Your turns")
            .flex_shrink_0()
            .w(rem(tokens.list_col_w))
            .h_full()
            .border_r_1()
            .border_color(tokens.line)
            .p(rem(tokens.sp_12))
            .gap(rem(tokens.sp_2))
            .overflow_y_scroll()
            .track_scroll(&self.jumps)
            .child(
                div()
                    .pb(rem(tokens.sp_6))
                    .typed(tokens.type_label)
                    .text_color(tokens.fg_faint)
                    .child("YOUR TURNS"),
            )
            .when(empty, |aside| aside.child(div().typed(tokens.type_small).text_color(tokens.fg_muted).child("Nothing was asked in what was kept.")))
            .children(page.jumps.iter().enumerate().map(|(at, jump)| {
                let time = clock.hhmm(jump.at.as_deref()).unwrap_or_default();
                let tooltip = SharedString::from(jump.text.clone());
                selected(
                    h_flex()
                        .id(("reader-turn", at))
                        .test_support()
                        .aria_label(SharedString::from(format!("Turn: {tooltip}")))
                        .aria_selected(jump.on)
                        .flex_shrink_0()
                        .w_full()
                        .min_h(rem(tokens.control_h))
                        .px(rem(tokens.sp_8))
                        .gap(rem(tokens.sp_8))
                        .items_center()
                        .justify_between()
                        .rounded(rem(tokens.r_sm))
                        .cursor_pointer()
                        .text_color(tokens.fg)
                        .when(!jump.on, |row| row.hover(move |style| style.bg(tokens.fill)))
                        .tooltip(tip(tooltip))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            ask(&this.heard, &this.caller, Call::SessionsAction { action: Action::Next, row: Some(at) }, cx)
                        }))
                        .child(div().min_w_0().truncate().text_size(rem(tokens.fs_sm)).child(SharedString::from(jump.text.clone())))
                        .child(div().flex_shrink_0().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(time))),
                    jump.on,
                    tokens,
                )
            }))
            .child(div().flex_1())
            .child(
                div()
                    .pt(rem(tokens.sp_12))
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .child(SharedString::from(format!("Read from {}'s own transcript file. A harness without one lists its sessions but cannot show them.", session.harness))),
            )
    }
}
