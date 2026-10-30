//! The diff pane beside the commits: the file under the cursor and its commit with the way to open
//! it as a review, and the diff or the reason there is none.

use argus_desktop_core::code::file::path_parts;
use argus_desktop_core::connection::page::{Call, HistoryPage};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, SharedString};

use super::History;
use crate::kit::file_head::{file_head, Head};
use crate::kit::ghosts::ghosts;
use crate::kit::tools;
use crate::theme::Tokens;

/// The widths of the loading bars of a diff, as the page draws them.
const DIFF_BARS: [f32; 5] = [0.4, 0.9, 0.8, 0.85, 0.6];

impl History {
    /// The kit file head over the diff: the file under the cursor with its counts and `in <sha>`,
    /// and the way to open it as a review.
    pub(super) fn diff_head(&self, page: &HistoryPage, path: &str, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (name, dir) = path_parts(path);
        let counts = page.rows.iter().find(|row| row.on && row.kind == "file").map(|row| (row.added, row.removed));
        let meta = page.commit.as_ref().map(|sha| SharedString::from(format!("in {sha}")));
        let head = Head { status: None, name, dir, counts, meta };
        let review = tools::ghost("history-as-review", "Open as a review", Some("O"), false, tokens)
            .on_click(cx.listener(|this, _, _, cx| this.ask(Call::HistoryAction { action: Action::AsReview, row: None }, cx)))
            .into_any_element();
        file_head("history-diff-head", &head, vec![review], tokens)
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
