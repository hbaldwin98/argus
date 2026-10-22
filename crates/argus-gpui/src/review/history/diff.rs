//! The diff pane beside the commits: the file under the cursor and its commit with the way to open
//! it as a review, and the diff or the reason there is none.

use argus_desktop_core::connection::page::{Call, HistoryPage};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::{h_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, SharedString};

use super::History;
use crate::kit::tip::tip;
use crate::kit::button::{button, Look};
use crate::kit::ghosts::ghosts;
use crate::kit::keycap::keycap;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The widths of the loading bars of a diff, as the page draws them.
const DIFF_BARS: [f32; 5] = [0.4, 0.9, 0.8, 0.85, 0.6];

impl History {
    /// The file under the cursor and its commit, with the way to open it as a
    /// review. `.fhdr`: the chrome colour over a hairline.
    pub(super) fn diff_head(&self, page: &HistoryPage, path: &str, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let cut = path.rfind('/');
        let name = SharedString::from(path[cut.map_or(0, |at| at + 1)..].to_owned());
        let dir = cut.map(|at| SharedString::from(path[..at].to_owned()));
        let full = SharedString::from(path.to_owned());
        let commit = SharedString::from(page.commit.clone().unwrap_or_default());
        let mono = cx.theme().mono_font_family.clone();
        h_flex()
            .id("history-diff-head")
            .test_support()
            .aria_label(full)
            .flex_shrink_0()
            .h(rem(tokens.sp_44))
            .px(rem(tokens.sp_12))
            .gap(rem(tokens.sp_12))
            .items_center()
            .bg(tokens.bg_chrome)
            .border_b_1()
            .border_color(tokens.line)
            .child(
                div()
                    .id("history-commit")
                    .test_support()
                    .aria_label(commit.clone())
                    .flex_shrink_0()
                    .font_family(mono.clone())
                    .text_color(tokens.fg_faint)
                    .tooltip(tip("Diffed against its first parent"))
                    .child(commit),
            )
            // The name leads and the folder after it is what gives way: a path
            // cut at its end loses the name.
            .child(
                h_flex()
                    .min_w_0()
                    .gap(rem(tokens.sp_8))
                    .overflow_hidden()
                    .font_family(mono)
                    .child(div().flex_shrink_0().weighted(tokens.weight_semibold).text_color(tokens.fg).child(name))
                    .when_some(dir, |head, dir| head.child(div().min_w_0().truncate().text_color(tokens.fg_faint).child(dir))),
            )
            .child(div().flex_1())
            .child(
                button("history-as-review", "Open as a review", Look::Plain, true, tokens)
                    .child("Open as a review")
                    .child(keycap("O", SharedString::from(tokens.font_mono), tokens))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.ask(Call::HistoryAction { action: Action::AsReview, row: None }, cx)
                    })),
            )
    }

    /// Which file, then the diff's reason before its loading bars, then its rows.
    pub(super) fn diff_body(&self, page: &HistoryPage, tokens: &Tokens) -> gpui_kit::AnyElement {
        if page.path.is_none() {
            return Self::said("history-pick", "Open a commit and pick a file.", tokens).into_any_element();
        }
        if let Some(why) = &page.diff_empty {
            return Self::said("history-diff-empty", why, tokens).into_any_element();
        }
        if page.diff_loading {
            return div().size_full().bg(tokens.bg_sunken).child(ghosts("history-diff-loading", &DIFF_BARS, tokens)).into_any_element();
        }
        div().id("history-rows").test_support().size_full().min_h_0().child(self.diff.clone()).into_any_element()
    }
}
