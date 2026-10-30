//! The History tab of the Git stage: a checkout's newest commits, each unfolding in place to the
//! files it touched, beside the diff of the file under the cursor. It replaces the history dialog,
//! drawn as a full stage.
//!
//!
//! Everything it draws is the session's `HistoryPage`: the rows with the cursor
//! and which commits are open, why there is no list or no diff, and the rows of
//! the diff of the file under the cursor. A press sends the call the Tauri page
//! sends (`review/history.ts`) and changes nothing on screen; the history
//! changes when the next `History` event says so, so a click on a hidden or
//! refused row leaves the list as the session holds it. A commit's diff
//! arrives only in that event, so none is drawn before it.
//!
//! The diff is the shared view with no cursor, no picking and no folds: Rust
//! filters the folds out, and the view asked for none takes no clicks
//! (HISTORY-9). What a body shows is chosen in the page's order: the list's
//! loading bars, then why it is empty, then rows; the diff's reason before its
//! loading bars (SHARED-3). The split between the commits and the diff is the
//! window's own, never sent anywhere. The filter field is the stage's (`review/filter`), drawn
//! over the commits; its keys and the history's are heard by the stage.

pub mod commits;
pub mod diff;

use argus_desktop_core::connection::page::{Call, HistoryPage};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Entity, ScrollHandle, SharedString, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::diff::{DiffEvent, DiffView};
use crate::kit::hints;
use crate::kit::split::{self, Split};
use crate::review::filter::Filter;
use crate::style::Typed as _;
use crate::theme::scale::rem;
use crate::theme::Tokens;

/// The width the commits open at, and how far the person may drag it.
const COMMITS: f32 = 360.0;
const COMMITS_MIN: f32 = 300.0;
const COMMITS_MAX: f32 = 820.0;

/// The keys the footer names, in order. Bound and unhinted: `o`, `c`, Home and End.
const HINTS: [hints::Hint; 6] = [
    (&["J", "K"], "move"),
    (&["L", "H"], "open / fold"),
    (&["Shift H"], "fold all"),
    (&["Enter"], "as a review"),
    (&["/"], "filter"),
    (&["Esc"], "close"),
];

pub struct History {
    heard: Entity<Heard>,
    caller: Caller,
    diff: Entity<DiffView>,
    /// The filter field, which the stage owns and this draws over the commits.
    filter: Entity<Filter>,
    split: Entity<Split>,
    scroll: ScrollHandle,
    /// The row lit when the list last scrolled to it, and where in the list it stood.
    lit: Option<(usize, usize)>,
    _subscriptions: [Subscription; 2],
}

impl History {
    /// Sends the diff's selected text to Rust to copy; whether there was any.
    pub fn copy(&self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self.diff.read(cx).selection_text() else { return false };
        // Not through `ask`: a copy needs no daemon, so a lost link does not hold it.
        self.caller.call(Call::CopyText { text });
        true
    }

    /// The diff on screen, for the stage's sideways keys.
    pub fn diff(&self) -> &Entity<DiffView> {
        &self.diff
    }

    pub fn new(heard: Entity<Heard>, caller: Caller, filter: Entity<Filter>, cx: &mut Context<Self>) -> Self {
        let diff = cx.new(|cx| DiffView::new("history-diff", false, false, cx));
        let split = cx.new(|cx| Split::new("history-split", "Resize the commits", COMMITS, COMMITS_MIN, COMMITS_MAX, cx));
        let resized = cx.observe(&split, |_, _, cx| cx.notify());
        let copying = cx.subscribe(&diff, |this, _, event: &DiffEvent, cx| {
            if matches!(event, DiffEvent::CopySelection) {
                this.copy(cx);
            }
        });
        Self { heard, caller, diff, filter, split, scroll: ScrollHandle::new(), lit: None, _subscriptions: [resized, copying] }
    }

    /// Follows the session: the diff's rows, and the lit row brought into view
    /// when it moved, by id or by place in the list: a filter or a fold that
    /// takes rows from above a lit row that keeps its id moves it with no
    /// event of its own (SHARED-5, HISTORY-18). A row that stays lit where it
    /// was leaves the scroll where it is.
    pub fn follow(&mut self, page: &HistoryPage, cx: &mut Context<Self>) {
        self.diff.update(cx, |diff, cx| diff.show(&page.diff, false, None, cx));
        let at = page.rows.iter().position(|row| row.on);
        let now = at.map(|at| (page.rows[at].row, at));
        if now != self.lit {
            self.lit = now;
            if let Some((_, at)) = now {
                self.scroll.scroll_to_item(at);
            }
        }
        cx.notify();
    }

    pub(super) fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// A quiet line standing where a pane is not drawn: the reason there is nothing to show.
    /// `.review-empty.fg-muted`: 24px about a line in the body type.
    pub(super) fn said(id: &'static str, words: &str, tokens: &Tokens) -> impl IntoElement {
        div()
            .id(id)
            .test_support()
            .aria_label(SharedString::from(words.to_owned()))
            .size_full()
            .p(rem(tokens.sp_24))
            .typed(tokens.type_body)
            .text_color(tokens.fg_muted)
            .child(SharedString::from(words.to_owned()))
    }
}

impl Render for History {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.heard.read(cx).history.clone() else {
            return div().into_any_element();
        };
        let commits = v_flex()
            .id("history-commits")
            .size_full()
            .min_w_0()
            .child(self.head(&page, &tokens, cx))
            .child(div().flex_1().min_h_0().child(self.list(&page, &tokens, cx)));
        let diff = v_flex()
            .id("history-diff-pane")
            .size_full()
            .min_w_0()
            .when_some(page.path.clone(), |pane, path| pane.child(self.diff_head(&page, &path, &tokens, cx)))
            .child(div().flex_1().min_h_0().bg(tokens.bg_sunken).child(self.diff_body(&page, &tokens)));
        v_flex()
            .id("history")
            .test_support()
            .size_full()
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(split::side(&self.split, "history-column", commits, &tokens, cx))
                    .child(div().flex_1().min_w_0().h_full().child(diff)),
            )
            .child(hints::bar("history-hints", &HINTS, &[], None, &tokens))
            .into_any_element()
    }
}
