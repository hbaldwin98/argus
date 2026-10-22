//! Which text is selected: where a press of a line's code begins it, how a drag extends it over
//! rows and past the list's edge, what it reads as, and what copies.
//!
//! A press on a line's code and a drag past the 5 px of `kit::drag` selects its text, as a
//! browser's selection does: the numbers and the marker are never in it, a split pair selects
//! within the half the press began in, and the selection is the window's own (a row index and a
//! byte of its drawn text at each end), gone when the rows change. The columns are the text
//! layouts the rows were drawn with, kept for the frame, so a position is read from what is on
//! screen. What copies it is the owner's (`selection_text`, sent to Rust); a view that `picks` also tells it a drag that crossed
//! rows, for it to send what a click then a shift-click would. A press that never goes 5 px stays
//! the row's click. Past the top or bottom edge the list scrolls with each move of the pointer; it
//! does not keep scrolling while the pointer is still.

use std::ops::Range;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{px, Context, FocusHandle, Pixels, Point, Window};

use super::paint::Ink;
use super::shape::{self, Shape};
use super::wrap;
use super::{DiffEvent, DiffView};
use crate::kit::drag::{Drag, Dragger, Step};

/// A place in the text: a row (an index of the rows, not its number), the
/// half of it in a split pair, and a byte of that half's drawn text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pos {
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

impl DiffView {
    /// The press of a code: the drag helper follows it from here.
    pub(super) fn press(&mut self, row: usize, half: usize, from: Point<Pixels>) {
        self.drag.press(from, Held { row, half, from });
    }

    /// What `code` needs of row `row`'s `half`, whose text is drawn as `shown`.
    pub(super) fn ink(&self, row: usize, half: usize, shown: &str, cx: &Context<Self>) -> Ink {
        let selected = self.selected(row, half).and_then(|range| {
            let (start, end) = (wrap::wrapped_index(shown, range.start, true), wrap::wrapped_index(shown, range.end, false));
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

    /// The selected text, a line to a row, or `None` when nothing is selected. What copies it is
    /// the owner's, through Rust's one clipboard writer (SHELL-SYSTEM-8): a view that wrote the
    /// platform clipboard itself would say nothing when it failed.
    pub fn selection_text(&self) -> Option<String> {
        let (a, b) = self.ordered()?;
        let mut out = Vec::new();
        for row in a.row..=b.row {
            let Some(drawn) = self.drawn.get(row) else { break };
            let present = self.rows.get(row).is_some_and(|row| if a.half == 0 { row.left.is_some() } else { row.right.is_some() });
            if !present || !matches!(shape::shape(&self.rows[row]), Shape::Line | Shape::Pair) {
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

    /// The place at `index` of a layout, in the text before any wrapping.
    fn spot(&self, row: usize, half: usize, index: usize) -> Pos {
        let shown = wrap::wrap(&self.drawn[row][half], self.per_line);
        Pos { row, half, at: wrap::unwrapped(&shown.text, index.min(shown.text.len())) }
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
