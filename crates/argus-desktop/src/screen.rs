//! A pane's screen as the page draws it: the rows of the pane's view that
//! changed since the last frame, as styled segments.
//!
//! What the view holds — the live grid, a view parked in the history, how
//! the daemon's messages and the person's scrolling change them — is
//! `argus_protocol::panes::view`'s, the same one the terminal client keeps,
//! so this window reads damage and scrolls exactly as that client does.

use argus_protocol::panes::view::{Effect, PaneView, Redraw};
use argus_protocol::{Cell, Color, Cursor, CursorShape, MouseTracking, PaneId, ServerMsg, BLANK};
use unicode_width::UnicodeWidthChar;

use crate::connection::page::{Frame, FrameCursor, Parked, Segment, Side, BOLD, ITALIC, REVERSE, UNDERLINE, WIDE};

pub struct Screen {
    view: PaneView,
    /// Rows changed since the last frame.
    dirty: Vec<bool>,
    /// Every row is to be sent: a new grid, a view that moved, or a page
    /// that holds nothing.
    full: bool,
    /// The cursor and modes the last frame carried, so a frame goes out
    /// when only they changed.
    sent: Option<(Cursor, MouseTracking, bool)>,
}

impl Screen {
    pub fn new(pane: PaneId) -> Screen {
        Screen { view: PaneView::new(pane), dirty: Vec::new(), full: false, sent: None }
    }

    pub fn view(&self) -> &PaneView {
        &self.view
    }

    /// Applies a message about this pane's view; `None` for anything else,
    /// which is left alone.
    pub fn receive(&mut self, msg: &ServerMsg) -> Option<Effect> {
        let effect = self.view.receive(msg)?;
        self.redraw(&effect.redraw);
        Some(effect)
    }

    /// Runs a gesture on the view, keeping what it redraws for the next
    /// frame and handing back what it asks.
    pub fn gesture(&mut self, gesture: impl FnOnce(&mut PaneView) -> Effect) -> Effect {
        let effect = gesture(&mut self.view);
        self.redraw(&effect.redraw);
        effect
    }

    /// Sends every row with the next frame: the page reloaded, or came back
    /// to this pane, and holds nothing of it.
    pub fn refresh(&mut self) {
        self.full = true;
    }

    /// What changed since the last frame, or `None` when nothing did. A
    /// parked view sends only its own rows, and only when they change.
    pub fn take_frame(&mut self) -> Option<Frame> {
        if !self.view.received() {
            return None;
        }
        let (rows, cols) = self.view.size();
        let modes = (self.view.cursor(), self.view.mouse(), self.view.alternate_screen());
        let parked = self.view.parked();
        let moved = parked.is_none() && self.sent != Some(modes);
        let full = self.full;
        if !full && !moved && !self.dirty.contains(&true) {
            return None;
        }
        let drawn = self.view.rows();
        let lines = (0..rows)
            .filter(|row| full || self.dirty[*row as usize])
            .map(|row| (row, segments(&drawn[row as usize])))
            .collect();
        self.full = false;
        self.dirty.fill(false);
        self.sent = Some(modes);
        let (cursor, mouse, alternate_screen) = modes;
        // A parked view is history: the child's cursor is on the live
        // screen, which is not the one drawn.
        let cursor = match parked {
            Some(_) => FrameCursor { row: 0, col: 0, visible: false, shape: "block" },
            None => FrameCursor { row: cursor.row, col: cursor.col, visible: cursor.visible, shape: shape(cursor.shape) },
        };
        Some(Frame {
            pane: self.view.pane().0,
            // Which side draws the screen is `shown`'s to say.
            side: Side::Workspace,
            rows,
            cols,
            full,
            cursor,
            alternate_screen,
            mouse: mouse.enabled(),
            lines,
            parked: parked.map(|view| Parked { offset: view.offset, depth: view.depth }),
        })
    }

    /// The text between two cells, inclusive and in reading order whichever
    /// way the drag went, as the terminal client copies it: each row's
    /// trailing blanks dropped, rows joined by newlines.
    pub fn text(&self, a: (u16, u16), b: (u16, u16)) -> String {
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        let grid = self.view.rows();
        let last = (end.0 as usize).min(grid.len().saturating_sub(1));
        let mut lines = Vec::new();
        for (index, row) in grid.iter().enumerate().take(last + 1).skip(start.0 as usize) {
            let from = if index == start.0 as usize { start.1 as usize } else { 0 };
            let to = if index == end.0 as usize { end.1 as usize + 1 } else { row.len() };
            let mut line = String::new();
            let mut col = 0;
            while col < to.min(row.len()) {
                let cell = &row[col];
                if col >= from {
                    line.push_str(&cell.ch);
                }
                col += if is_wide(cell) { 2 } else { 1 };
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    fn redraw(&mut self, redraw: &Redraw) {
        match redraw {
            Redraw::Nothing => {}
            Redraw::Rows(rows) => {
                for row in rows {
                    if let Some(dirty) = self.dirty.get_mut(*row as usize) {
                        *dirty = true;
                    }
                }
            }
            Redraw::All => {
                self.dirty = vec![false; self.view.size().0 as usize];
                self.full = true;
            }
        }
    }
}

fn shape(shape: CursorShape) -> &'static str {
    match shape {
        CursorShape::Default | CursorShape::BlinkingBlock | CursorShape::SteadyBlock => "block",
        CursorShape::BlinkingUnderline | CursorShape::SteadyUnderline => "underline",
        CursorShape::BlinkingBar | CursorShape::SteadyBar => "bar",
    }
}

/// A row as stretches of one style. Trailing blanks in the default style
/// are left off, since the page clears a row before drawing it; a wide cell
/// is a segment of its own and its spacer is skipped.
fn segments(row: &[Cell]) -> Vec<Segment> {
    let end = row.iter().rposition(|cell| !is_blank(cell)).map_or(0, |last| last + 1);
    let mut out: Vec<Segment> = Vec::new();
    let mut col = 0;
    while col < end {
        let cell = &row[col];
        let (fg, bg, flags) = (pack(cell.fg), pack(cell.bg), flags(cell));
        if is_wide(cell) {
            out.push(Segment(col as u16, vec![cell.ch.to_string()], fg, bg, flags | WIDE));
            col += 2;
            continue;
        }
        match out.last_mut() {
            Some(Segment(start, cells, f, b, fl))
                if (*f, *b, *fl) == (fg, bg, flags)
                    && *fl & WIDE == 0
                    && *start as usize + cells.len() == col =>
            {
                cells.push(cell.ch.to_string());
            }
            _ => out.push(Segment(col as u16, vec![cell.ch.to_string()], fg, bg, flags)),
        }
        col += 1;
    }
    out
}

fn is_blank(cell: &Cell) -> bool {
    cell.ch == BLANK && pack(cell.fg) == 0 && pack(cell.bg) == 0 && flags(cell) == 0
}

fn is_wide(cell: &Cell) -> bool {
    cell.ch.chars().next().and_then(UnicodeWidthChar::width) == Some(2)
}

fn flags(cell: &Cell) -> u8 {
    let flag = |on: bool, bit: u8| if on { bit } else { 0 };
    flag(cell.bold, BOLD)
        | flag(cell.italic, ITALIC)
        | flag(cell.underline, UNDERLINE)
        | flag(cell.reverse, REVERSE)
}

fn pack(color: Color) -> u32 {
    match color {
        Color::Default => 0,
        Color::Idx(i) => 1 << 24 | i as u32,
        Color::Rgb(r, g, b) => 2 << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{CellRun, CellSpan, MouseTracking, Scroll};

    const PANE: PaneId = PaneId(7);

    fn cell(ch: &str) -> Cell {
        Cell { ch: ch.into(), ..Cell::default() }
    }

    fn row(text: &str, cols: usize) -> Vec<Cell> {
        let mut cells: Vec<Cell> = text.chars().map(|c| cell(&c.to_string())).collect();
        cells.resize(cols, Cell::default());
        cells
    }

    fn rows(cursor: Cursor, lines: &[&str], cols: u16) -> ServerMsg {
        let grid: Vec<Vec<Cell>> = lines.iter().map(|l| row(l, cols as usize)).collect();
        ServerMsg::PaneRows {
            pane: PANE,
            rows: lines.len() as u16,
            cols,
            runs: argus_protocol::grid_runs(&grid),
            cursor,
            mouse: MouseTracking::default(),
            alternate_screen: false,
            keyboard: Default::default(),
        }
    }

    fn damage(scroll: Option<Scroll>, runs: Vec<CellRun>, cursor: Cursor) -> ServerMsg {
        ServerMsg::RowDamage {
            pane: PANE,
            scroll,
            runs,
            cursor,
            mouse: MouseTracking::default(),
            alternate_screen: false,
            keyboard: Default::default(),
        }
    }

    fn text(segments: &[Segment]) -> String {
        segments.iter().flat_map(|s| s.1.iter().cloned()).collect()
    }

    fn lines(frame: &Frame) -> Vec<(u16, String)> {
        frame.lines.iter().map(|(row, segs)| (*row, text(segs))).collect()
    }

    #[test]
    fn a_snapshot_sends_every_row_then_damage_only_the_rows_it_touched() {
        let mut screen = Screen::new(PANE);
        screen.receive(&rows(Cursor::default(), &["one", "two", "three"], 8));
        let first = screen.take_frame().unwrap();
        assert!(first.full);
        assert_eq!(lines(&first), [(0, "one".into()), (1, "two".into()), (2, "three".into())]);

        let run = CellRun::encode(1, 0, &row("TWO", 3));
        screen.receive(&damage(None, vec![run], Cursor::default()));
        let next = screen.take_frame().unwrap();
        assert!(!next.full);
        assert_eq!(lines(&next), [(1, "TWO".into())]);
    }

    #[test]
    fn a_scroll_redraws_its_region() {
        let mut screen = Screen::new(PANE);
        screen.receive(&rows(Cursor::default(), &["a", "b", "c", "d"], 4));
        screen.take_frame();

        let scroll = Scroll { top: 0, bottom: 3, up: 1 };
        let run = CellRun::encode(2, 0, &row("new", 3));
        screen.receive(&damage(Some(scroll), vec![run], Cursor::default()));
        let frame = screen.take_frame().unwrap();
        assert_eq!(lines(&frame), [(0, "b".into()), (1, "c".into()), (2, "new".into())]);
    }

    #[test]
    fn a_wide_character_is_flagged_and_its_spacer_is_not_sent() {
        let mut screen = Screen::new(PANE);
        let mut cells = vec![cell("a"), cell("日"), cell(" "), cell("b")];
        cells.resize(6, Cell::default());
        let msg = ServerMsg::PaneSnapshot {
            pane: PANE,
            rows: 1,
            cols: 6,
            cells: vec![cells],
            cursor: Cursor::default(),
            mouse: MouseTracking::default(),
            alternate_screen: false,
            keyboard: Default::default(),
        };
        screen.receive(&msg);
        let frame = screen.take_frame().unwrap();
        let segs = &frame.lines[0].1;
        assert_eq!(segs.len(), 3, "{segs:?}");
        assert_eq!(segs[1], Segment(1, vec!["日".into()], 0, 0, WIDE));
        assert_eq!(segs[2], Segment(3, vec!["b".into()], 0, 0, 0));
    }

    #[test]
    fn trailing_blanks_are_trimmed_but_coloured_ones_are_kept() {
        let lit = Cell { bg: Color::Idx(4), ..Cell::default() };
        let mut cells = row("hi", 6);
        cells[4] = lit;
        let segs = segments(&cells);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0], Segment(0, vec!["h".into(), "i".into(), " ".into(), " ".into()], 0, 0, 0));
        assert_eq!(segs[1], Segment(4, vec![" ".into()], 0, 1 << 24 | 4, 0));
        assert!(segments(&row("", 6)).is_empty());
    }

    #[test]
    fn a_cursor_that_only_moved_sends_a_frame_with_no_lines() {
        let mut screen = Screen::new(PANE);
        screen.receive(&rows(Cursor::default(), &["x"], 4));
        screen.take_frame();
        let moved = Cursor { col: 3, ..Cursor::default() };
        screen.receive(&damage(None, Vec::new(), moved));
        let frame = screen.take_frame().unwrap();
        assert!(frame.lines.is_empty());
        assert_eq!(frame.cursor.col, 3);
    }

    #[test]
    fn nothing_changed_is_no_frame() {
        let mut screen = Screen::new(PANE);
        assert!(screen.take_frame().is_none(), "nothing received yet");
        screen.receive(&rows(Cursor::default(), &["x"], 4));
        screen.take_frame();
        assert!(screen.take_frame().is_none());
        screen.receive(&damage(None, Vec::new(), Cursor::default()));
        assert!(screen.take_frame().is_none(), "the same cursor is no change");
    }

    #[test]
    fn a_fresh_grid_replaces_the_dimensions() {
        let mut screen = Screen::new(PANE);
        screen.receive(&rows(Cursor::default(), &["x"], 4));
        screen.take_frame();
        screen.receive(&rows(Cursor::default(), &["a", "b"], 9));
        let frame = screen.take_frame().unwrap();
        assert_eq!((frame.rows, frame.cols, frame.full), (2, 9, true));
    }

    #[test]
    fn the_per_cell_forms_from_an_older_daemon_are_read_too() {
        let mut screen = Screen::new(PANE);
        let snapshot = ServerMsg::PaneSnapshot {
            pane: PANE,
            rows: 2,
            cols: 4,
            cells: vec![row("ab", 4), row("cd", 4)],
            cursor: Cursor::default(),
            mouse: MouseTracking::default(),
            alternate_screen: false,
            keyboard: Default::default(),
        };
        screen.receive(&snapshot);
        screen.take_frame();
        let spans = vec![CellSpan { row: 1, col: 1, cells: vec![cell("X")] }];
        let damage = ServerMsg::Damage {
            pane: PANE,
            spans,
            cursor: Cursor::default(),
            mouse: MouseTracking::default(),
            alternate_screen: false,
            keyboard: Default::default(),
        };
        screen.receive(&damage);
        assert_eq!(lines(&screen.take_frame().unwrap()), [(1, "cX".into())]);
    }

    #[test]
    fn another_panes_screen_is_left_alone() {
        let mut screen = Screen::new(PaneId(99));
        assert!(screen.receive(&rows(Cursor::default(), &["x"], 4)).is_none());
        assert!(screen.take_frame().is_none());
    }

    #[test]
    fn a_refresh_sends_every_row_again() {
        let mut screen = Screen::new(PANE);
        screen.receive(&rows(Cursor::default(), &["x", "y"], 4));
        screen.take_frame();
        screen.refresh();
        let frame = screen.take_frame().unwrap();
        assert!(frame.full);
        assert_eq!(frame.lines.len(), 2);
    }

    #[test]
    fn a_backward_drag_copies_in_reading_order_without_trailing_blanks() {
        let mut screen = Screen::new(PANE);
        screen.receive(&rows(Cursor::default(), &["abcdefgh", "ijkl"], 8));
        assert_eq!(screen.text((1, 3), (0, 1)), "bcdefgh\nijkl");
    }

    #[test]
    fn a_wide_character_is_copied_once() {
        let mut screen = Screen::new(PANE);
        let mut cells = vec![cell("a"), cell("日"), cell(" "), cell("b")];
        cells.resize(6, Cell::default());
        screen.receive(&ServerMsg::PaneSnapshot {
            pane: PANE,
            rows: 1,
            cols: 6,
            cells: vec![cells],
            cursor: Cursor::default(),
            mouse: MouseTracking::default(),
            alternate_screen: false,
            keyboard: Default::default(),
        });
        assert_eq!(screen.text((0, 0), (0, 5)), "a日b");
    }

    #[test]
    fn a_parked_view_draws_its_history_and_holds_still_under_live_output() {
        let mut screen = Screen::new(PANE);
        screen.receive(&rows(Cursor::default(), &["live"], 8));
        screen.take_frame();
        let _ = screen.gesture(|view| view.scroll(-3));
        let history = ServerMsg::ScrollbackRows { pane: PANE, offset: 3, depth: 40, cells: vec![row("old", 8)] };
        screen.receive(&history);
        let frame = screen.take_frame().unwrap();
        assert_eq!(frame.parked, Some(Parked { offset: 3, depth: 40 }));
        assert!(!frame.cursor.visible);
        assert_eq!(frame.lines[0].1[0].1.concat(), "old");
        assert_eq!(screen.text((0, 0), (0, 7)), "old");

        screen.receive(&damage(None, vec![CellRun::encode(0, 0, &row("x", 1))], Cursor::default()));
        assert!(screen.take_frame().is_none(), "the parked rows do not move");

        let _ = screen.gesture(PaneView::live);
        let frame = screen.take_frame().unwrap();
        assert!(frame.full && frame.parked.is_none());
        assert_eq!(frame.lines[0].1[0].1.concat(), "xive");
    }

    #[test]
    fn a_parked_view_survives_a_screen_that_grew() {
        // The parked rows were read at the old size; a frame drawn before
        // the re-read must still cover every row.
        let mut screen = Screen::new(PANE);
        screen.receive(&rows(Cursor::default(), &["live"], 8));
        let _ = screen.gesture(|view| view.scroll(-3));
        screen.take_frame();
        let resized = screen.receive(&rows(Cursor::default(), &["a", "b", "c"], 12)).unwrap();
        assert!(matches!(resized.send, Some(argus_protocol::ClientMsg::Scrollback { offset: 3, .. })));
        let frame = screen.take_frame().unwrap();
        assert_eq!((frame.rows, frame.lines.len()), (3, 3));
    }
}
