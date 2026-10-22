//! The Changes tab of the Git stage: the review of a checkout's working
//! changes, or of one commit, drawn as a full stage. The changed files stand
//! beside the diff of one, lines are picked and commented on in place, and the
//! keys that move among them are the review's (`review/keys`, heard by the
//! stage).
//!
//! Everything it draws is the session's `ReviewPage`: the files, the file on
//! screen, the rows with the cursor and the pick, the comment box, the jump
//! field's matches, and why there is no diff when there is none. A press sends
//! the call the Tauri page sends (`review/dialog.ts`) and changes nothing on
//! screen; the review changes when the next `Review` event says so, so a
//! refused click leaves the diff as the session holds it.
//!
//! What a body shows is chosen in the page's order (SHARED-3): loading bars,
//! then the muted reason there is no diff, then the rows. The split between
//! the files and the diff is the window's own, never sent anywhere.

use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, ReviewClick, ReviewPage};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Entity, Role, SharedString, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::counts::counts;
use crate::kit::diff::{DiffEvent, DiffView, Slot};
use crate::review::comment::{self, ways, CommentBox};
use crate::review::files::Files;
use crate::kit::ghosts::ghosts;
use crate::kit::hints;
use crate::kit::segment;
use crate::kit::split::{self, Split};
use crate::style::Typed as _;
use crate::theme::scale::{rem, zoom};
use crate::theme::Tokens;

/// The width the list of files opens at (`--list-col-w`), and how far the
/// person may drag it.
const FILES: f32 = 300.0;
const FILES_MIN: f32 = 200.0;
const FILES_MAX: f32 = 520.0;

/// The widths of the loading bars of a review's diff, as the page draws them.
const BARS: [f32; 6] = [0.4, 0.9, 0.8, 0.85, 0.6, 0.75];

/// The keys the footer names, in order. The ones bound and unhinted today are `[` `]`,
/// `z`, Home, End and `o`, as in the page.
const HINTS: [hints::Hint; 9] = [
    (&["J", "K"], "line"),
    (&["V"], "pick"),
    (&["C"], "comment"),
    (&["B"], "side"),
    (&["S"], "split"),
    (&["Ctrl P"], "jump"),
    (&["E"], "editor"),
    (&["Enter"], "unfold"),
    (&["Esc"], "close"),
];

pub struct Changes {
    heard: Entity<Heard>,
    caller: Caller,
    files: Entity<Files>,
    diff: Entity<DiffView>,
    comment: Entity<CommentBox>,
    split: Entity<Split>,
    _subscriptions: [Subscription; 2],
}

impl Changes {
    /// The diff on screen, for the stage's sideways keys.
    pub fn diff(&self) -> &Entity<DiffView> {
        &self.diff
    }

    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let files = cx.new(|cx| Files::new(heard.clone(), caller.clone(), window, cx));
        let comment = cx.new(|cx| CommentBox::new(heard.clone(), caller.clone(), Rc::new(ways::Review), window, cx));
        let diff = cx.new(|cx| DiffView::new("review-diff", true, true, cx));
        let split = cx.new(|cx| Split::new("review-split", "Resize the files", FILES, FILES_MIN, FILES_MAX, cx));
        let resized = cx.observe(&split, |_, _, cx| cx.notify());
        let pressed = cx.subscribe(&diff, |this, _, event: &DiffEvent, cx| {
            let target = match *event {
                DiffEvent::CopySelection => {
                    this.copy(cx);
                    return;
                }
                DiffEvent::Row { row, extend } => ReviewClick::Row { row, extend },
                DiffEvent::Fold { row, all } => ReviewClick::Fold { row, all },
                // A drag across rows is the click on its first and the shift-click
                // on its last, and nothing is picked until the session says so.
                DiffEvent::Range { first, last } => {
                    this.ask(Call::ReviewClick { target: ReviewClick::Row { row: first, extend: false } }, cx);
                    ReviewClick::Row { row: last, extend: true }
                }
            };
            this.ask(Call::ReviewClick { target }, cx);
        });
        Self { heard, caller, files, diff, comment, split, _subscriptions: [pressed, resized] }
    }

    /// Follows the session: the box, the field, then the rows, in the order
    /// that lets an open jump field win the keys over an open comment box
    /// (REVIEW-17).
    pub fn follow(&mut self, page: &ReviewPage, window: &mut Window, cx: &mut Context<Self>) {
        self.comment.update(cx, |comment, cx| comment.follow(page.comment.as_ref(), window, cx));
        self.files.update(cx, |files, cx| files.follow(page, window, cx));
        let slot = page.comment.as_ref().map(|c| Slot { view: self.comment.clone().into(), height: comment::height(c, zoom(window.rem_size())) });
        self.diff.update(cx, |diff, cx| diff.show(&page.rows, page.split, slot, cx));
    }

    /// Sends the diff's selected text to Rust to copy; whether there was any.
    pub fn copy(&self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self.diff.read(cx).selection_text() else { return false };
        // Not through `ask`: a copy needs no daemon, so a lost link does not hold it.
        self.caller.call(Call::CopyText { text });
        true
    }

    fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// The file on screen, and the layout the diff is read in.
    fn head(&self, page: &ReviewPage, tokens: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let file = page.file.and_then(|i| page.files.get(i));
        let layout = |id: &'static str, label: &'static str, on: bool, split: bool, cx: &mut Context<Self>| {
            segment::pill(id, Role::RadioButton, on, tokens).child(label).on_click(cx.listener(move |this, _, _, cx| {
                // The one already on sends nothing.
                if page_split(&this.heard, cx) != split {
                    this.ask(Call::ReviewAction { action: Action::Split }, cx);
                }
            }))
        };
        // `.fhdr`: on the chrome colour over a hairline, 12px about its parts.
        h_flex()
            .id("review-diff-head")
            .flex_shrink_0()
            .h(rem(tokens.sp_44))
            .px(rem(tokens.sp_12))
            .gap(rem(tokens.sp_12))
            .items_center()
            .bg(tokens.bg_chrome)
            .border_b_1()
            .border_color(tokens.line)
            .when_some(file, |head, file| {
                head.child(div().flex_shrink_0().weighted(tokens.weight_bold).child(SharedString::from(file.name.clone())))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .typed(tokens.type_small)
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(tokens.fg_muted)
                            .child(SharedString::from(file.dir.trim_end_matches('/').to_owned())),
                    )
                    .child(counts(file.added, file.removed, tokens).typed(tokens.type_small))
            })
            .child(div().flex_1())
            // `.ktag`: what is picked, as a pill.
            .when_some(page.pick.clone(), |head, pick| {
                head.child(
                    div()
                        .id("review-pick")
                        .test_support()
                        .aria_label(SharedString::from(format!("{pick} picked")))
                        .flex_shrink_0()
                        .px(rem(tokens.sp_8))
                        .rounded(rem(tokens.r_pill))
                        .border_1()
                        .border_color(tokens.line_strong)
                        .typed(tokens.type_small)
                        .text_color(tokens.fg_muted)
                        .whitespace_nowrap()
                        .child(SharedString::from(format!("{pick} picked"))),
                )
            })
            .child(
                segment::track("review-view", "Layout", Role::RadioGroup, tokens)
                    .child(layout("review-view-unified", "Unified", !page.split, false, cx))
                    .child(layout("review-view-split", "Split", page.split, true, cx)),
            )
    }

    /// Loading bars, or why there is no diff, or the diff.
    fn body(&self, page: &ReviewPage, tokens: &Tokens) -> gpui_kit::AnyElement {
        if page.loading {
            return div().size_full().bg(tokens.bg_sunken).child(ghosts("review-loading", &BARS, tokens)).into_any_element();
        }
        if let Some(why) = &page.empty {
            // `.review-empty`: 24px about a line in the body type.
            return div()
                .id("review-empty")
                .test_support()
                .aria_label(SharedString::from(why.clone()))
                .size_full()
                .p(rem(tokens.sp_24))
                .bg(tokens.bg_sunken)
                .typed(tokens.type_body)
                .text_color(tokens.fg_muted)
                .child(SharedString::from(why.clone()))
                .into_any_element();
        }
        div().id("review-rows").test_support().size_full().min_h_0().child(self.diff.clone()).into_any_element()
    }
}

/// Whether the review the session holds is split, for the pill that is already on.
fn page_split(heard: &Entity<Heard>, cx: &Context<Changes>) -> bool {
    heard.read(cx).review.as_ref().is_some_and(|page| page.split)
}

impl Render for Changes {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.heard.read(cx).review.clone() else {
            return div().into_any_element();
        };
        let diff = v_flex().id("review-diff-pane").size_full().min_w_0().child(self.head(&page, &tokens, cx)).child(
            div().flex_1().min_h_0().child(self.body(&page, &tokens)),
        );
        v_flex()
            .id("review")
            .test_support()
            .size_full()
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(split::side(&self.split, "review-column", self.files.clone(), &tokens, cx))
                    .child(div().flex_1().min_w_0().h_full().child(diff)),
            )
            .child(hints::bar("review-hints", &HINTS, &tokens))
            .into_any_element()
    }
}
