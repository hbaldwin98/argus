//! The element that paints a pane's screen: one mono cell measured, each row
//! of segments turned into shaped text runs and merged background quads
//! (`panes/layout`), and the cursor over them.
//!
//! It paints only the grid it was given, in the cells that fit its own size,
//! and says how many rows and columns that size holds the first time it is
//! laid out at a new size, which the view reports to the session as the
//! page reports its container's. The spike measured a full 140x40 frame at
//! about 0.55 ms in release, so nothing is cached between frames.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{
    fill, outline, point, px, relative, size, App, Bounds, BorderStyle, ContentMask, Element, ElementId,
    ElementInputHandler, Entity, FocusHandle, Font, FontStyle, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString, Style, TextAlign, TextRun,
    UnderlineStyle, Window,
};

use crate::panes::grid::grid_for;
use crate::panes::layout::{layout, TextBatch};
use crate::panes::palette::Palette;
use crate::panes::screen::{Screen, Shape};
use crate::panes::selection::Selection;
use crate::panes::terminal::Terminal;

/// The terminal's type as the page sets it: 13px on a 20px line.
pub const FONT_SIZE: f32 = 13.0;
pub const LINE_HEIGHT: f32 = 20.0;

pub struct TerminalElement {
    pub screen: Arc<Screen>,
    pub palette: Palette,
    pub family: SharedString,
    /// Whether this pane has the keys and the window is the person's: a
    /// block cursor is filled, else hollow.
    pub focused: bool,
    /// The cells a drag across this screen covers, drawn over the text.
    pub selection: Option<Selection>,
    /// How strongly the selection washes the text colour (`share_term_selection`).
    pub share: f32,
    /// The view that hears the room this element has and takes its text.
    pub terminal: Entity<Terminal>,
    pub focus: FocusHandle,
    /// The grid last said to the view, so one size is said once.
    pub seen: Rc<Cell<Option<(u16, u16)>>>,
    /// The cell this element measured, for the input method's candidate
    /// window to sit by the cursor.
    pub cell: Rc<Cell<(Pixels, Pixels)>>,
    /// Where the grid starts in the window, for the cell under the pointer.
    pub origin: Rc<Cell<Point<Pixels>>>,
    /// How many times the screen was painted.
    pub paints: Rc<Cell<u64>>,
}

pub struct Prepainted {
    font: Font,
    cell_w: Pixels,
    cell_h: Pixels,
    fit: (u16, u16),
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepainted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut (),
        _window: &mut Window,
        cx: &mut App,
    ) -> Prepainted {
        let font = Font { family: self.family.clone(), ..Font::default() };
        let text = cx.text_system();
        let font_id = text.resolve_font(&font);
        // The advance of `M`, as the page measures: one cell is one advance,
        // and the row height is the type's own line height, not the font's.
        let advance = text.advance(font_id, px(FONT_SIZE), 'M').map_or(px(FONT_SIZE * 0.6), |s| s.width);
        let (cell_w, cell_h) = (advance, px(LINE_HEIGHT));
        let fit = grid_for(
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            f32::from(cell_w),
            f32::from(cell_h),
        );
        self.cell.set((cell_w, cell_h));
        self.origin.set(bounds.origin);
        // A hidden or zero-sized element has no room to report: measuring it
        // would size the pane to one cell.
        let room = bounds.size.width > px(0.0) && bounds.size.height > px(0.0);
        if room && self.seen.get() != Some(fit) {
            self.seen.set(Some(fit));
            let terminal = self.terminal.clone();
            // The view is not updated while a frame is being built.
            cx.defer(move |cx| {
                terminal.update(cx, |terminal, cx| terminal.room(fit.0, fit.1, cx));
            });
        }
        Prepainted { font, cell_w, cell_h, fit }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut (),
        metrics: &mut Prepainted,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.paints.set(self.paints.get() + 1);
        window.handle_input(&self.focus, ElementInputHandler::new(bounds, self.terminal.clone()), cx);
        window.paint_quad(fill(bounds, self.palette.bg.hsla()));

        let laid = layout(self.screen.lines(), &self.palette, metrics.fit);
        let (cell_w, cell_h) = (metrics.cell_w, metrics.cell_h);
        let origin = bounds.origin;
        let at = |row: u16, col: u16| point(origin.x + cell_w * f32::from(col), origin.y + cell_h * f32::from(row));
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for f in &laid.fills {
                let rect = Bounds {
                    origin: at(f.row, f.col),
                    size: size(cell_w * f32::from(f.cols), cell_h * f32::from(f.rows)),
                };
                window.paint_quad(fill(rect, f.color.hsla()));
            }
            for batch in &laid.batches {
                self.paint_batch(batch, at(batch.row, batch.col), metrics, window, cx);
            }
            self.paint_selection(metrics, origin, window);
            self.paint_cursor(metrics, origin, window, cx);
        });
    }
}

impl TerminalElement {
    fn paint_batch(&self, batch: &TextBatch, at: Point<Pixels>, metrics: &Prepainted, window: &mut Window, cx: &mut App) {
        let style = batch.style;
        let mut font = metrics.font.clone();
        if style.bold {
            font.weight = FontWeight::BOLD;
        }
        if style.italic {
            font.style = FontStyle::Italic;
        }
        let color = style.fg.hsla();
        let underline = style.underline.then(|| UnderlineStyle { thickness: px(1.0), color: Some(color), wavy: false });
        self.paint_glyphs(&batch.text, font, color, underline, batch.wide, at, metrics, window, cx);
    }

    /// `text` at `at`. Forcing every glyph's advance to one cell is what
    /// keeps a proportional fallback glyph on the grid; a wide glyph keeps
    /// its own advance and is centred over its two cells.
    #[allow(clippy::too_many_arguments)]
    fn paint_glyphs(
        &self,
        text: &str,
        font: Font,
        color: Hsla,
        underline: Option<UnderlineStyle>,
        wide: bool,
        at: Point<Pixels>,
        metrics: &Prepainted,
        window: &mut Window,
        cx: &mut App,
    ) {
        let run = TextRun {
            len: text.len(),
            font,
            color,
            background_color: None,
            underline,
            strikethrough: None,
        };
        let force = (!wide).then_some(metrics.cell_w);
        let line = window.text_system().shape_line(SharedString::from(text.to_owned()), px(FONT_SIZE), &[run], force);
        let at = if wide {
            point(at.x + px(centred(f32::from(metrics.cell_w) * 2.0, f32::from(line.width()))), at.y)
        } else {
            at
        };
        line.paint(at, metrics.cell_h, TextAlign::Left, None, window, cx).ok();
    }

    /// The selection as the page draws it: the text colour at a share of its strength over the
    /// cells it covers, in reading order, under the cursor.
    fn paint_selection(&self, metrics: &Prepainted, origin: Point<Pixels>, window: &mut Window) {
        let Some(selection) = self.selection else { return };
        let (rows, cols) = self.screen.size();
        let (fit_rows, fit_cols) = metrics.fit;
        let wash = self.palette.fg.hsla().opacity(self.share);
        for row in 0..rows.min(fit_rows) {
            let Some((from, to)) = selection.span(row, rows, cols.min(fit_cols)) else { continue };
            let top_left = point(origin.x + metrics.cell_w * f32::from(from), origin.y + metrics.cell_h * f32::from(row));
            let band = size(metrics.cell_w * f32::from(to - from), metrics.cell_h);
            window.paint_quad(fill(Bounds { origin: top_left, size: band }, wash));
        }
    }

    /// The cursor as the page draws it: a block in the text colour with its
    /// glyph redrawn in the background colour, hollow when the pane has no
    /// keys, or a bar or underline when the child asked for one. Hidden when
    /// the child hid it or it is outside the cells that fit.
    fn paint_cursor(&self, metrics: &Prepainted, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        let cursor = self.screen.cursor();
        let (rows, cols) = metrics.fit;
        let (grid_rows, grid_cols) = self.screen.size();
        if !cursor.visible || cursor.row >= rows.min(grid_rows) || cursor.col >= cols.min(grid_cols) {
            return;
        }
        let (cell_w, cell_h) = (metrics.cell_w, metrics.cell_h);
        let top_left = point(origin.x + cell_w * f32::from(cursor.col), origin.y + cell_h * f32::from(cursor.row));
        let color = self.palette.fg.hsla();
        match cursor.shape {
            Shape::Bar => {
                window.paint_quad(fill(Bounds { origin: top_left, size: size(px(2.0), cell_h) }, color));
            }
            Shape::Underline => {
                let origin = point(top_left.x, top_left.y + cell_h - px(2.0));
                window.paint_quad(fill(Bounds { origin, size: size(cell_w, px(2.0)) }, color));
            }
            Shape::Block if !self.focused => {
                let cell = Bounds { origin: top_left, size: size(cell_w, cell_h) };
                window.paint_quad(outline(cell, color, BorderStyle::Solid));
            }
            Shape::Block => {
                let (glyph, wide) = self.screen.cell_at(cursor.row, cursor.col);
                let cells = if wide { 2.0 } else { 1.0 };
                window.paint_quad(fill(Bounds { origin: top_left, size: size(cell_w * cells, cell_h) }, color));
                if glyph != " " {
                    let font = Font { family: self.family.clone(), ..Font::default() };
                    self.paint_glyphs(glyph, font, self.palette.bg.hsla(), None, wide, top_left, metrics, window, cx);
                }
            }
        }
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

/// How far in from the left of `room` a glyph `glyph` wide starts to sit in
/// the middle of it; negative when the glyph is the wider.
fn centred(room: f32, glyph: f32) -> f32 {
    (room - glyph) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_glyph_is_centred_over_its_two_cells() {
        assert_eq!(centred(16.0, 12.0), 2.0);
        assert_eq!(centred(16.0, 16.0), 0.0);
    }

    #[test]
    fn a_glyph_wider_than_its_cells_overhangs_both_sides_evenly() {
        assert_eq!(centred(16.0, 20.0), -2.0);
    }
}
