//! What the stage's header says of the Workspace's grid: where a parked view is, and the size only
//! for a moment after it changed, since it is not news at rest (SHELL-SCREEN-10).
//!
//! `panes/size_label.ts` and the part of `panes/screen.ts` that decides when a size is news. Time
//! is a value the caller reads, so the rule is the same in a test as under a timer, and nothing
//! here keeps a repaint loop alive: the caller asks again once [`SHOWN_FOR`] and a moment have
//! passed.

use std::time::{Duration, Instant};

use argus_desktop_core::connection::page::Parked;

/// How long the grid's size shows after it changes.
pub const SHOWN_FOR: Duration = Duration::from_millis(1000);

/// How long after the window ends that the label is worked out again: a hair past it, so the
/// asking never lands a hair before.
pub const REASK_AFTER: Duration = Duration::from_millis(1020);

#[derive(Default)]
pub struct SizeLabel {
    pane: Option<u64>,
    rows: u16,
    cols: u16,
    parked: Option<Parked>,
    /// When the size stops showing.
    until: Option<Instant>,
}

impl SizeLabel {
    /// A frame left the Workspace's screen holding `pane` at `rows` by `cols`, parked or not.
    /// Whether the size starts to show: a new size that is not another pane's and not the first
    /// grid anyone saw, which nobody resized.
    pub fn frame(&mut self, pane: Option<u64>, rows: u16, cols: u16, parked: Option<Parked>, now: Instant) -> bool {
        let switched = pane != self.pane;
        let resized = (rows, cols) != (self.rows, self.cols);
        let news = resized && !switched && self.cols != 0;
        self.pane = pane;
        self.rows = rows;
        self.cols = cols;
        self.parked = parked;
        if news {
            self.until = Some(now + SHOWN_FOR);
        }
        news
    }

    /// The words: nothing for no grid; a parked view leads with how far back it is, which stays
    /// at rest, and the size follows while it shows.
    pub fn text(&self, now: Instant) -> String {
        if self.cols == 0 {
            return String::new();
        }
        let back = self.parked.map(|parked| format!("\u{2191} {}/{}", parked.offset, parked.depth));
        let size = self.until.is_some_and(|until| now < until).then(|| format!("{} \u{d7} {}", self.cols, self.rows));
        match (back, size) {
            (Some(back), Some(size)) => format!("{back} \u{b7} {size}"),
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => String::new(),
        }
    }
}

#[cfg(test)]
mod tests;
