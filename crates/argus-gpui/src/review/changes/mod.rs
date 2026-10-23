//! The Changes pane, as the prototype draws it (`claude.ai/artifact/TV5HXHL9pijRnTzoUgCsxq`):
//! the review of a checkout's working changes, or of one commit, with its list column (`side`)
//! beside the diff of one file under its head (`head`). The same view is the panel's body
//! wherever the panel is — on the stage in place of the pane, docked, floating, or in a cell
//! beside an agent's pane — and it lays itself out by its own width (`narrow`): below 760px the
//! files and the diff take turns. It draws no header of its own; the dock, the dialog, the cell
//! or the Git stage names it.
//!
//! Everything it draws is the session's `ReviewPage`: the files, the file on screen, the rows
//! with the cursor and the pick, the comment box, the jump field's matches, and why there is no
//! diff when there is none, with the change ruler down the diff's edge (`kit::ruler`), a mark for
//! each run of changed rows that scrolls the diff to it. A press sends its call and changes nothing on screen; the review
//! changes when the next `Review` event says so, so a refused click leaves the diff as the
//! session holds it. What a body shows is chosen in the page's order (SHARED-3): loading bars,
//! then the muted reason there is no diff, then the rows. The split between the files and the
//! diff, the find field and the list's shape are the window's own, never sent anywhere.
//!
//! A review of working changes stages: each changed line has a box, ticked when the diff is the
//! index's, and each hunk a button (`kit::diff::tick`). A tick, a Shift-tick from the last one,
//! or a hunk is a `Stage` call, and the boxes move when the diff read after the index moved says
//! so. The review's drafts hang under their lines (`draft_card`), and the Review button over the
//! diff opens the panel that sends them (`submit`).

pub mod commit;
pub mod head;
pub mod lists;
pub mod narrow;
pub mod side;
pub mod tree;

use std::collections::HashMap;
use std::rc::Rc;

use argus_desktop_core::connection::page::{Call, ReviewClick, ReviewPage, ReviewReading};
use argus_desktop_core::review::staging::StageGesture;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::TestSupportExt as _;
use gpui_kit::{div, prelude::*, Context, Entity, SharedString, Subscription, Window};

use crate::connection::caller::Caller;
use crate::connection::heard::Heard;
use crate::kit::diff::gutter::{shown, GUTTER};
use crate::kit::diff::tick::{tickable, Ticks, BOX};
use crate::kit::diff::{DiffEvent, DiffView, Slot};
use crate::review::comment::{self, ways, CommentBox};
use crate::review::draft_card::{self, DraftCard};
use crate::review::changes::side::Files;
use crate::review::changes::narrow::Narrow;
use crate::review::submit::Submit;
use crate::kit::ghosts::ghosts;
use crate::kit::ruler::{Ids, Mark, Ruler, Track};
use argus_desktop_core::code::ruler::{run_title, runs};
use crate::kit::hints;
use crate::kit::split::{self, Split};
use crate::style::Typed as _;
use crate::theme::scale::{rem, zoom};
use crate::theme::Tokens;

const RULER: Ids = Ids { ruler: "review-ruler", band: "review-ruler-view", mark: "review-mark", label: "Where this file changed" };

/// The width the list column opens at (`--side`), and how far the person may drag it.
const FILES: f32 = 340.0;
const FILES_MIN: f32 = 240.0;
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
    /// The diff's scrollbar, with a mark for each run of its changed rows.
    ruler: Entity<Ruler>,
    /// A file with no change, read whole from the Files tab in place of the diff.
    whole: Entity<DiffView>,
    comment: Entity<CommentBox>,
    split: Entity<Split>,
    submit: Entity<Submit>,
    /// Each draft on screen's card, by its id, and what hears the card grow as it is edited.
    cards: HashMap<u64, (Entity<DraftCard>, Subscription)>,
    /// The row of the last box ticked, which a Shift-tick ranges from.
    last_tick: Option<usize>,
    narrow: Narrow,
    /// The pane's own focus: beside the work it has the keys while the pointer last pressed in it.
    pub focus: gpui_kit::FocusHandle,
    _subscriptions: [Subscription; 3],
}

impl Changes {
    /// The diff on screen, for the stage's sideways keys.
    pub fn diff(&self) -> &Entity<DiffView> {
        &self.diff
    }

    pub fn new(heard: Entity<Heard>, caller: Caller, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let files = cx.new(|cx| Files::new(heard.clone(), caller.clone(), window, cx));
        let comment = cx.new(|cx| CommentBox::new(heard.clone(), caller.clone(), Rc::new(ways::Review), window, cx));
        let diff = cx.new(|cx| DiffView::new("review-diff", true, true, cx).ruled());
        let ruler = cx.new(|cx| {
            let track = diff.clone();
            // A mark's run, a third of the way down, as a step to a change puts it.
            let pressed = Rc::new(move |found: &Mark, cx: &mut gpui_kit::App| {
                if let Some((top, _)) = track.span(found.first, found.first, cx) {
                    let (_, viewport, _) = track.measures(cx);
                    track.scroll_to((top - viewport / 3.0).max(0.0), cx);
                }
            });
            Ruler::new(RULER, Rc::new(diff.clone()), pressed, cx)
        });
        let tokens = *Tokens::global(cx);
        let whole = cx.new(|cx| DiffView::new("review-whole", true, false, cx).reading(&tokens));
        let split = cx.new(|cx| Split::new("review-split", "Resize the files", FILES, FILES_MIN, FILES_MAX, cx));
        let submit = cx.new(|cx| Submit::new(heard.clone(), caller.clone(), window, cx));
        let resized = cx.observe(&split, |_, _, cx| cx.notify());
        let opened = cx.subscribe(&files, |this, _, _: &side::Opened, cx| {
            this.narrow.opened();
            cx.notify();
        });
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
        Self { heard, caller, files, diff, ruler, whole, comment, split, submit, cards: HashMap::new(), last_tick: None, narrow: Narrow::new(), focus: cx.focus_handle(), _subscriptions: [pressed, resized, opened] }
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
        self.narrow.follow(page);
        self.comment.update(cx, |comment, cx| comment.follow(page.comment.as_ref(), window, cx));
        self.files.update(cx, |files, cx| files.follow(page, window, cx));
        let slot = page.comment.as_ref().map(|c| Slot { view: self.comment.clone().into(), height: comment::height(c, zoom(window.rem_size())) });
        let ticks = page.staging.as_ref().map(|s| Ticks { staged: s.staged, busy: s.busy });
        self.diff.update(cx, |diff, cx| {
            diff.set_ticks(ticks, cx);
            diff.show(&page.rows, page.split, slot, cx);
        });
        self.place_notes(page, window, cx);
        let marks = runs(&page.rows)
            .into_iter()
            .map(|run| {
                let title = SharedString::from(run_title(run.kind));
                Mark { first: run.first, last: run.last, kind: run.kind, label: title.clone(), title }
            })
            .collect();
        let track: Rc<dyn Track> = Rc::new(self.diff.clone());
        self.ruler.update(cx, |ruler, cx| ruler.show(marks, track, cx));
        let whole = page.tree.as_ref().and_then(|tree| tree.reading.as_ref()).map(|reading| reading.rows.as_slice()).unwrap_or_default();
        self.whole.update(cx, |view, cx| view.show(whole, false, None, cx));
        self.submit.update(cx, |submit, cx| submit.follow(page.drafts.as_ref(), window, cx));
    }

    /// `j` or `k` to the Files tab while it is shown; whether it took the key.
    pub fn step_files(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        self.files.update(cx, |files, cx| files.step(forward, cx))
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

    /// A file read whole: loading bars, or why it has no lines, or its lines.
    fn reading_body(&self, reading: &ReviewReading, tokens: &Tokens) -> gpui_kit::AnyElement {
        if reading.loading {
            return div().size_full().bg(tokens.bg_sunken).child(ghosts("review-loading", &BARS, tokens)).into_any_element();
        }
        if let Some(why) = &reading.note {
            return said(why, tokens);
        }
        div().id("review-whole-rows").test_support().size_full().min_h_0().bg(tokens.bg_sunken).child(self.whole.clone()).into_any_element()
    }

    /// Loading bars, or why there is no diff, or the diff.
    fn body(&self, page: &ReviewPage, tokens: &Tokens) -> gpui_kit::AnyElement {
        if page.loading {
            return div().size_full().bg(tokens.bg_sunken).child(ghosts("review-loading", &BARS, tokens)).into_any_element();
        }
        if let Some(why) = &page.empty {
            return said(why, tokens);
        }
        h_flex()
            .id("review-rows")
            .test_support()
            .size_full()
            .min_h_0()
            .child(div().flex_1().min_w_0().h_full().child(self.diff.clone()))
            .when(!page.rows.is_empty(), |rows| rows.child(self.ruler.clone()))
            .into_any_element()
    }
}

/// `.review-empty`: why there is nothing to read, 24px about a line in the body type.
fn said(why: &str, tokens: &Tokens) -> gpui_kit::AnyElement {
    div()
        .id("review-empty")
        .test_support()
        .aria_label(SharedString::from(why.to_owned()))
        .size_full()
        .p(rem(tokens.sp_24))
        .bg(tokens.bg_sunken)
        .typed(tokens.type_body)
        .text_color(tokens.fg_muted)
        .child(SharedString::from(why.to_owned()))
        .into_any_element()
}

/// Whether the review the session holds is split, for the pill that is already on.
fn page_split(heard: &Entity<Heard>, cx: &Context<Changes>) -> bool {
    heard.read(cx).review.as_ref().is_some_and(|page| page.split)
}

impl Render for Changes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = *Tokens::global(cx);
        let Some(page) = self.heard.read(cx).review.clone() else {
            return div().into_any_element();
        };
        let narrow = self.narrow.narrow(window.rem_size());
        // The diff's own width: the pane's, less the list column's beside it.
        let across = self.narrow.width(window.rem_size()) - if narrow { 0.0 } else { self.split.read(cx).width() };
        let fold = across > 0.0 && across < head::TOOLS_FOLD;
        let reading = page.tree.as_ref().and_then(|tree| tree.reading.clone());
        let (head, body) = match &reading {
            Some(reading) => (self.reading_head(reading, narrow, &tokens, cx).into_any_element(), self.reading_body(reading, &tokens)),
            None => (self.head(&page, narrow, fold, &tokens, cx).into_any_element(), self.body(&page, &tokens)),
        };
        let diff = v_flex().id("review-diff-pane").relative().size_full().min_w_0().child(head).child(div().flex_1().min_h_0().child(body)).child(self.submit.clone());
        // Narrow, the files and the diff take turns; wide, the files stand beside the diff.
        let body = if !narrow {
            h_flex()
                .flex_1()
                .min_h_0()
                .child(split::side(&self.split, "review-column", self.files.clone(), &tokens, cx))
                .child(div().flex_1().min_w_0().h_full().child(diff))
        } else if self.narrow.listing() {
            h_flex().flex_1().min_h_0().child(div().size_full().child(self.files.clone()))
        } else {
            h_flex().flex_1().min_h_0().child(diff)
        };
        v_flex()
            .id("review")
            .test_support()
            .track_focus(&self.focus)
            // A press anywhere in it gives it the keys, unless they are already in it; on the way
            // down, so a part of it that takes them for itself (a field, the sash) still does.
            .capture_any_mouse_down(cx.listener(|this, _, window, cx| {
                if window.focused(cx).is_none_or(|held| !this.focus.contains(&held, window)) {
                    window.focus(&this.focus, cx);
                }
            }))
            .relative()
            .size_full()
            .child(self.narrow.measure())
            .child(body)
            .when(!narrow, |pane| pane.child(hints::bar("review-hints", &HINTS, &tokens)))
            .into_any_element()
    }
}
