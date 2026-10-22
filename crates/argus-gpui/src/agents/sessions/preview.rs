//! What the session under the cursor said, and the footer that names the keys and what it offers
//! (SESSIONS-12, -13, -15, -16).
//!
//! The preview is the session's own `SessionPreview`: its header, then the transcript as the
//! reader draws a line of it, with the note of how many earlier entries were left out. A session with
//! no transcript says why in the body and still shows its title, time and checkout.

use argus_desktop_core::agents::said::{earlier, started};
use argus_desktop_core::agents::sessions_dialog::{SessionPreview, SessionsPage};
use argus_desktop_core::system::clock::Clock;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{div, prelude::*, Context, SharedString, TestSupportExt as _};

use super::{said, SessionsView};
use crate::agents::offer::offer;
use crate::agents::transcript::entry::{entry, Mark};
use crate::kit::dialog::foot;
use crate::kit::ghosts::ghosts;
use crate::kit::keycap::keycap;
use crate::kit::tip::tip;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The widths of the loading bars of a preview, as the page draws them.
const BARS: [f32; 6] = [0.5, 0.9, 0.8, 0.4, 0.85, 0.6];

/// The keys the footer names, in order.
const HINTS: [(&[&str], &str); 4] = [(&["J", "K"], "move"), (&["/"], "search"), (&["H", "C"], "harness, checkout"), (&["Esc"], "close")];

impl SessionsView {
    /// `.review-diff`: the header of the session under the cursor over its transcript.
    pub(super) fn preview(&self, page: &SessionsPage, tokens: &Tokens, _cx: &mut Context<Self>) -> impl IntoElement {
        let clock = Clock::local();
        v_flex()
            .id("sessions-preview")
            .test_support()
            .flex_1()
            .min_w_0()
            .h_full()
            .children(page.preview.as_ref().map(|preview| self.preview_head(preview, &clock, tokens)))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .bg(tokens.bg_sunken)
                    .child(match &page.preview {
                        Some(preview) => self.transcript(preview, &clock, tokens),
                        None => said("sessions-pick", "Pick a session.", tokens).into_any_element(),
                    }),
            )
    }

    /// `.fhdr`: the title, the harness, the checkout and when it started, and at the right the first
    /// eight characters of its id, which tell sessions apart.
    fn preview_head(&self, preview: &SessionPreview, clock: &Clock, tokens: &Tokens) -> impl IntoElement {
        let mono = SharedString::from(tokens.font_mono);
        let started = started(preview, clock);
        h_flex()
            .id("sessions-preview-head")
            .test_support()
            .flex_shrink_0()
            .items_center()
            .gap(rem(tokens.sp_12))
            .px(rem(tokens.sp_12))
            .py(rem(tokens.sp_8))
            .border_b_1()
            .border_color(tokens.line)
            .bg(tokens.bg_chrome)
            .text_size(rem(tokens.fs_md))
            .child(div().min_w_0().truncate().text_color(tokens.fg).child(SharedString::from(preview.title.clone())))
            .child(div().flex_shrink_0().typed(tokens.type_small).text_color(tokens.fg_muted).child(SharedString::from(preview.harness.clone())))
            .child(div().flex_shrink(1.0).max_w(rem(tokens.scheckout_max_w)).truncate().typed(tokens.type_small).font_family(mono.clone()).text_color(tokens.fg_faint).child(SharedString::from(preview.checkout.clone())))
            .children(started.map(|words| {
                let words = SharedString::from(words);
                div()
                    .id("sessions-started")
                    .test_support()
                    .aria_label(words.clone())
                    .flex_shrink_0()
                    .typed(tokens.type_small)
                    .text_color(tokens.fg_muted)
                    .tooltip(tip(SharedString::from(preview.started.clone().unwrap_or_default())))
                    .child(words)
            }))
            .child(div().flex_1())
            .child(
                div()
                    .id("sessions-id")
                    .test_support()
                    .aria_label(SharedString::from(preview.session_id.clone()))
                    .flex_shrink_0()
                    .typed(tokens.type_small)
                    .font_family(mono)
                    .text_color(tokens.fg_faint)
                    .tooltip(tip(SharedString::from(preview.session_id.clone())))
                    .child(SharedString::from(preview.session_id.chars().take(8).collect::<String>())),
            )
    }

    /// The loading bars, the notice in place of a transcript, or its lines under the note of how many
    /// earlier ones the daemon left out.
    fn transcript(&self, preview: &SessionPreview, clock: &Clock, tokens: &Tokens) -> gpui_kit::AnyElement {
        if preview.loading {
            return ghosts("sessions-body-loading", &BARS, tokens).into_any_element();
        }
        if let Some(notice) = &preview.notice {
            return said("sessions-notice", notice, tokens).into_any_element();
        }
        v_flex()
            .id("sessions-body")
            .test_support()
            .aria_label("Transcript")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.body)
            .px(rem(tokens.sp_16))
            .py(rem(tokens.sp_12))
            .gap(rem(tokens.sp_8))
            .when(preview.earlier > 0, |body| {
                let note = SharedString::from(earlier(preview.earlier, false));
                body.child(div().id("sessions-earlier").test_support().aria_label(note.clone()).flex_shrink_0().text_center().typed(tokens.type_small).text_color(tokens.fg_muted).child(note))
            })
            .children(preview.entries.iter().enumerate().filter_map(|(at, said)| entry(("sessions-entry", at), said, Mark::None, clock, tokens)))
            .into_any_element()
    }

    /// `.dfoot.meta`: the keys, and at the right what the session under the cursor offers.
    pub(super) fn footer(&self, page: &SessionsPage, tokens: &Tokens, _cx: &mut Context<Self>) -> impl IntoElement {
        let mono = SharedString::from(tokens.font_mono);
        let offers = page.preview.as_ref().map(|preview| offer(preview, true, &self.heard, &self.caller, tokens)).unwrap_or_default();
        foot(tokens)
            .id("sessions-foot")
            .test_support()
            .typed(tokens.type_small)
            .text_color(tokens.fg_muted)
            .children(HINTS.iter().map(|(keys, words)| {
                h_flex()
                    .gap(rem(tokens.sp_6))
                    .items_center()
                    .whitespace_nowrap()
                    .children(keys.iter().map(|key| keycap(SharedString::from((*key).to_owned()), mono.clone(), tokens)))
                    .child(*words)
            }))
            .child(div().flex_1())
            .children(offers)
    }
}
