//! A diff drawn: file's rows in a virtual list, so only what is on screen is
//! built however long the diff is.
//!
//! The rows are `argus-desktop-core`'s `DiffRow`s, unified or split, and what
//! each one is (a hunk, a fold, a line, a pair; the cursor, the pick; what the
//! highlighter cut) is already said in them (`lines` makes the data of each).
//! This draws them and reports what was pressed: a click on a line, with
//! Shift held or not, and a fold's buttons. It sends no call and changes no
//! row, so a refused click leaves the diff as the session says it is.
//!
//! One diff view serves review and history, and the patch block and the Code
//! stage after them, so it knows none of their calls: a surface that does not
//! take clicks asks for a view with none. A surface may put one thing among
//! the rows, a slot of a height it names, under a row: the review's comment
//! box. The scroll offset stays across every update and the cursor's row is
//! brought into view when the rows change, "nearest" (SHARED-5): no movement
//! while it is already visible.
//!
//! A long line is never cut, as in the page. Unified, every row is as wide as
//! the longest line (or the pane, when that is wider) so the list scrolls
//! sideways and each row's colour runs on past the edge; the list measures
//! one item for its content width, so each row carries that width itself.
//! Split, each half wraps at the cell and the list is told each row's height
//! up front, which is why the wrapping is done here (`lines::wrap`) rather
//! than left to the text system, and why the heights are recomputed when the
//! pane's width changes: a canvas reports it, the render reads it a frame
//! later.
//!
//! A press on a line's code and a drag past the 5 px of `pointer::drag` selects
//! its text, as a browser's selection does: the numbers and the marker are
//! never in it, a split pair selects within the half the press began in, and
//! the selection is the window's own (a row index and a byte of its drawn
//! text at each end), gone when the rows change. The columns are the text
//! layouts the rows were drawn with, kept for the frame, so a position is
//! read from what is on screen. The owner copies it (`copy`); a view that
//! `picks` also tells it a drag that crossed rows, for it to send what a
//! click then a shift-click would. A press that never goes 5 px stays the
//! row's click. Past the top or bottom edge the list scrolls with each move
//! of the pointer; it does not keep scrolling while the pointer is still.
//!
//! Both scrollbars always show, as the page's native box does; a held middle
//! button drags the diff, and the stage's Left and Right nudge it (`nudge`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use argus_desktop_core::connection::page::{DiffRow, DiffSide};
use gpui_kit::component::{h_flex, ActiveTheme as _};
use gpui_kit::{
    canvas, div, prelude::*, px, AnyElement, AnyView, App, Bounds, ClickEvent, ClipboardItem, Context, ElementId, EventEmitter,
    FocusHandle, Font, FontStyle, HighlightStyle, Hsla, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollStrategy, SharedString, Size, Styled, StyledText, TestSupportExt as _, TextLayout, Window, point,
};
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::component::{v_virtual_list, VirtualListScrollHandle};
use gpui_kit::size;

use crate::diff::lines::{self, Code, Item, Shape};
use crate::pointer::drag::{self, Drag, Dragger, Step};
use crate::theme::Tokens;

/// The width of a line-number column, and of the marker's.
const NUMBER: f32 = 44.0;
const MARKER: f32 = 16.0;
/// What a comment box's left edge clears: both number columns and the marker (`.comment-card`'s 104px).
pub const GUTTER: f32 = NUMBER * 2.0 + MARKER;
/// Room after the last cell of a line, so its end is not flush with the edge
/// of its colour.
const TAIL: f32 = 8.0;
/// A hunk header's or note's side padding, both sides.
const HEADING_PAD: f32 = 24.0;
/// What a row's own border takes of its width, both sides.
const ROW_BORDER: f32 = 2.0;

/// What a press on the diff means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffEvent {
    /// A line, pair or fold; `extend` is Shift held.
    Row { row: usize, extend: bool },
    /// One of a fold's buttons: a step of its lines, or all of them.
    Fold { row: usize, all: bool },
    /// A drag that began on one row and ended on another, in a view that
    /// picks: the rows it began and ended on, for a click then a shift-click.
    Range { first: usize, last: usize },
}

/// A place in the text: a row (an index of the rows, not its number), the
/// half of it in a split pair, and a byte of that half's drawn text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pos {
    row: usize,
    half: usize,
    at: usize,
}

/// What a press of a line's code holds: which, and where the pointer went down.
#[derive(Clone)]
pub struct Held {
    row: usize,
    half: usize,
    from: Point<Pixels>,
}

/// A row and the y of its edge nearest the pointer.
type Edge = (usize, Pixels);

/// The text layout of each code on screen, by row and half, as the last build left them.
type Layouts = Rc<RefCell<HashMap<(usize, usize), TextLayout>>>;

/// What a code needs to be selectable: which it is, what of it is selected
/// (in the text as drawn, wrapped), and where its layout is kept.
struct Ink {
    row: usize,
    half: usize,
    selected: Option<Range<usize>>,
    selection: Hsla,
    layouts: Layouts,
}

/// A thing drawn under a row, among the rows.
#[derive(Clone)]
pub struct Slot {
    pub view: AnyView,
    pub height: Pixels,
}

pub struct DiffView {
    id: SharedString,
    interactive: bool,
    /// Whether a drag across rows is told to the owner as a range to pick.
    picks: bool,
    rows: Rc<Vec<DiffRow>>,
    split: bool,
    slot: Option<Slot>,
    items: Rc<Vec<Item>>,
    sizes: Rc<Vec<Size<Pixels>>>,
    scroll: VirtualListScrollHandle,
    built: Cell<usize>,
    /// Each row's left and right side as drawn (tabs stopped), made once per
    /// rows rather than per frame.
    drawn: Vec<[Code; 2]>,
    /// Each row's height: a pair's grows with the lines its longer half wraps to.
    heights: Vec<f32>,
    /// The cells of the widest line, and of the widest hunk header or note.
    widest: (usize, usize),
    /// The list's width as the last frame laid it out, and the advance of one
    /// mono cell in the diff's font, once rendered.
    pane: Rc<Cell<Pixels>>,
    advance: Pixels,
    /// The cells a split half's code is wrapped at when `heights` was made.
    per_line: usize,
    /// Where a middle-button pan was pressed and the scroll offset then.
    panning: Option<(Point<Pixels>, Point<Pixels>)>,
    drag: Drag<Held>,
    keys: FocusHandle,
    /// Where the selection began and where it is now, either way round.
    selection: Option<(Pos, Pos)>,
    layouts: Layouts,
    /// Where the list was laid out last, for the edges a drag scrolls at.
    area: Rc<Cell<Bounds<Pixels>>>,
}

#[cfg(test)]
thread_local! {
    /// What each diff view by id has built, for a test that cannot reach the
    /// entity inside a whole window.
    static BUILT: std::cell::RefCell<std::collections::HashMap<String, usize>> = Default::default();
}

/// How many items the diff view named `id` has built on this thread, over all
/// its lives, as [`DiffView::built`] counts them.
#[cfg(test)]
pub(crate) fn built_by(id: &str) -> usize {
    BUILT.with(|built| built.borrow().get(id).copied().unwrap_or(0))
}

impl EventEmitter<DiffEvent> for DiffView {}

impl DiffView {
    /// A view named `id`, which takes clicks when `interactive`, and tells a
    /// drag across rows when `picks`. Its text can always be selected.
    pub fn new(id: impl Into<SharedString>, interactive: bool, picks: bool, cx: &mut Context<Self>) -> Self {
        Self {
            id: id.into(),
            interactive,
            picks,
            rows: Rc::default(),
            split: false,
            slot: None,
            items: Rc::default(),
            sizes: Rc::default(),
            scroll: VirtualListScrollHandle::new(),
            built: Cell::new(0),
            drawn: Vec::new(),
            heights: Vec::new(),
            widest: (0, 0),
            pane: Rc::default(),
            advance: px(0.0),
            per_line: 0,
            panning: None,
            drag: Drag::new(),
            keys: cx.focus_handle(),
            selection: None,
            layouts: Rc::default(),
            area: Rc::default(),
        }
    }

    /// Draws `rows`, as split pairs or unified lines, with `slot` under the
    /// row it belongs to. Nothing changes, and nothing is notified, when it is
    /// what was drawn already: the session says the same thing again whenever
    /// anything else about the window changes.
    pub fn show(&mut self, rows: &[DiffRow], split: bool, slot: Option<Slot>, cx: &mut Context<Self>) {
        let same_rows = self.rows.as_slice() == rows;
        let same_slot = match (&self.slot, &slot) {
            (None, None) => true,
            (Some(a), Some(b)) => a.view.entity_id() == b.view.entity_id() && a.height == b.height,
            _ => false,
        };
        if same_rows && same_slot && self.split == split {
            return;
        }
        if !same_rows {
            self.rows = Rc::new(rows.to_vec());
            self.selection = None;
            self.draw();
        }
        self.split = split;
        self.slot = slot;
        self.items = Rc::new(lines::items(&self.rows, self.slot.is_some()));
        self.per_line = self.wrap_at();
        self.size();
        if !same_rows {
            if let Some(at) = lines::cursor_item(&self.rows, &self.items) {
                self.scroll.scroll_to_item(at, ScrollStrategy::Top);
            }
        }
        cx.notify();
    }

    /// The sides of every row as drawn, and how wide the widest are.
    fn draw(&mut self) {
        self.drawn = self
            .rows
            .iter()
            .map(|row| {
                let code = |side: &Option<DiffSide>| side.as_ref().map(lines::code).unwrap_or_default();
                [code(&row.left), code(&row.right)]
            })
            .collect();
        let mut widest = (0, 0);
        for (row, drawn) in self.rows.iter().zip(&self.drawn) {
            match lines::shape(row) {
                Shape::Hunk | Shape::Note => widest.1 = widest.1.max(lines::cells(&row.text)),
                Shape::Line => widest.0 = widest.0.max(lines::cells(&drawn[0].text)),
                Shape::Fold | Shape::Pair => {}
            }
        }
        self.widest = widest;
    }

    /// The cells a split half's code wraps at in this pane: 0, no wrapping,
    /// until the pane and the font have been measured and for a unified diff.
    fn wrap_at(&self) -> usize {
        let (pane, advance) = (f32::from(self.pane.get()), f32::from(self.advance));
        if !self.split || pane <= 0.0 || advance <= 0.0 {
            return 0;
        }
        // Row border both sides, the second half's left rule, and a few
        // pixels of slack against a rounding that would push a full line over.
        let half = (pane - ROW_BORDER - 1.0) / 2.0;
        (((half - NUMBER - MARKER - TAIL) / advance).floor() as usize).max(1)
    }

    /// `heights` and `sizes` for the rows at `per_line`.
    fn size(&mut self) {
        self.heights = self
            .rows
            .iter()
            .zip(&self.drawn)
            .map(|(row, drawn)| match lines::shape(row) {
                Shape::Pair => {
                    let most = drawn.iter().map(|side| lines::wrapped_lines(&side.text, self.per_line)).max().unwrap_or(1);
                    lines::pair_height(most)
                }
                _ => lines::height(row),
            })
            .collect();
        let slot_height = self.slot.as_ref().map_or(px(0.0), |slot| slot.height);
        self.sizes = Rc::new(
            self.items
                .iter()
                .map(|item| match item {
                    Item::Row(i) => size(px(0.0), px(self.heights[*i])),
                    Item::Comment => size(px(0.0), slot_height),
                })
                .collect(),
        );
    }

    /// A row's width: unified, the longest line's or the pane's if wider, so
    /// the rows share the width the list measures; split, the pane's. `None`
    /// before the pane is known, when a row takes what it is given.
    fn width(&self) -> Option<Pixels> {
        let pane = self.pane.get();
        if self.split {
            return (pane > px(0.0)).then_some(pane);
        }
        let advance = f32::from(self.advance);
        let line = NUMBER * 2.0 + MARKER + self.widest.0 as f32 * advance + TAIL + ROW_BORDER;
        let heading = HEADING_PAD + self.widest.1 as f32 * advance;
        Some(pane.max(px(line.max(heading).ceil())))
    }

    fn wide<E: Styled>(&self, element: E) -> E {
        match self.width() {
            Some(width) => element.w(width),
            None => element.w_full(),
        }
    }

    /// How many items the list holds: its rows and the slot.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// How many items were ever built for the list: a long diff builds the few on screen.
    pub fn built(&self) -> usize {
        self.built.get()
    }

    pub fn scroll(&self) -> &VirtualListScrollHandle {
        &self.scroll
    }

    /// Scrolls sideways by `cells` (left is negative), for the arrow keys.
    pub fn nudge(&mut self, cells: f32, cx: &mut Context<Self>) {
        let step = self.advance.max(px(1.0)) * cells;
        let offset = self.scroll.offset();
        self.place(point(offset.x - step, offset.y), cx);
    }

    /// Puts the scroll at `offset`, held to what the content has to show.
    fn place(&mut self, offset: Point<Pixels>, cx: &mut Context<Self>) {
        let most = self.scroll.max_offset();
        self.scroll.set_offset(point(offset.x.clamp(-most.x, px(0.0)), offset.y.clamp(-most.y, px(0.0))));
        cx.notify();
    }

    fn row_id(ix: usize) -> ElementId {
        ("diff-row", ix).into()
    }

    /// One row, `ix` of the rows.
    fn row(&self, ix: usize, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.rows[ix];
        let base = match lines::shape(row) {
            Shape::Hunk | Shape::Note => return self.heading(ix, row, tokens),
            Shape::Fold => return self.fold(ix, row, tokens, cx),
            Shape::Line => self.line(ix, row, tokens, cx),
            Shape::Pair => self.pair(ix, row, tokens, cx),
        };
        let (number, interactive) = (row.row, self.interactive);
        self.wide(base)
            .id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(px(self.heights[ix]))
            .border_1()
            .border_color(if row.cursor || row.picked { tokens.line_strong } else { gpui_kit::transparent_black() })
            .when(interactive, |row| {
                row.cursor_text().on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                    cx.emit(DiffEvent::Row { row: number, extend: event.modifiers().shift });
                }))
            })
            .into_any_element()
    }

    /// A hunk's `@@` line or a note: said, never picked.
    fn heading(&self, ix: usize, row: &DiffRow, tokens: &Tokens) -> AnyElement {
        self.wide(h_flex())
            .id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(px(lines::HUNK))
            .px(px(tokens.sp_12))
            .items_center()
            .bg(tokens.bg_chrome)
            .text_color(tokens.fg_faint)
            .overflow_hidden()
            .whitespace_nowrap()
            .child(SharedString::from(row.text.clone()))
            .into_any_element()
    }

    /// The lines between hunks the diff did not carry, and the buttons that
    /// ask for them. The cursor may rest on one, so it takes a click too.
    fn fold(&self, ix: usize, row: &DiffRow, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let number = row.row;
        let button = |id: &'static str, label: String, all: bool, cx: &mut Context<Self>| {
            div()
                .id((id, ix))
                .test_support()
                .aria_label(SharedString::from(label.clone()))
                // `.fbtn`: the reference colour in the interface face, lifting to `--fill`.
                .px(px(tokens.sp_4))
                .h(px(tokens.fold_btn_h))
                .flex()
                .items_center()
                .rounded(px(tokens.r_xs))
                .font_family(SharedString::from(tokens.font_ui))
                .text_size(px(tokens.fs_sm))
                .text_color(tokens.ref_)
                .cursor_pointer()
                .hover({
                    let fill = tokens.fill;
                    move |style| style.bg(fill)
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    // The row under it would take the click as a move first.
                    cx.stop_propagation();
                    cx.emit(DiffEvent::Fold { row: number, all });
                }))
                .child(SharedString::from(label))
        };
        self.wide(h_flex())
            .id(Self::row_id(ix))
            .test_support()
            .flex_shrink_0()
            .h(px(lines::FOLD))
            .px(px(tokens.sp_12))
            .gap(px(tokens.sp_8))
            .items_center()
            .bg(tokens.bg_chrome)
            .text_color(tokens.fg_muted)
            .font_family(SharedString::from(tokens.font_ui))
            .text_size(px(tokens.fs_sm))
            .border_t_1()
            .border_b_1()
            .border_dashed()
            .border_color(if row.cursor { tokens.line_strong } else { tokens.line })
            .child(SharedString::from(format!("\u{22ef} {}", row.text)))
            .when(self.interactive, |fold| {
                fold.children(match row.step {
                    Some(step) => vec![
                        button("diff-fold", format!("Show {step}"), false, cx),
                        button("diff-fold-all", "Show all".to_owned(), true, cx),
                    ],
                    None => vec![button("diff-fold", "Show".to_owned(), true, cx)],
                })
            })
            .when(self.interactive, |fold| fold.cursor_text())
            .when(self.interactive, |fold| {
                fold.on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                    cx.emit(DiffEvent::Row { row: number, extend: event.modifiers().shift });
                }))
            })
            .into_any_element()
    }

    /// A unified line: two numbers, a marker, then code.
    fn line(&self, ix: usize, row: &DiffRow, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::Div {
        let side = row.left.as_ref();
        let kind = side.map_or("context", |side| side.kind);
        let fill = lines::fill(kind, row.picked, row.cursor, tokens);
        // The gutter stays at the pane's left edge while the code scrolls
        // sideways under it: the row has moved by the offset, the gutter moves back.
        let pin = Pin { shift: -self.scroll.offset().x, fill };
        let down = cx.listener(move |this, event: &MouseDownEvent, _, _| this.press(ix, 0, event.position));
        h_flex()
            .items_center()
            .when_some(fill, |row, fill| row.bg(fill))
            .child(half(side, &self.drawn[ix][0], true, tokens, self.ink(ix, 0, &self.drawn[ix][0].text, cx), down, Some(pin)))
            .overflow_hidden()
    }

    /// A split pair: its two halves, each with its own number, marker and
    /// code, cut at its own half. A missing half is blank and recessed.
    fn pair(&self, ix: usize, row: &DiffRow, tokens: &Tokens, cx: &mut Context<Self>) -> gpui_kit::Div {
        let side = |side: Option<&DiffSide>, second: bool| {
            let fill = match side {
                Some(side) => lines::fill(side.kind, row.picked, row.cursor, tokens),
                None => Some(tokens.bg_chrome),
            };
            h_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .items_center()
                .overflow_hidden()
                .when(second, |half| half.border_l_1().border_color(tokens.line))
                .when_some(fill, |half, fill| half.bg(fill))
                .child({
                    let wrapped = lines::wrap(&self.drawn[ix][usize::from(second)], self.per_line);
                    let ink = self.ink(ix, usize::from(second), &wrapped.text, cx);
                    half(side, &wrapped, false, tokens, ink, cx.listener(move |this, event: &MouseDownEvent, _, _| this.press(ix, usize::from(second), event.position)), None)
                })
        };
        h_flex()
            .items_center()
            .when(self.heights[ix] > lines::LINE, |pair| pair.line_height(px(lines::LINE)))
            .child(side(row.left.as_ref(), false))
            .child(side(row.right.as_ref(), true))
    }

    fn item(&self, ix: usize, tokens: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        self.built.set(self.built.get() + 1);
        #[cfg(test)]
        BUILT.with(|built| *built.borrow_mut().entry(self.id.to_string()).or_default() += 1);
        match self.items.get(ix) {
            Some(Item::Row(row)) => self.row(*row, tokens, cx),
            Some(Item::Comment) => match &self.slot {
                Some(slot) => div()
                    .id(("diff-slot", 0usize))
                    .test_support()
                    .flex_shrink_0()
                    .w_full()
                    .h(slot.height)
                    .child(slot.view.clone())
                    .into_any_element(),
                None => div().into_any_element(),
            },
            None => div().into_any_element(),
        }
    }
}

impl DiffView {
    /// The press of a code: the drag helper follows it from here.
    fn press(&mut self, row: usize, half: usize, from: Point<Pixels>) {
        self.drag.press(from, Held { row, half, from });
    }

    /// What `code` needs of row `row`'s `half`, whose text is drawn as `shown`.
    fn ink(&self, row: usize, half: usize, shown: &str, cx: &Context<Self>) -> Ink {
        let selected = self.selected(row, half).and_then(|range| {
            let (start, end) = (lines::wrapped_index(shown, range.start, true), lines::wrapped_index(shown, range.end, false));
            (start < end).then_some(start..end)
        });
        Ink { row, half, selected, selection: cx.theme().selection, layouts: self.layouts.clone() }
    }

    fn ordered(&self) -> Option<(Pos, Pos)> {
        let (a, b) = self.selection?;
        Some(if (a.row, a.at) <= (b.row, b.at) { (a, b) } else { (b, a) })
    }

    /// The bytes of row `row`'s `half` that are selected, in its text before any wrapping.
    fn selected(&self, row: usize, half: usize) -> Option<Range<usize>> {
        let (a, b) = self.ordered()?;
        if row < a.row || row > b.row || half != a.half {
            return None;
        }
        let len = self.drawn.get(row)?[half].text.len();
        let range = (if row == a.row { a.at } else { 0 })..(if row == b.row { b.at } else { len });
        (range.start < range.end).then_some(range)
    }

    /// The selected text, a line to a row, or `None` when nothing is selected.
    fn selected_text(&self) -> Option<String> {
        let (a, b) = self.ordered()?;
        let mut out = Vec::new();
        for row in a.row..=b.row {
            let Some(drawn) = self.drawn.get(row) else { break };
            let present = self.rows.get(row).is_some_and(|row| if a.half == 0 { row.left.is_some() } else { row.right.is_some() });
            if !present || !matches!(lines::shape(&self.rows[row]), Shape::Line | Shape::Pair) {
                continue;
            }
            let text = &drawn[a.half].text;
            match self.selected(row, a.half) {
                Some(range) => out.push(text[range].to_owned()),
                None if row > a.row && row < b.row => out.push(String::new()),
                None => {}
            }
        }
        (!out.is_empty()).then(|| out.join("
"))
    }

    /// Puts the selected text on the clipboard; whether there was any.
    pub fn copy(&self, cx: &mut App) -> bool {
        let Some(text) = self.selected_text() else { return false };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    /// The place at `index` of a layout, in the text before any wrapping.
    fn spot(&self, row: usize, half: usize, index: usize) -> Pos {
        let shown = lines::wrap(&self.drawn[row][half], self.per_line);
        Pos { row, half, at: lines::unwrapped(&shown.text, index.min(shown.text.len())) }
    }

    /// The place in row `row`'s `half` under the pointer at `at`, if it is drawn.
    fn spot_in(&self, row: usize, half: usize, at: Point<Pixels>) -> Option<Pos> {
        self.drawn.get(row)?;
        let layouts = self.layouts.borrow();
        let layout = layouts.get(&(row, half))?;
        let (Ok(index) | Err(index)) = layout.index_for_position(at);
        Some(self.spot(row, half, index))
    }

    /// The place in `half` the pointer at `at` is over: in the row under it,
    /// or past the end of the rows above it or the start of those below, so a
    /// drag out of the text or onto a heading still extends the selection.
    fn spot_near(&self, half: usize, at: Point<Pixels>, anchor: Pos) -> Option<Pos> {
        let layouts = self.layouts.borrow();
        let (mut above, mut below): (Option<Edge>, Option<Edge>) = (None, None);
        for (&(row, shown), layout) in layouts.iter() {
            if shown != half || row >= self.drawn.len() {
                continue;
            }
            let bounds = layout.bounds();
            if at.y >= bounds.top() && at.y < bounds.bottom() {
                let (Ok(index) | Err(index)) = layout.index_for_position(at);
                return Some(self.spot(row, half, index));
            }
            if bounds.bottom() <= at.y {
                if above.is_none_or(|(_, bottom)| bounds.bottom() > bottom) {
                    above = Some((row, bounds.bottom()));
                }
            } else if below.is_none_or(|(_, top)| bounds.top() < top) {
                below = Some((row, bounds.top()));
            }
        }
        let end = |row: usize| Pos { row, half, at: self.drawn[row][half].text.len() };
        match (above, below) {
            (Some((a, _)), Some((b, _))) => Some(if anchor.row <= a { end(a) } else { Pos { row: b, half, at: 0 } }),
            (Some((a, _)), None) => Some(end(a)),
            (None, Some((b, _))) => Some(Pos { row: b, half, at: 0 }),
            (None, None) => None,
        }
    }

    /// Moves the selection's far end to the pointer at `at`, scrolling the
    /// list a step when the pointer is past its top or bottom.
    fn extend(&mut self, half: usize, at: Point<Pixels>) {
        let Some((anchor, _)) = self.selection else { return };
        let area = self.area.get();
        let over = if at.y < area.top() {
            at.y - area.top()
        } else if at.y > area.bottom() {
            at.y - area.bottom()
        } else {
            px(0.0)
        };
        if over != px(0.0) {
            let mut offset = self.scroll.offset();
            let step = over.as_f32().clamp(-40.0, 40.0);
            offset.y = px((offset.y.as_f32() - step).clamp(-self.scroll.max_offset().y.as_f32(), 0.0));
            self.scroll.set_offset(offset);
        }
        if let Some(head) = self.spot_near(half, at, anchor) {
            self.selection = Some((anchor, head));
        }
    }
}

impl Dragger for DiffView {
    type Payload = Held;

    fn drag(&self) -> &Drag<Held> {
        &self.drag
    }

    fn drag_mut(&mut self) -> &mut Drag<Held> {
        &mut self.drag
    }

    fn keys(&self) -> &FocusHandle {
        &self.keys
    }

    fn dragged(&mut self, step: Step<Held>, _: &mut Window, cx: &mut Context<Self>) {
        match step {
            Step::Began(held) => {
                self.selection = self.spot_in(held.row, held.half, held.from).map(|place| (place, place));
                if let Some(at) = self.drag.at() {
                    self.extend(held.half, at);
                }
            }
            Step::Moved(held) => {
                if let Some(at) = self.drag.at() {
                    self.extend(held.half, at);
                }
            }
            Step::Dropped(held, at) => {
                self.extend(held.half, at);
                let Some((anchor, head)) = self.selection else { return };
                if anchor == head {
                    self.selection = None;
                }
                if self.picks && anchor.row != head.row {
                    if let (Some(first), Some(last)) = (self.rows.get(anchor.row), self.rows.get(head.row)) {
                        cx.emit(DiffEvent::Range { first: first.row, last: last.row });
                    }
                }
            }
            Step::Clicked(_) | Step::Cancelled(_) => self.selection = None,
        }
    }
}

/// A unified line's gutter held at the pane's left edge: how far right it
/// moves to undo the sideways scroll, and the row's wash it wears over the
/// diff's background so the code passing under it is hidden.
#[derive(Clone, Copy)]
struct Pin {
    shift: Pixels,
    fill: Option<Hsla>,
}

/// A line's number or numbers, its marker and its code.
fn half(
    side: Option<&DiffSide>,
    drawn: &Code,
    both: bool,
    tokens: &Tokens,
    ink: Ink,
    down: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    pin: Option<Pin>,
) -> impl IntoElement {
    // `.dno`: `--fg-faint` on every line, changed or not.
    let number_colour = tokens.fg_faint;
    let marker_colour = match side.map(|side| side.kind) {
        Some("added") => tokens.ok,
        Some("removed") => tokens.danger,
        _ => tokens.fg_faint,
    };
    let numbers = lines::numbers(side, both).into_iter().map(move |n| {
        div()
            .flex_shrink_0()
            .w(px(NUMBER))
            .pr_2()
            .text_right()
            .text_color(number_colour)
            .child(SharedString::from(n.map_or_else(String::new, |n| n.to_string())))
    });
    let marker = div()
        .flex_shrink_0()
        .w(px(MARKER))
        .text_center()
        .text_color(marker_colour)
        .child(SharedString::from(side.map_or("", |side| side.marker)));
    let code = div()
        .flex_1()
        .min_w_0()
        .whitespace_nowrap()
        .on_mouse_down(MouseButton::Left, down)
        .children(side.map(|_| code(drawn, tokens, ink)));
    let Some(pin) = pin else {
        return h_flex().flex_1().min_w_0().items_center().children(numbers).child(marker).child(code);
    };
    // Painted after the code so it lies over it: a spacer keeps the code's
    // place, and the gutter itself sits absolutely at the pane's edge.
    let width = px(lines::numbers(side, both).len() as f32 * NUMBER + MARKER);
    let gutter = div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(pin.shift)
        .w(width)
        .bg(tokens.bg_sunken)
        .child(h_flex().size_full().items_center().when_some(pin.fill, |gutter, fill| gutter.bg(fill)).children(numbers).child(marker));
    h_flex()
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .items_center()
        .child(div().flex_shrink_0().w(width))
        .child(code)
        .child(gutter)
}

/// The side's text with its highlight kinds as colours.
fn code(drawn: &Code, tokens: &Tokens, ink: Ink) -> impl IntoElement {
    let highlights = lines::runs(drawn, ink.selected.clone()).into_iter().map(|(range, kind, selected)| {
        let tone = kind.and_then(|kind| lines::tone(kind, tokens));
        let style = HighlightStyle {
            color: tone.map(|tone| tone.colour),
            font_style: tone.and_then(|tone| tone.italic.then_some(FontStyle::Italic)),
            background_color: selected.then_some(ink.selection),
            ..HighlightStyle::default()
        };
        (range, style)
    });
    let text = StyledText::new(SharedString::from(drawn.text.clone())).with_highlights(highlights);
    // Kept so a pointer's place can be read back from what is on screen.
    ink.layouts.borrow_mut().insert((ink.row, ink.half), text.layout().clone());
    text
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        // `.dbody`: the mono face at `--fs-sm`, which is smaller than the terminal's.
        let tokens = *Tokens::global(cx);
        let (family, text_size) = (theme.mono_font_family.clone(), px(tokens.fs_sm));
        // One cell is the advance of `M`, as the terminal panes measure it.
        let text = window.text_system();
        let font_id = text.resolve_font(&Font { family: family.clone(), ..Font::default() });
        self.advance = text.advance(font_id, text_size, 'M').map_or(text_size * 0.6, |advance| advance.width);
        if self.wrap_at() != self.per_line {
            self.per_line = self.wrap_at();
            self.size();
        }
        let (pane, area, diff) = (self.pane.clone(), self.area.clone(), cx.entity());
        div()
            .id(SharedString::from(format!("{}-body", self.id)))
            .relative()
            .size_full()
            .min_h_0()
            .bg(tokens.bg_sunken)
            .font_family(family)
            .text_size(text_size)
            .text_color(tokens.fg_muted)
            .child(
                // Where the list ends up is only known once it is laid out; a
                // change asks for one more render that reads it.
                canvas(
                    move |bounds, _, cx| {
                        area.set(bounds);
                        if pane.get() != bounds.size.width {
                            pane.set(bounds.size.width);
                            cx.defer(move |cx| diff.update(cx, |_, cx| cx.notify()));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                v_virtual_list(
                    cx.entity(),
                    SharedString::from(format!("{}-list", self.id)),
                    self.sizes.clone(),
                    move |this, range, _, cx| {
                        // Only what this build draws is on screen to be read.
                        this.layouts.borrow_mut().clear();
                        range.map(|ix| this.item(ix, &tokens, cx)).collect::<Vec<_>>()
                    },
                )
                .track_scroll(&self.scroll),
            )
            // The page's diff scrolls in a native box with both bars; without
            // them a mouse with no sideways wheel cannot reach a long line.
            // Laid over the list, as gpui-kit's table lays its own: a bar in
            // the flow would be given what is left under the list.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(Scrollbar::new(&self.scroll).viewport_from_layout().mode(ScrollbarMode::Always)),
            )
            // A held middle button drags the diff under the pointer, both ways.
            .on_mouse_down(MouseButton::Middle, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                this.panning = Some((event.position, this.scroll.offset()));
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let Some((from, offset)) = this.panning else { return };
                if event.pressed_button != Some(MouseButton::Middle) {
                    this.panning = None;
                    return;
                }
                this.place(offset + (event.position - from), cx);
            }))
            .on_mouse_up(MouseButton::Middle, cx.listener(|this, _: &MouseUpEvent, _, _| this.panning = None))
            .child(
                // What the drag takes the keys with, so Escape reaches it first.
                div()
                    .id(SharedString::from(format!("{}-keys", self.id)))
                    .size_0()
                    .track_focus(&self.keys)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| drag::escape(this, event, window, cx))),
            )
            .child(drag::watch(cx.weak_entity()))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{point, size, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions};

    fn line(row: usize) -> DiffRow {
        let side = DiffSide { old: Some(row as u32), new: Some(row as u32), marker: "", kind: "context", parts: vec![("x".into(), "")] };
        DiffRow { row, kind: "line", text: String::new(), hidden: None, step: None, left: Some(side), right: None, cursor: false, picked: false }
    }

    fn fold(row: usize) -> DiffRow {
        DiffRow { row, kind: "fold", text: "8 lines".into(), hidden: Some(8), step: None, left: None, right: None, cursor: false, picked: false }
    }

    struct Opened {
        handle: gpui_kit::AnyWindowHandle,
        view: Entity<DiffView>,
        pressed: Rc<RefCell<Vec<DiffEvent>>>,
        notified: Rc<Cell<u32>>,
    }

    fn open(cx: &mut TestAppContext, interactive: bool) -> Opened {
        open_picking(cx, interactive, false)
    }

    fn open_picking(cx: &mut TestAppContext, interactive: bool, picks: bool) -> Opened {
        cx.update(crate::window::init);
        let pressed: Rc<RefCell<Vec<DiffEvent>>> = Rc::default();
        let notified: Rc<Cell<u32>> = Rc::default();
        let mut built = None;
        let (handle, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(600.0), px(400.0))))),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    let view = cx.new(|cx| DiffView::new("diff", interactive, picks, cx));
                    let sink = pressed.clone();
                    cx.subscribe(&view, move |_, event: &DiffEvent, _| sink.borrow_mut().push(*event)).detach();
                    let count = notified.clone();
                    cx.observe(&view, move |_, _| count.set(count.get() + 1)).detach();
                    built = Some(view.clone());
                    view
                },
            )
            .expect("open test window")
        });
        Opened { handle, view: built.unwrap(), pressed, notified }
    }

    fn show(cx: &mut TestAppContext, opened: &Opened, rows: &[DiffRow]) {
        cx.update(|cx| opened.view.update(cx, |view, cx| view.show(rows, false, None, cx)));
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }

    #[gpui_kit::test]
    fn a_diff_with_no_clicks_says_its_folds_without_buttons_and_reports_no_press(cx: &mut TestAppContext) {
        let opened = open(cx, false);
        show(cx, &opened, &[line(0), fold(1), line(2)]);
        cx.update_window(opened.handle, |_, window, cx| {
            assert!(window.try_find(("diff-row", 1usize)).is_some(), "the fold is still said");
            window.click(("diff-row", 0usize), cx);
            window.click(("diff-row", 1usize), cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert!(opened.pressed.borrow().is_empty(), "{:?}", opened.pressed.borrow());
        cx.update_window(opened.handle, |_, window, _| {
            assert!(window.try_find(("diff-fold", 1usize)).is_none(), "a view that takes no click offers no button");
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn a_diff_that_takes_clicks_reports_a_line_a_fold_and_a_shifted_line(cx: &mut TestAppContext) {
        let opened = open(cx, true);
        show(cx, &opened, &[line(0), fold(1), line(2)]);
        cx.update_window(opened.handle, |_, window, cx| {
            window.click(("diff-row", 0usize), cx);
            window.click(("diff-fold", 1usize), cx);
            window.click_with_modifiers(("diff-row", 2usize), gpui_kit::Modifiers::shift(), cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(
            *opened.pressed.borrow(),
            [
                DiffEvent::Row { row: 0, extend: false },
                DiffEvent::Fold { row: 1, all: true },
                DiffEvent::Row { row: 2, extend: true },
            ]
        );
    }

    #[gpui_kit::test]
    fn a_long_diff_builds_only_the_rows_the_list_asks_for(cx: &mut TestAppContext) {
        let opened = open(cx, true);
        let rows: Vec<DiffRow> = (0..20_000).map(line).collect();
        show(cx, &opened, &rows);
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        cx.update(|cx| {
            let view = opened.view.read(cx);
            assert_eq!(view.len(), 20_000);
            assert!(view.built() > 0 && view.built() < 300, "built {} items of 20000", view.built());
        });
    }

    #[gpui_kit::test]
    fn showing_what_is_drawn_already_redraws_nothing(cx: &mut TestAppContext) {
        let opened = open(cx, true);
        let rows = [line(0), line(1)];
        show(cx, &opened, &rows);
        let before = opened.notified.get();
        show(cx, &opened, &rows);
        assert_eq!(opened.notified.get(), before, "the same rows said again repainted the list");
        show(cx, &opened, &[line(0)]);
        assert_eq!(opened.notified.get(), before + 1);
        cx.update(|cx| assert_eq!(opened.view.read(cx).len(), 1));
    }

    fn text_line(row: usize, text: &str) -> DiffRow {
        let mut line = line(row);
        line.left.as_mut().unwrap().parts = vec![(text.into(), "")];
        line
    }

    fn pair(row: usize, left: &str, right: &str) -> DiffRow {
        let side = |text: &str, kind: &'static str| DiffSide { old: Some(row as u32), new: Some(row as u32), marker: "", kind, parts: vec![(text.into(), "")] };
        DiffRow {
            row,
            kind: "pair",
            text: String::new(),
            hidden: None,
            step: None,
            left: Some(side(left, "removed")),
            right: Some(side(right, "added")),
            cursor: false,
            picked: false,
        }
    }

    /// The rows drawn and settled: the pane's width is read a frame after it is laid out.
    fn settle(cx: &mut TestAppContext, opened: &Opened, rows: &[DiffRow], split: bool) {
        cx.update(|cx| opened.view.update(cx, |view, cx| view.show(rows, split, None, cx)));
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        }
    }

    #[gpui_kit::test]
    fn a_line_longer_than_the_pane_scrolls_sideways_and_is_not_cut(cx: &mut TestAppContext) {
        let opened = open(cx, true);
        let rows = [line(0), text_line(1, &"x".repeat(400)), line(2)];
        settle(cx, &opened, &rows, false);
        let scroll = cx.update(|cx| opened.view.read(cx).scroll().clone());
        let max = scroll.max_offset().x;
        // 400 cells are far more than the 600 px pane, whatever the font.
        assert!(max > px(600.0), "the list has {max:?} to scroll sideways");
        let long = cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", 1usize)).bounds()).unwrap();
        let short = cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", 0usize)).bounds()).unwrap();
        assert_eq!(long.size.width, short.size.width, "every row carries the column's width, so each colour runs on");
        assert!(long.size.width > px(1000.0), "the row is {:?} wide, not cut at the pane", long.size.width);
        assert_eq!(scroll.offset().x, px(0.0));
        cx.update_window(opened.handle, |_, window, cx| {
            // Over the first screenful: the long row's centre is off the window.
            let wheel = gpui_kit::ScrollWheelEvent {
                position: point(px(100.0), px(30.0)),
                delta: gpui_kit::ScrollDelta::Pixels(point(px(-120.0), px(0.0))),
                ..Default::default()
            };
            window.dispatch_event(gpui_kit::InputEvent::to_platform_input(wheel), cx);
            window.render_frame(cx);
        })
        .unwrap();
        assert_eq!(scroll.offset().x, px(-120.0), "a sideways wheel moves the list sideways");
    }

    #[gpui_kit::test]
    fn a_nudge_and_a_middle_drag_scroll_sideways_held_to_the_content(cx: &mut TestAppContext) {
        let opened = open(cx, true);
        settle(cx, &opened, &[line(0), text_line(1, &"x".repeat(400)), line(2)], false);
        let scroll = cx.update(|cx| opened.view.read(cx).scroll().clone());
        cx.update(|cx| opened.view.update(cx, |view, cx| view.nudge(4.0, cx)));
        assert!(scroll.offset().x < px(0.0), "a nudge right shows more of the line");
        cx.update(|cx| opened.view.update(cx, |view, cx| view.nudge(-1000.0, cx)));
        assert_eq!(scroll.offset().x, px(0.0), "a nudge stops at the start");
        cx.update_window(opened.handle, |_, window, cx| {
            let at = point(px(300.0), px(30.0));
            window.dispatch_event(gpui_kit::InputEvent::to_platform_input(MouseDownEvent { button: MouseButton::Middle, position: at, click_count: 1, ..Default::default() }), cx);
            window.dispatch_event(
                gpui_kit::InputEvent::to_platform_input(MouseMoveEvent { position: point(px(150.0), px(30.0)), pressed_button: Some(MouseButton::Middle), ..Default::default() }),
                cx,
            );
            window.dispatch_event(gpui_kit::InputEvent::to_platform_input(MouseUpEvent { button: MouseButton::Middle, position: point(px(150.0), px(30.0)), click_count: 1, ..Default::default() }), cx);
            window.render_frame(cx);
        })
        .unwrap();
        assert_eq!(scroll.offset().x, px(-150.0), "the diff follows the middle-dragged pointer");
        assert!(opened.pressed.borrow().is_empty(), "a middle drag presses no row");
    }

    #[gpui_kit::test]
    fn a_diff_of_short_lines_has_no_sideways_range_and_fills_the_pane(cx: &mut TestAppContext) {
        let opened = open(cx, true);
        settle(cx, &opened, &[line(0), text_line(1, "short"), line(2)], false);
        let scroll = cx.update(|cx| opened.view.read(cx).scroll().clone());
        assert_eq!(scroll.max_offset().x, px(0.0));
        let width = cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", 1usize)).bounds().size.width).unwrap();
        assert_eq!(width, px(600.0), "a row is stretched to the pane");
    }

    #[gpui_kit::test]
    fn a_long_half_of_a_split_row_wraps_and_the_rows_around_it_keep_one_line(cx: &mut TestAppContext) {
        let opened = open(cx, true);
        let rows = [pair(0, "a", "b"), pair(1, &"y".repeat(200), "short"), pair(2, "c", "d")];
        settle(cx, &opened, &rows, true);
        let narrow = cx.update(|cx| opened.view.read(cx).heights.clone());
        assert_eq!(narrow[0], lines::LINE);
        assert_eq!(narrow[2], lines::LINE);
        assert!(narrow[1] > lines::LINE * 2.0, "200 cells in a half of a 600 px pane are several lines, got {}", narrow[1]);
        let drawn = cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", 1usize)).bounds().size.height).unwrap();
        assert_eq!(drawn, px(narrow[1]), "the row is drawn as tall as the list was told");
        let scroll = cx.update(|cx| opened.view.read(cx).scroll().clone());
        assert_eq!(scroll.max_offset().x, px(0.0), "a split view wraps and never scrolls sideways");

        let before = opened.notified.get();
        cx.simulate_window_resize(opened.handle, size(px(1200.0), px(400.0)));
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        cx.run_until_parked();
        assert!(opened.notified.get() > before, "a new pane width asks the view for another render, which reads it");
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        }
        let wide = cx.update(|cx| opened.view.read(cx).heights.clone());
        assert!(wide[1] > lines::LINE && wide[1] < narrow[1], "a wider pane wraps the half to fewer lines: {} then {}", narrow[1], wide[1]);
    }

    /// The point `columns` cells into row `ix`'s code, at its middle.
    fn in_code(cx: &mut TestAppContext, opened: &Opened, ix: usize, columns: f32) -> gpui_kit::Point<Pixels> {
        let advance = cx.update(|cx| opened.view.read(cx).advance);
        let bounds = cx.update_window(opened.handle, |_, window, _| window.find(("diff-row", ix)).bounds()).unwrap();
        // The border, both number columns and the marker come first.
        point(bounds.origin.x + px(NUMBER * 2.0 + MARKER + 1.0 + 1.0) + advance * columns, bounds.center().y)
    }

    fn dragged(cx: &mut TestAppContext, opened: &Opened, from: gpui_kit::Point<Pixels>, to: gpui_kit::Point<Pixels>) {
        cx.update_window(opened.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    }

    fn copied(cx: &mut TestAppContext, opened: &Opened) -> (bool, Option<String>) {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(String::new()));
        let had = cx.update(|cx| opened.view.update(cx, |view, cx| view.copy(cx)));
        (had, cx.read_from_clipboard().and_then(|item| item.text()))
    }

    #[gpui_kit::test]
    fn a_drag_across_three_rows_selects_their_text_and_copy_puts_it_on_the_clipboard(cx: &mut TestAppContext) {
        let opened = open_picking(cx, true, true);
        show(cx, &opened, &[text_line(0, "alpha"), text_line(1, "beta"), text_line(2, "gamma delta")]);
        let (from, to) = (in_code(cx, &opened, 0, 0.0), in_code(cx, &opened, 2, 5.2));
        dragged(cx, &opened, from, to);
        assert_eq!(copied(cx, &opened), (true, Some("alpha
beta
gamma".to_owned())));
        // From the middle of a row to the middle of another.
        let (from, to) = (in_code(cx, &opened, 0, 2.2), in_code(cx, &opened, 2, 2.2));
        dragged(cx, &opened, from, to);
        assert_eq!(copied(cx, &opened), (true, Some("pha
beta
ga".to_owned())));
        // And upward, the same text.
        dragged(cx, &opened, to, from);
        assert_eq!(copied(cx, &opened), (true, Some("pha
beta
ga".to_owned())));
    }

    #[gpui_kit::test]
    fn a_press_that_moves_under_five_pixels_is_the_rows_click_and_selects_nothing(cx: &mut TestAppContext) {
        let opened = open_picking(cx, true, true);
        show(cx, &opened, &[text_line(0, "alpha"), text_line(1, "beta")]);
        let from = in_code(cx, &opened, 0, 1.0);
        dragged(cx, &opened, from, point(from.x + px(3.0), from.y));
        assert_eq!(*opened.pressed.borrow(), [DiffEvent::Row { row: 0, extend: false }]);
        assert_eq!(copied(cx, &opened), (false, None));
    }

    #[gpui_kit::test]
    fn a_drag_across_rows_in_a_view_that_picks_tells_its_first_and_last_row_and_a_click_is_not_sent_too(cx: &mut TestAppContext) {
        let opened = open_picking(cx, true, true);
        show(cx, &opened, &[text_line(0, "alpha"), text_line(1, "beta"), text_line(2, "gamma")]);
        let (from, to) = (in_code(cx, &opened, 0, 1.0), in_code(cx, &opened, 2, 1.0));
        dragged(cx, &opened, from, to);
        assert_eq!(*opened.pressed.borrow(), [DiffEvent::Range { first: 0, last: 2 }]);
        opened.pressed.borrow_mut().clear();
        // Within one row: text is selected, nothing is picked.
        let (from, to) = (in_code(cx, &opened, 1, 0.0), in_code(cx, &opened, 1, 3.0));
        dragged(cx, &opened, from, to);
        assert!(opened.pressed.borrow().is_empty(), "{:?}", opened.pressed.borrow());
        assert_eq!(copied(cx, &opened), (true, Some("bet".to_owned())));
    }

    #[gpui_kit::test]
    fn a_view_that_does_not_pick_tells_nothing_of_a_drag_but_still_selects(cx: &mut TestAppContext) {
        let opened = open_picking(cx, false, false);
        show(cx, &opened, &[text_line(0, "alpha"), text_line(1, "beta")]);
        let (from, to) = (in_code(cx, &opened, 0, 0.0), in_code(cx, &opened, 1, 9.0));
        dragged(cx, &opened, from, to);
        assert!(opened.pressed.borrow().is_empty());
        assert_eq!(copied(cx, &opened), (true, Some("alpha
beta".to_owned())));
        // New rows clear it.
        show(cx, &opened, &[text_line(0, "other")]);
        assert!(!copied(cx, &opened).0);
    }

    #[gpui_kit::test]
    fn a_split_pair_selects_within_the_half_the_press_began_in(cx: &mut TestAppContext) {
        let opened = open_picking(cx, true, true);
        settle(cx, &opened, &[pair(0, "left one", "right one"), pair(1, "left two", "right two")], true);
        let bounds = cx.update_window(opened.handle, |_, window, _| (window.find(("diff-row", 0usize)).bounds(), window.find(("diff-row", 1usize)).bounds())).unwrap();
        let start = point(bounds.0.origin.x + px(NUMBER + MARKER + 2.0), bounds.0.center().y);
        // Dragged out over the right half: still the left half's text.
        let end = point(bounds.1.right() - px(4.0), bounds.1.center().y);
        dragged(cx, &opened, start, end);
        assert_eq!(copied(cx, &opened), (true, Some("left one
left two".to_owned())));
    }
}
