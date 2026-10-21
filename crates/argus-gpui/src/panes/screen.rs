//! What the window holds of the Workspace pane's screen: every row's styled
//! segments, kept so a frame that changed only some rows, or only the
//! cursor, still leaves the rest to draw.
//!
//! Frames are applied as `panes/screen.ts` applies them: a frame for another
//! pane, or a full one, drops every row held; a changed size keeps the rows
//! that still exist; the rows a frame lists replace those held, whole. What
//! the daemon's view holds (history, damage) is `argus-desktop-core`'s.

use argus_desktop_core::connection::page::{Frame, FrameCursor, Parked, Segment, WIDE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Block,
    Underline,
    Bar,
}

impl Shape {
    fn named(name: &str) -> Self {
        match name {
            "bar" => Shape::Bar,
            "underline" => Shape::Underline,
            _ => Shape::Block,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub row: u16,
    pub col: u16,
    pub visible: bool,
    pub shape: Shape,
}

impl From<FrameCursor> for Cursor {
    fn from(cursor: FrameCursor) -> Self {
        Cursor { row: cursor.row, col: cursor.col, visible: cursor.visible, shape: Shape::named(cursor.shape) }
    }
}

/// How many cells a segment covers: a wide cell is sent alone, with no
/// spacer after it, and covers two columns.
pub fn width(segment: &Segment) -> u16 {
    if segment.4 & WIDE != 0 {
        2
    } else {
        segment.1.len() as u16
    }
}

#[derive(Debug, Clone)]
pub struct Screen {
    pane: Option<u64>,
    rows: u16,
    cols: u16,
    lines: Vec<Vec<Segment>>,
    cursor: Cursor,
    parked: Option<Parked>,
}

impl Screen {
    /// Holds nothing: no pane, no rows.
    pub fn new() -> Self {
        Self {
            pane: None,
            rows: 0,
            cols: 0,
            lines: Vec::new(),
            cursor: Cursor { row: 0, col: 0, visible: false, shape: Shape::Block },
            parked: None,
        }
    }

    pub fn apply(&mut self, frame: Frame) {
        if self.pane != Some(frame.pane) || frame.full {
            self.pane = Some(frame.pane);
            self.lines = vec![Vec::new(); frame.rows as usize];
        }
        self.rows = frame.rows;
        self.cols = frame.cols;
        self.lines.resize(frame.rows as usize, Vec::new());
        for (row, segments) in frame.lines {
            if let Some(slot) = self.lines.get_mut(row as usize) {
                *slot = segments;
            }
        }
        self.cursor = frame.cursor.into();
        self.parked = frame.parked;
    }

    /// The pane the rows are of.
    pub fn pane(&self) -> Option<u64> {
        self.pane
    }

    /// The grid the daemon sent, which may differ from the room reported.
    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    pub fn lines(&self) -> &[Vec<Segment>] {
        &self.lines
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn parked(&self) -> Option<Parked> {
        self.parked
    }

    /// The glyph at a cell, either column of a wide one, and whether it is a
    /// wide one, for a block cursor to redraw in the background colour. A
    /// blank is a space.
    pub fn cell_at(&self, row: u16, col: u16) -> (&str, bool) {
        for segment in self.lines.get(row as usize).into_iter().flatten() {
            if col >= segment.0 && col < segment.0 + width(segment) {
                let wide = segment.4 & WIDE != 0;
                let i = if wide { 0 } else { (col - segment.0) as usize };
                return (segment.1.get(i).map_or(" ", String::as_str), wide);
            }
        }
        (" ", false)
    }

    pub fn glyph_at(&self, row: u16, col: u16) -> &str {
        self.cell_at(row, col).0
    }

    /// A row as text, blanks between segments kept and trailing ones
    /// dropped, for a log line that says what the screen holds.
    pub fn row_text(&self, row: u16) -> String {
        let mut text = String::new();
        let mut col = 0;
        for segment in self.lines.get(row as usize).into_iter().flatten() {
            while col < segment.0 {
                text.push(' ');
                col += 1;
            }
            for cell in &segment.1 {
                text.push_str(if cell.is_empty() { " " } else { cell });
            }
            col = segment.0 + width(segment);
        }
        text.trim_end().to_owned()
    }
}

impl Default for Screen {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::Side;

    fn seg(col: u16, text: &str) -> Segment {
        Segment(col, text.chars().map(|c| c.to_string()).collect(), 0, 0, 0)
    }

    fn frame(pane: u64, rows: u16, cols: u16, full: bool, lines: Vec<(u16, Vec<Segment>)>) -> Frame {
        Frame {
            pane,
            side: Side::Workspace,
            rows,
            cols,
            full,
            cursor: FrameCursor { row: 0, col: 0, visible: true, shape: "bar" },
            alternate_screen: false,
            mouse: false,
            lines,
            parked: None,
        }
    }

    #[test]
    fn a_full_frame_holds_every_row_it_lists() {
        let mut screen = Screen::new();
        screen.apply(frame(1, 2, 10, true, vec![(0, vec![seg(0, "one")]), (1, vec![seg(0, "two")])]));
        assert_eq!(screen.pane(), Some(1));
        assert_eq!(screen.size(), (2, 10));
        assert_eq!(screen.row_text(0), "one");
        assert_eq!(screen.row_text(1), "two");
    }

    #[test]
    fn a_partial_frame_replaces_only_its_rows() {
        let mut screen = Screen::new();
        screen.apply(frame(1, 2, 10, true, vec![(0, vec![seg(0, "one")]), (1, vec![seg(0, "two")])]));
        screen.apply(frame(1, 2, 10, false, vec![(1, vec![seg(0, "new")])]));
        assert_eq!(screen.row_text(0), "one");
        assert_eq!(screen.row_text(1), "new");
    }

    #[test]
    fn a_full_frame_clears_the_rows_it_does_not_list() {
        let mut screen = Screen::new();
        screen.apply(frame(1, 2, 10, true, vec![(0, vec![seg(0, "one")])]));
        screen.apply(frame(1, 2, 10, true, vec![(1, vec![seg(0, "x")])]));
        assert_eq!(screen.row_text(0), "");
        assert_eq!(screen.row_text(1), "x");
    }

    #[test]
    fn a_frame_for_another_pane_drops_the_rows_held() {
        let mut screen = Screen::new();
        screen.apply(frame(1, 2, 10, true, vec![(0, vec![seg(0, "one")])]));
        screen.apply(frame(2, 2, 10, false, vec![(1, vec![seg(0, "two")])]));
        assert_eq!(screen.pane(), Some(2));
        assert_eq!(screen.row_text(0), "");
        assert_eq!(screen.row_text(1), "two");
    }

    #[test]
    fn a_resize_keeps_the_rows_that_still_exist_and_adds_blank_ones() {
        let mut screen = Screen::new();
        screen.apply(frame(1, 3, 10, true, vec![(0, vec![seg(0, "a")]), (2, vec![seg(0, "c")])]));
        screen.apply(frame(1, 2, 8, false, Vec::new()));
        assert_eq!(screen.size(), (2, 8));
        assert_eq!(screen.lines().len(), 2);
        assert_eq!(screen.row_text(0), "a");
        screen.apply(frame(1, 4, 8, false, vec![(3, vec![seg(0, "d")])]));
        assert_eq!(screen.lines().len(), 4);
        assert_eq!(screen.row_text(0), "a");
        assert_eq!(screen.row_text(3), "d");
    }

    #[test]
    fn a_row_past_the_grid_is_ignored() {
        let mut screen = Screen::new();
        screen.apply(frame(1, 1, 10, true, vec![(5, vec![seg(0, "lost")])]));
        assert_eq!(screen.lines().len(), 1);
        assert_eq!(screen.row_text(5), "");
    }

    #[test]
    fn the_cursor_and_the_parked_view_come_with_every_frame() {
        let mut screen = Screen::new();
        let mut f = frame(1, 1, 10, true, Vec::new());
        f.cursor = FrameCursor { row: 0, col: 4, visible: false, shape: "underline" };
        f.parked = Some(Parked { offset: 3, depth: 9 });
        screen.apply(f);
        assert_eq!(screen.cursor(), Cursor { row: 0, col: 4, visible: false, shape: Shape::Underline });
        assert_eq!(screen.parked(), Some(Parked { offset: 3, depth: 9 }));
        screen.apply(frame(1, 1, 10, false, Vec::new()));
        assert_eq!(screen.cursor().shape, Shape::Bar);
        assert_eq!(screen.parked(), None);
    }

    #[test]
    fn the_glyph_under_a_wide_cell_reads_from_either_column() {
        let mut screen = Screen::new();
        let wide = Segment(1, vec!["日".into()], 0, 0, WIDE);
        screen.apply(frame(1, 1, 10, true, vec![(0, vec![seg(0, "a"), wide])]));
        assert_eq!(screen.glyph_at(0, 0), "a");
        assert_eq!(screen.glyph_at(0, 1), "日");
        assert_eq!(screen.glyph_at(0, 2), "日");
        assert_eq!(screen.glyph_at(0, 3), " ");
    }

    #[test]
    fn row_text_keeps_the_blanks_between_segments() {
        let mut screen = Screen::new();
        screen.apply(frame(1, 1, 10, true, vec![(0, vec![seg(0, "ab"), seg(5, "cd")])]));
        assert_eq!(screen.row_text(0), "ab   cd");
    }
}
