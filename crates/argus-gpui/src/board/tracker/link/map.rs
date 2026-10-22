//! The repository's tracker map in the link dialog (BOARD-LINK-9, -10): a summary of its first lines
//! with the draft pill and Edit, or the editor with Cancel, Turn the draft down and Save or Approve.

use argus_desktop_core::board::tracker::link_dialog::{LinkGesture, LinkPage};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, AnyElement, Context, SharedString, TestSupportExt as _};

use super::LinkView;
use crate::kit::button::{button, Look};
use crate::kit::dialog::{label, meta};
use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// How many lines of the map the summary shows.
const SUMMARY_LINES: usize = 6;

impl LinkView {
    pub(super) fn map(&self, page: &LinkPage, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let by = |page: &LinkPage| page.map.draft_by.clone().unwrap_or_else(|| "an agent".to_owned());
        let mono = SharedString::from(tokens.font_mono);
        if page.editing_map.is_some() {
            let drafted = page.map.draft.is_some();
            let label_words = if drafted { format!("Tracker map \u{b7} draft from {}", by(page)) } else { "Tracker map".to_owned() };
            let save = if drafted { "Approve as written" } else { "Save the map" };
            let enter = keycap("Ctrl+Enter", mono, tokens).text_color(tokens.fg_on_accent).border_color(tokens.fg_on_accent.opacity(0.5));
            return v_flex()
                .id("link-map")
                .gap(rem(tokens.sp_6))
                .child(label(label_words, tokens))
                .child(
                    div()
                        .id("link-map-text")
                        .child(Textarea::new(self.map.state()).aria_label("The tracker map").h(rem(tokens.sp_24 * 12.0)).font_family(SharedString::from(tokens.font_mono))),
                )
                .child(
                    h_flex()
                        .gap(rem(tokens.sp_8))
                        .justify_end()
                        .child(button("link-map-cancel", "Cancel", Look::Plain, true, tokens).child("Cancel").on_click(cx.listener(|this, _, _, _| this.send(LinkGesture::CancelMap))))
                        .children(drafted.then(|| {
                            button("link-map-discard", "Turn the draft down", Look::Plain, true, tokens)
                                .child("Turn the draft down")
                                .on_click(cx.listener(|this, _, _, _| this.send(LinkGesture::DiscardDraft)))
                        }))
                        .child(
                            button("link-map-save", save, Look::Primary, true, tokens)
                                .child(save)
                                .child(enter)
                                .on_click(cx.listener(|this, _, _, cx| this.send(LinkGesture::SaveMap { text: this.map.text(cx).to_string() }))),
                        ),
                )
                .into_any_element();
        }
        let lines: Vec<&str> = page.map.body.lines().filter(|line| !line.trim().is_empty()).collect();
        let pill = page.map.draft.as_ref().map(|_| {
            let words = SharedString::from(format!("draft from {} waits", by(page)));
            div()
                .id("link-map-draft")
                .test_support()
                .aria_label(words.clone())
                .px(rem(tokens.sp_8))
                .py(rem(tokens.hairline))
                .rounded(rem(tokens.r_pill))
                .bg(tokens.fill)
                .text_size(rem(tokens.fs_xs))
                .weighted(tokens.weight_semibold)
                .text_color(tokens.attention)
                .child(words)
        });
        v_flex()
            .id("link-map")
            .gap(rem(tokens.sp_6))
            .child(h_flex().gap(rem(tokens.sp_8)).items_center().child(label("The repository\u{2019}s tracker map", tokens)).children(pill))
            .child(if lines.is_empty() {
                meta("No map yet: write one, or ask an agent to draft it.", tokens).into_any_element()
            } else {
                v_flex()
                    .typed(tokens.type_small)
                    .font_family(SharedString::from(tokens.font_mono))
                    .text_color(tokens.fg_muted)
                    .children(lines.into_iter().take(SUMMARY_LINES).map(|line| div().truncate().child(SharedString::from(line.to_owned()))))
                    .into_any_element()
            })
            .child(
                h_flex()
                    .gap(rem(tokens.sp_8))
                    .items_center()
                    .child(div().flex_1().min_w_0().child(meta("An agent reads the map whenever it links or reconciles.", tokens)))
                    .child(
                        button("link-map-edit", "Edit the map\u{2026}", Look::Plain, true, tokens)
                            .child("Edit the map\u{2026}")
                            .child(keycap("M", SharedString::from(tokens.font_mono), tokens))
                            .on_click(cx.listener(|this, _, _, _| this.send(LinkGesture::EditMap))),
                    ),
            )
            .into_any_element()
    }
}
