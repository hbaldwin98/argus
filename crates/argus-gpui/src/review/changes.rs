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
//!
//! A review of working changes stages: each changed line has a box, ticked
//! when the diff is the index's, and each hunk a button (`kit::diff::tick`).
//! A tick, a Shift-tick from the last one, or a hunk is a `Stage` call, and
//! the boxes move when the diff read after the index moved says so. The
//! review's drafts hang under their lines (`draft_card`), and the Review
//! button over the diff opens the panel that sends them (`submit`).

use std::collections::HashMap;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, ReviewClick, ReviewPage};
use argus_desktop_core::review::staging::StageGesture;
use argus_desktop_core::keys::keymap::Action;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Entity, Role, SharedString, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::counts::counts;
use crate::kit::diff::gutter::{shown, GUTTER};
use crate::kit::diff::tick::{tickable, Ticks, BOX};
use crate::kit::diff::{DiffEvent, DiffView, Slot};
use crate::review::comment::{self, ways, CommentBox};
use crate::review::draft_card::{self, DraftCard};
use crate::review::files::Files;
use crate::review::submit::Submit;
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
    submit: Entity<Submit>,
    /// Each draft on screen's card, by its id, and what hears the card grow as it is edited.
    cards: HashMap<u64, (Entity<DraftCard>, Subscription)>,
    /// The row of the last box ticked, which a Shift-tick ranges from.
    last_tick: Option<usize>,
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
        let submit = cx.new(|cx| Submit::new(heard.clone(), caller.clone(), window, cx));
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
                DiffEvent::Tick { row, extend } => {
                    this.tick(row, extend, cx);
                    return;
                }
                DiffEvent::Hunk { row } => {
                    this.ask(Call::Stage { gesture: StageGesture::Hunk { row } }, cx);
                    return;
                }
            };
            this.ask(Call::ReviewClick { target }, cx);
        });
        Self { heard, caller, files, diff, comment, split, submit, cards: HashMap::new(), last_tick: None, _subscriptions: [pressed, resized] }
    }

    /// A box ticked: its line, or with Shift every changed line from the last box ticked to it.
    fn tick(&mut self, row: usize, extend: bool, cx: &mut Context<Self>) {
        let rows = match (extend, self.last_tick) {
            (true, Some(from)) => {
                let page = self.heard.read(cx).review.clone();
                let rows = page.map(|page| page.rows.clone()).unwrap_or_default();
                let at = |n: usize| rows.iter().position(|r| r.row == n);
                match (at(from), at(row)) {
                    (Some(a), Some(b)) => rows[a.min(b)..=a.max(b)]
                        .iter()
                        .filter(|r| tickable(r.left.as_ref()) || tickable(r.right.as_ref()))
                        .map(|r| r.row)
                        .collect(),
                    _ => vec![row],
                }
            }
            _ => vec![row],
        };
        self.last_tick = Some(row);
        self.ask(Call::Stage { gesture: StageGesture::Rows { rows } }, cx);
    }

    /// How far in from the left a card among the rows stands, so its words begin where the code does.
    fn card_left(page: &ReviewPage) -> f32 {
        let numbers = if page.split { GUTTER } else { shown(&page.rows).width() };
        numbers + if page.staging.is_some() { BOX } else { 0.0 }
    }

    /// Each draft written on the file and side on screen, as a card under its row; a card that
    /// grows as it is edited is measured again.
    fn place_notes(&mut self, page: &ReviewPage, window: &mut Window, cx: &mut Context<Self>) {
        let zoom = zoom(window.rem_size());
        let left = Self::card_left(page);
        let drafts: Vec<_> = page.drafts.iter().flat_map(|d| d.drafts.iter()).filter(|d| d.row.is_some()).cloned().collect();
        self.cards.retain(|id, _| drafts.iter().any(|d| d.id == *id));
        let mut notes = Vec::new();
        for draft in drafts {
            let (heard, caller) = (self.heard.clone(), self.caller.clone());
            let (card, _) = self.cards.entry(draft.id).or_insert_with(|| {
                let card = cx.new(|_| DraftCard::new(heard, caller, draft.clone()));
                let grows = cx.observe_in(&card, window, |this, _, window, cx| {
                    if let Some(page) = this.heard.read(cx).review.clone() {
                        this.place_notes(&page, window, cx);
                    }
                });
                (card, grows)
            });
            let card = card.clone();
            let editing = card.update(cx, |card, cx| {
                card.follow(&draft, left, cx);
                card.editing()
            });
            let height = draft_card::height(&draft, editing, zoom);
            notes.push((draft.row.unwrap_or_default(), Slot { view: card.into(), height }));
        }
        self.diff.update(cx, |diff, cx| diff.set_notes(notes, cx));
    }

    /// Follows the session: the box, the field, then the rows, in the order
    /// that lets an open jump field win the keys over an open comment box
    /// (REVIEW-17).
    pub fn follow(&mut self, page: &ReviewPage, window: &mut Window, cx: &mut Context<Self>) {
        self.comment.update(cx, |comment, cx| comment.follow(page.comment.as_ref(), window, cx));
        self.files.update(cx, |files, cx| files.follow(page, window, cx));
        let slot = page.comment.as_ref().map(|c| Slot { view: self.comment.clone().into(), height: comment::height(c, zoom(window.rem_size())) });
        let ticks = page.staging.as_ref().map(|s| Ticks { staged: s.staged, busy: s.busy });
        self.diff.update(cx, |diff, cx| {
            diff.set_ticks(ticks, cx);
            diff.show(&page.rows, page.split, slot, cx);
        });
        self.place_notes(page, window, cx);
        self.submit.update(cx, |submit, cx| submit.follow(page.drafts.as_ref(), window, cx));
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
            .when_some(page.drafts.as_ref(), |head, drafts| {
                let pending = drafts.drafts.len();
                head.child(
                    h_flex()
                        .id("review-open-submit")
                        .test_support()
                        .aria_label(SharedString::from(format!("Review, {pending} pending")))
                        .flex_shrink_0()
                        .gap(rem(tokens.sp_6))
                        .items_center()
                        .h(rem(tokens.control_h_sm))
                        .px(rem(tokens.sp_8))
                        .rounded(rem(tokens.r_sm))
                        .border_1()
                        .border_color(if pending > 0 { tokens.accent.opacity(0.6) } else { tokens.line_strong })
                        .typed(tokens.type_small)
                        .text_color(tokens.fg)
                        .cursor_pointer()
                        .hover(|button| button.bg(tokens.fill))
                        .child("Review")
                        .child(
                            div()
                                .px(rem(tokens.sp_6))
                                .rounded(rem(tokens.r_pill))
                                .bg(if pending > 0 { tokens.accent } else { tokens.fill })
                                .text_color(if pending > 0 { tokens.fg_on_accent } else { tokens.fg_muted })
                                .text_size(rem(tokens.fs_xs))
                                .child(SharedString::from(pending.to_string())),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.submit.update(cx, |submit, cx| submit.toggle(cx)))),
                )
            })
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
        let diff = v_flex()
            .id("review-diff-pane")
            .relative()
            .size_full()
            .min_w_0()
            .child(self.head(&page, &tokens, cx))
            .child(div().flex_1().min_h_0().child(self.body(&page, &tokens)))
            .child(self.submit.clone());
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
