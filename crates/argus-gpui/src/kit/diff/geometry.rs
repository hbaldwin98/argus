//! Where the rows are and what part of them is on screen, in the window's pixels: for whatever
//! draws a ruler beside the list, or scrolls it to a row of its own choosing.
//!
//! The list's rows have heights told up front, so where a row stands is known from the rows alone
//! (a removal marker or the comment box between lines is among them), and a ruler placed from
//! these moves with them. A row is found by the `row` its `DiffRow` carries, which for the Code
//! stage is the line's number.

use gpui_kit::{point, px};

use super::items::Item;
use super::shape::{self, Shape};
use super::DiffView;

impl DiffView {
    /// The top of the first row numbered `first` and the bottom of the last numbered `last`, from
    /// the top of the content; none when `first` is not among the rows. A `last` that is not there
    /// is the first row alone.
    pub fn rows_span(&self, first: usize, last: usize) -> Option<(f32, f32)> {
        let from = *self.numbered.get(&first)?;
        let to = self.numbered.get(&last).copied().unwrap_or(from);
        Some((self.tops[from], self.tops[to + 1]))
    }

    /// Finds each line or pair by its number, among the items.
    pub(super) fn number_rows(&mut self) {
        self.numbered.clear();
        for (at, item) in self.items.iter().enumerate() {
            let Item::Row(row) = item else { continue };
            let row = &self.rows[*row];
            if matches!(shape::shape(row), Shape::Line | Shape::Pair) {
                self.numbered.entry(row.row).or_insert(at);
            }
        }
    }

    /// How tall everything is together, the comment box and the markers among the rows included.
    pub fn content_px(&self) -> f32 {
        self.tops.last().copied().unwrap_or(0.0)
    }

    /// How tall the list is where it is drawn: nothing before its first frame.
    pub fn viewport_px(&self) -> f32 {
        f32::from(self.scroll.bounds().size.height)
    }

    /// How far down the content the list is scrolled.
    pub fn scrolled_px(&self) -> f32 {
        (-f32::from(self.scroll.offset().y)).max(0.0)
    }

    /// Scrolls the content to `y` from its top, keeping the sideways scroll. The list holds it to its
    /// own ends when it is next laid out.
    pub fn scroll_to_px(&self, y: f32) {
        let offset = self.scroll.offset();
        self.scroll.set_offset(point(offset.x, px(-y)));
    }

    /// Where the row numbered `number` starts and how tall it is, for the cursor.
    pub fn row_px(&self, number: usize) -> Option<(f32, f32)> {
        self.rows_span(number, number).map(|(top, bottom)| (top, bottom - top))
    }
}
