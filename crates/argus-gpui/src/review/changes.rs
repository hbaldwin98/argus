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

use argus_desktop_core::connection::page::{Call, ReviewClick, ReviewPage};
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    div, prelude::*, px, relative, Context, Entity, FontWeight, Role, SharedString, Subscription, Window,
};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::diff::view::{DiffEvent, DiffView, Slot};
use crate::review::comment::{self, CommentBox};
use crate::review::files::Files;
use crate::review::hints;
use crate::review::segment;
use crate::review::split::{self, Split};
use crate::theme::Ledger;

/// The width the list of files opens at, and how far the person may drag it.
const FILES: f32 = 320.0;
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
    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let files = cx.new(|cx| Files::new(heard.clone(), caller.clone(), window, cx));
        let comment = cx.new(|cx| CommentBox::new(heard.clone(), caller.clone(), window, cx));
        let diff = cx.new(|_| DiffView::new("review-diff", true));
        let split = cx.new(|cx| Split::new("review-split", "Resize the files", FILES, FILES_MIN, FILES_MAX, cx));
        let resized = cx.observe(&split, |_, _, cx| cx.notify());
        let pressed = cx.subscribe(&diff, |this, _, event: &DiffEvent, cx| {
            let target = match *event {
                DiffEvent::Row { row, extend } => ReviewClick::Row { row, extend },
                DiffEvent::Fold { row, all } => ReviewClick::Fold { row, all },
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
        let slot = page.comment.as_ref().map(|_| Slot { view: self.comment.clone().into(), height: comment::HEIGHT });
        self.diff.update(cx, |diff, cx| diff.show(&page.rows, page.split, slot, cx));
    }

    fn ask(&self, call: Call, cx: &Context<Self>) {
        if !self.heard.read(cx).held() {
            self.caller.call(call);
        }
    }

    /// The file on screen, and the layout the diff is read in.
    fn head(&self, page: &ReviewPage, ledger: &Ledger, cx: &mut Context<Self>) -> impl IntoElement {
        let file = page.file.and_then(|i| page.files.get(i));
        let layout = |id: &'static str, label: &'static str, on: bool, split: bool, cx: &mut Context<Self>| {
            segment::pill(id, Role::RadioButton, on, ledger).child(label).on_click(cx.listener(move |this, _, _, cx| {
                // The one already on sends nothing.
                if page_split(&this.heard, cx) != split {
                    this.ask(Call::ReviewAction { action: Action::Split }, cx);
                }
            }))
        };
        h_flex()
            .id("review-diff-head")
            .flex_shrink_0()
            .h(px(44.0))
            .px_4()
            .gap_3()
            .items_center()
            .border_b_1()
            .border_color(ledger.line)
            .when_some(file, |head, file| {
                head.child(div().flex_shrink_0().font_weight(FontWeight::SEMIBOLD).child(SharedString::from(file.name.clone())))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(ledger.faint)
                            .child(SharedString::from(file.dir.trim_end_matches('/').to_owned())),
                    )
                    .child(div().text_xs().text_color(ledger.success).child(SharedString::from(format!("+{}", file.added))))
                    .child(div().text_xs().text_color(ledger.danger).child(SharedString::from(format!("\u{2212}{}", file.removed))))
            })
            .child(div().flex_1())
            .when_some(page.pick.clone(), |head, pick| {
                head.child(
                    div()
                        .id("review-pick")
                        .test_support()
                        .aria_label(SharedString::from(format!("{pick} picked")))
                        .text_xs()
                        .text_color(ledger.muted)
                        .child(SharedString::from(format!("{pick} picked"))),
                )
            })
            .child(
                segment::track("review-view", "Layout", Role::RadioGroup, ledger)
                    .child(layout("review-view-unified", "Unified", !page.split, false, cx))
                    .child(layout("review-view-split", "Split", page.split, true, cx)),
            )
    }

    /// Loading bars, or why there is no diff, or the diff.
    fn body(&self, page: &ReviewPage, ledger: &Ledger) -> gpui_kit::AnyElement {
        if page.loading {
            return v_flex()
                .id("review-loading")
                .test_support()
                .size_full()
                .p_4()
                .gap_3()
                .bg(ledger.background)
                .children(BARS.map(|width| div().h(px(12.0)).w(relative(width)).rounded(px(3.0)).bg(ledger.raised)))
                .into_any_element();
        }
        if let Some(why) = &page.empty {
            return div()
                .id("review-empty")
                .test_support()
                .aria_label(SharedString::from(why.clone()))
                .size_full()
                .p_6()
                .bg(ledger.background)
                .text_color(ledger.muted)
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
        let ledger = *Ledger::global(cx);
        let Some(page) = self.heard.read(cx).review.clone() else {
            return div().into_any_element();
        };
        let diff = v_flex().id("review-diff-pane").size_full().min_w_0().child(self.head(&page, &ledger, cx)).child(
            div().flex_1().min_h_0().child(self.body(&page, &ledger)),
        );
        v_flex()
            .id("review")
            .test_support()
            .size_full()
            .bg(ledger.background)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(split::side(&self.split, "review-column", self.files.clone(), &ledger, cx))
                    .child(div().flex_1().min_w_0().h_full().child(diff)),
            )
            .child(hints::bar("review-hints", &HINTS, &ledger))
            .into_any_element()
    }
}
