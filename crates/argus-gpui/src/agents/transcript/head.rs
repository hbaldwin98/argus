//! The reader's head and foot: the way back to the list, what session this is, find, copy and the
//! offer; and the footer's keys (TRANSCRIPT-2, -11, -12).

use argus_desktop_core::agents::said::{find_count, reader_meta};
use argus_desktop_core::connection::page::{Call, ReaderPage};
use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::system::clock::Clock;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, SharedString, TestSupportExt as _};

use super::ReaderView;
use crate::agents::ask::ask;
use crate::agents::offer::offer;
use crate::agents::search::filter;
use crate::kit::button::{button, Look};
use crate::kit::dialog::{esc, foot};
use crate::kit::keycap::keycap;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

fn act(action: Action) -> Call {
    Call::SessionsAction { action, row: None }
}

impl ReaderView {
    /// `.dhead.review-head` in a sheet dialog: a toolbar one row tall, under a rule.
    pub(super) fn head(&self, page: &ReaderPage, clock: &Clock, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let session = &page.session;
        let meta = reader_meta(page, clock);
        let mono = SharedString::from(tokens.font_mono);
        let count = find_count(page);
        let (heard, caller) = (self.heard.clone(), self.caller.clone());
        let back = cx.listener(|this, _, _, cx| ask(&this.heard, &this.caller, act(Action::CloseLayer), cx));
        let copy = cx.listener(|this, _, _, cx| ask(&this.heard, &this.caller, act(Action::CopyId), cx));
        let parts: Vec<gpui_kit::AnyElement> = {
            let said = |words: SharedString| div().flex_shrink_0().child(words).into_any_element();
            let dot = || said(SharedString::from("\u{b7}"));
            let mut parts = Vec::new();
            if !meta.repository.is_empty() {
                parts.push(said(SharedString::from(meta.repository.clone())));
            }
            let checkout = div().flex_shrink_0().font_family(mono.clone()).text_color(tokens.ref_).child(SharedString::from(meta.checkout.clone())).into_any_element();
            parts.push(checkout);
            parts.extend(meta.span.clone().map(|span| said(SharedString::from(span))));
            parts.extend(meta.turns.clone().map(|turns| said(SharedString::from(turns))));
            let mut spaced = Vec::new();
            for (at, part) in parts.into_iter().enumerate() {
                if at > 0 {
                    spaced.push(dot());
                }
                spaced.push(part);
            }
            spaced
        };
        h_flex()
            .id("reader-head")
            .test_support()
            .flex_shrink_0()
            .items_center()
            .gap(rem(tokens.sp_12))
            .min_h(rem(tokens.sp_44 + tokens.sp_12))
            .px(rem(tokens.sp_24))
            .py(rem(tokens.sp_8))
            .border_b_1()
            .border_color(tokens.line)
            .child(button("reader-back", "Back to the recent sessions", Look::Plain, true, tokens).child("\u{2039} Recent sessions").child(esc(tokens)).on_click(back))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_shrink(1.0)
                    .gap(rem(tokens.sp_2))
                    .child(
                        h_flex()
                            .min_w_0()
                            .gap(rem(tokens.sp_8))
                            .items_center()
                            .child(
                                div()
                                    .id("reader-title")
                                    .test_support()
                                    .aria_label(SharedString::from(session.title.clone()))
                                    .min_w_0()
                                    .truncate()
                                    .typed(tokens.type_heading)
                                    .text_color(tokens.fg)
                                    .child(SharedString::from(session.title.clone())),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .px(rem(tokens.sp_6))
                                    .py(rem(tokens.hairline))
                                    .rounded(rem(tokens.r_xs))
                                    .bg(tokens.fill)
                                    .text_color(tokens.fg_muted)
                                    .font_family(mono.clone())
                                    .text_size(rem(tokens.fs_xs))
                                    .child(SharedString::from(session.harness.clone())),
                            ),
                    )
                    .child(h_flex().id("reader-meta").test_support().min_w_0().overflow_hidden().gap(rem(tokens.sp_4)).typed(tokens.type_small).text_color(tokens.fg_muted).children(parts)),
            )
            .child(div().flex_1())
            .child(filter("reader-find", "Find in this transcript", &self.find, tokens))
            .children(count.map(|count| div().id("reader-count").test_support().aria_label(SharedString::from(count.clone())).flex_shrink_0().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(count))))
            .child(
                button("reader-copy", "Copy session id", Look::Plain, true, tokens)
                    .tooltip(tip(SharedString::from(session.session_id.clone())))
                    .child("Copy session id")
                    .child(keycap("Y", mono, tokens))
                    .on_click(copy),
            )
            .children(offer(session, false, &heard, &caller, tokens))
    }
}

/// `.dfoot.meta`: the reader's keys, and at the right the whole session id.
pub(super) fn keys(page: &ReaderPage, tokens: &Tokens) -> impl IntoElement {
    let mono = SharedString::from(tokens.font_mono);
    let cap = |key: &'static str| keycap(key, mono.clone(), tokens);
    let said = |words: &'static str| div().flex_shrink_0().child(words);
    foot(tokens)
        .id("reader-foot")
        .test_support()
        .typed(tokens.type_small)
        .text_color(tokens.fg_muted)
        .child(cap("J"))
        .child(cap("K"))
        .child(said("your turns \u{b7}"))
        .child(cap("/"))
        .child(said("find \u{b7}"))
        .child(cap("N"))
        .child(said("next match \u{b7}"))
        .child(cap("Y"))
        .child(said("copy id \u{b7}"))
        .child(cap("Esc"))
        .child(said("back to the list"))
        .child(div().flex_1())
        .child(div().min_w_0().truncate().font_family(mono.clone()).child(SharedString::from(page.session.session_id.clone())))
}
