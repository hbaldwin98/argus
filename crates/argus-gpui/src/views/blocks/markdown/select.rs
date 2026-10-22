//! Which text is selected: where a press of a leaf begins it, how a drag extends it across leaves
//! and past the edge of the text, and what a click on a link does.
//!
//! A press and a drag past the 5 px of `kit::drag` selects text as a browser's selection does; a
//! press that never goes 5 px is a click, which opens the link under it (`Call::OpenUrl`, asked of
//! Rust) and otherwise puts the selection away. The columns are the text layouts of this build,
//! so a place is read from what is on screen. The text the selection reads as is core's
//! (`leaves::text_between`); what copies it is Rust's.

use std::ops::Range;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::views::markdown::leaves::{text_between, Pos};
use gpui_kit::{Context, FocusHandle, Pixels, Point, Window};

use super::Markdown;
use crate::kit::drag::{Drag, Dragger, Step};
use crate::views::link;

/// What a press holds: the leaf it began on and where the pointer went down.
#[derive(Clone)]
pub struct Held {
    leaf: usize,
    from: Point<Pixels>,
}

impl Markdown {
    /// The press of the left button at `at`: the drag helper follows it from here. A press that is
    /// on no text puts the selection away, as a click on the page's background does.
    pub(super) fn pressed(&mut self, at: Point<Pixels>, _: &mut Window, cx: &mut Context<Self>) {
        match self.leaf_at(at) {
            Some(leaf) => self.drag.press(at, Held { leaf, from: at }),
            None => {
                if self.selection.take().is_some() {
                    cx.notify();
                }
            }
        }
    }

    /// The leaf whose text is under `at`.
    fn leaf_at(&self, at: Point<Pixels>) -> Option<usize> {
        self.layouts.borrow().iter().find(|(_, layout)| layout.bounds().contains(&at)).map(|(leaf, _)| *leaf)
    }

    /// The place in leaf `leaf` the pointer at `at` is over, or the nearest to it.
    fn spot_in(&self, leaf: usize, at: Point<Pixels>) -> Option<Pos> {
        let layouts = self.layouts.borrow();
        let (Ok(index) | Err(index)) = layouts.get(&leaf)?.index_for_position(at);
        Some(Pos { leaf, at: index.min(self.leaves.get(leaf)?.styled.text.len()) })
    }

    /// The place the pointer at `at` is over: in the text under it, else in the nearest text on its
    /// line, else the end of the text above it or the start of what is below, so a drag out of the
    /// text or between two blocks still extends the selection.
    fn spot_near(&self, at: Point<Pixels>) -> Option<Pos> {
        let layouts = self.layouts.borrow();
        let mut beside: Option<(usize, Pixels)> = None;
        let mut above: Option<(usize, Pixels)> = None;
        let mut below: Option<(usize, Pixels)> = None;
        for (&leaf, layout) in layouts.iter() {
            let bounds = layout.bounds();
            if bounds.contains(&at) {
                return self.spot_in(leaf, at);
            }
            if at.y >= bounds.top() && at.y < bounds.bottom() {
                let gap = if at.x < bounds.left() { bounds.left() - at.x } else { at.x - bounds.right() };
                if beside.is_none_or(|(_, best)| gap < best) {
                    beside = Some((leaf, gap));
                }
            } else if bounds.bottom() <= at.y {
                if above.is_none_or(|(held, bottom)| bounds.bottom() > bottom || (bounds.bottom() == bottom && leaf > held)) {
                    above = Some((leaf, bounds.bottom()));
                }
            } else if below.is_none_or(|(held, top)| bounds.top() < top || (bounds.top() == top && leaf < held)) {
                below = Some((leaf, bounds.top()));
            }
        }
        if let Some((leaf, _)) = beside {
            return self.spot_in(leaf, at);
        }
        let end = |leaf: usize| Pos { leaf, at: self.leaves[leaf].styled.text.len() };
        match (above, below) {
            (Some((leaf, _)), _) => Some(end(leaf)),
            (None, Some((leaf, _))) => Some(Pos { leaf, at: 0 }),
            (None, None) => None,
        }
    }

    fn ordered(&self) -> Option<(Pos, Pos)> {
        let (a, b) = self.selection?;
        Some(if a <= b { (a, b) } else { (b, a) })
    }

    /// The bytes of leaf `leaf` that are selected.
    pub(super) fn selected_in(&self, leaf: usize) -> Option<Range<usize>> {
        let (a, b) = self.ordered()?;
        if leaf < a.leaf || leaf > b.leaf {
            return None;
        }
        let len = self.leaves.get(leaf)?.styled.text.len();
        let range = (if leaf == a.leaf { a.at } else { 0 })..(if leaf == b.leaf { b.at } else { len });
        (range.start < range.end).then_some(range)
    }

    /// The selected text, or `None` when nothing is selected.
    pub fn selection_text(&self) -> Option<String> {
        let (a, b) = self.ordered()?;
        text_between(&self.leaves, a, b)
    }

    /// Asks Rust to copy the selection; whether there was one. What a host's copy key calls.
    pub fn copy_selection(&self) -> bool {
        match self.selection_text() {
            Some(text) if !text.is_empty() => {
                self.caller.call(Call::CopyText { text });
                true
            }
            _ => false,
        }
    }

    /// Moves the selection's far end to the pointer at `at`.
    fn extend(&mut self, at: Point<Pixels>) {
        let (Some((anchor, _)), Some(head)) = (self.selection, self.spot_near(at)) else { return };
        self.selection = Some((anchor, head));
    }

    /// The link under the place a press went down, if it is on one.
    fn link_at(&self, held: &Held) -> Option<String> {
        let layouts = self.layouts.borrow();
        let layout = layouts.get(&held.leaf)?;
        let Ok(index) = layout.index_for_position(held.from) else { return None };
        self.leaves.get(held.leaf)?.styled.link_at(index).map(str::to_owned)
    }
}

impl Dragger for Markdown {
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

    fn dragged(&mut self, step: Step<Held>, _: &mut Window, _: &mut Context<Self>) {
        match step {
            Step::Began(held) => {
                self.selection = self.spot_in(held.leaf, held.from).map(|place| (place, place));
                if let Some(at) = self.drag.at() {
                    self.extend(at);
                }
            }
            Step::Moved(_) => {
                if let Some(at) = self.drag.at() {
                    self.extend(at);
                }
            }
            Step::Dropped(_, at) => {
                self.extend(at);
                if self.selection.is_some_and(|(a, b)| a == b) {
                    self.selection = None;
                }
            }
            Step::Clicked(held) => {
                self.selection = None;
                if let Some(url) = self.link_at(&held) {
                    self.caller.call(link::open(&url));
                }
            }
            Step::Cancelled(_) => self.selection = None,
        }
    }
}
