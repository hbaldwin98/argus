//! How wide and how tall the rows are: the cells of the widest line, the cells a split half wraps
//! at in this pane, each row's height, and the width every row carries.
//!
//! A unified diff is one column as wide as its longest line (or the pane, when that is wider), so
//! the list scrolls sideways and each row's colour runs on past the edge; the list measures one
//! item for its content width, so each row carries that width itself. A split half wraps at the
//! cell and the list is told each row's height up front, which is why the heights are recomputed
//! when the pane's width changes: a canvas reports it, the render reads it a frame later.

use std::rc::Rc;

use argus_desktop_core::connection::page::DiffSide;
use gpui_kit::{px, size, Pixels, Styled};

use super::gutter::{MARKER, NUMBER};
use super::items::Item;
use super::shape::{self, Shape};
use super::{text, wrap, DiffView};

/// Room after the last cell of a line, so its end is not flush with the edge
/// of its colour.
const TAIL: f32 = 8.0;
/// A hunk header's or note's side padding, both sides.
const HEADING_PAD: f32 = 24.0;
/// What a row's own border takes of its width, both sides.
const ROW_BORDER: f32 = 2.0;

impl DiffView {
    /// The sides of every row as drawn, and how wide the widest are.
    pub(super) fn draw(&mut self) {
        self.drawn = self
            .rows
            .iter()
            .map(|row| {
                let code = |side: &Option<DiffSide>| side.as_ref().map(text::code).unwrap_or_default();
                [code(&row.left), code(&row.right)]
            })
            .collect();
        let mut widest = (0, 0);
        for (row, drawn) in self.rows.iter().zip(&self.drawn) {
            match shape::shape(row) {
                Shape::Hunk | Shape::Note => widest.1 = widest.1.max(text::cells(&row.text)),
                Shape::Line => widest.0 = widest.0.max(text::cells(&drawn[0].text)),
                Shape::Fold | Shape::Pair => {}
            }
        }
        self.widest = widest;
    }

    /// The cells a split half's code wraps at in this pane: 0, no wrapping,
    /// until the pane and the font have been measured and for a unified diff.
    pub(super) fn wrap_at(&self) -> usize {
        let (pane, advance) = (f32::from(self.pane.get()), f32::from(self.advance));
        if !self.split || pane <= 0.0 || advance <= 0.0 {
            return 0;
        }
        // Row border both sides, the second half's left rule, and a few
        // pixels of slack against a rounding that would push a full line over.
        let half = (pane - (ROW_BORDER + 1.0) * self.zoom) / 2.0;
        (((half - (NUMBER + MARKER + TAIL) * self.zoom) / advance).floor() as usize).max(1)
    }

    /// `heights` (the page's pixels) and `sizes` (the window's, at the zoom) for the rows at `per_line`.
    pub(super) fn size(&mut self) {
        self.heights = self
            .rows
            .iter()
            .zip(&self.drawn)
            .map(|(row, drawn)| match shape::shape(row) {
                Shape::Pair => {
                    let most = drawn.iter().map(|side| wrap::wrapped_lines(&side.text, self.per_line)).max().unwrap_or(1);
                    shape::pair_height(most)
                }
                _ => shape::height(row),
            })
            .collect();
        self.sized_at = self.zoom;
        let slot_height = self.slot.as_ref().map_or(px(0.0), |slot| slot.height);
        self.sizes = Rc::new(
            self.items
                .iter()
                .map(|item| match item {
                    Item::Row(i) => size(px(0.0), px(self.heights[*i] * self.zoom)),
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
        let line = (NUMBER * 2.0 + MARKER + TAIL + ROW_BORDER) * self.zoom + self.widest.0 as f32 * advance;
        let heading = HEADING_PAD * self.zoom + self.widest.1 as f32 * advance;
        Some(pane.max(px(line.max(heading).ceil())))
    }

    pub(super) fn wide<E: Styled>(&self, element: E) -> E {
        match self.width() {
            Some(width) => element.w(width),
            None => element.w_full(),
        }
    }
}
