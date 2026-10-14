//! A pane's screen as the page draws it: the grid the daemon's damage lands
//! on, and the rows changed since the last frame.
//!
//! The grid is decoded with the protocol's own `grid_from_runs`,
//! `Scroll::apply` and `CellRun::apply`, never a copy in the page, so this
//! window reads damage exactly as the terminal client does.

use argus_protocol::{grid_from_runs, Cell, Color, Cursor, CursorShape, PaneId, ServerMsg, BLANK};
use unicode_width::UnicodeWidthChar;

use crate::page::{Frame, FrameCursor, Segment, BOLD, ITALIC, REVERSE, UNDERLINE, WIDE};

pub struct Screen {
    pane: PaneId,
    rows: u16,
    cols: u16,
    grid: Vec<Vec<Cell>>,
    cursor: Cursor,
    alternate_screen: bool,
    /// Rows changed since the last frame.
    dirty: Vec<bool>,
    /// Every row is to be sent: a new grid, or a page that holds nothing.
    full: bool,
    cursor_moved: bool,
    /// Whether the daemon has sent the grid yet; damage before it has
    /// nothing to land on.
    received: bool,
}

impl Screen {
    pub fn new(pane: PaneId) -> Screen {
        Screen {
            pane,
            rows: 0,
            cols: 0,
            grid: Vec::new(),
            cursor: Cursor::default(),
            alternate_screen: false,
            dirty: Vec::new(),
            full: false,
            cursor_moved: false,
            received: false,
        }
    }

    /// Applies a screen message for this pane. Returns false for anything
    /// else, which is left alone.
    pub fn apply(&mut self, msg: &ServerMsg) -> bool {
        match msg {
            ServerMsg::PaneRows { pane, rows, cols, runs, cursor, alternate_screen, .. } if *pane == self.pane => {
                self.replace(*rows, *cols, grid_from_runs(*rows, *cols, runs));
                self.moved(*cursor, *alternate_screen);
            }
            ServerMsg::PaneSnapshot { pane, rows, cols, cells, cursor, alternate_screen, .. } if *pane == self.pane => {
                self.replace(*rows, *cols, cells.clone());
                self.moved(*cursor, *alternate_screen);
            }
            ServerMsg::RowDamage { pane, scroll, runs, cursor, alternate_screen, .. } if *pane == self.pane => {
                if !self.received {
                    return true;
                }
                // The scroll moves what the runs were diffed against, so it
                // goes first, and every row it touched is redrawn.
                if let Some(scroll) = scroll {
                    scroll.apply(&mut self.grid);
                    let bottom = (scroll.bottom as usize).min(self.dirty.len());
                    let top = (scroll.top as usize).min(bottom);
                    self.dirty[top..bottom].fill(true);
                }
                for run in runs {
                    run.apply(&mut self.grid);
                    self.mark(run.row);
                }
                self.moved(*cursor, *alternate_screen);
            }
            ServerMsg::Damage { pane, spans, cursor, alternate_screen, .. } if *pane == self.pane => {
                if !self.received {
                    return true;
                }
                for span in spans {
                    let Some(row) = self.grid.get_mut(span.row as usize) else {
                        continue;
                    };
                    for (slot, cell) in row.iter_mut().skip(span.col as usize).zip(&span.cells) {
                        *slot = cell.clone();
                    }
                    self.mark(span.row);
                }
                self.moved(*cursor, *alternate_screen);
            }
            _ => return false,
        }
        true
    }

    /// Sends every row with the next frame: the page reloaded, or came back
    /// to this pane, and holds nothing of it.
    pub fn refresh(&mut self) {
        self.full = true;
    }

    /// What changed since the last frame, or `None` when nothing did.
    pub fn take_frame(&mut self) -> Option<Frame> {
        if !self.received {
            return None;
        }
        let any_row = self.full || self.dirty.iter().any(|d| *d);
        if !any_row && !self.cursor_moved {
            return None;
        }
        let full = self.full;
        let lines = (0..self.rows)
            .filter(|row| full || self.dirty[*row as usize])
            .map(|row| (row, segments(&self.grid[row as usize])))
            .collect();
        self.full = false;
        self.cursor_moved = false;
        self.dirty.fill(false);
        Some(Frame {
            pane: self.pane.0,
            rows: self.rows,
            cols: self.cols,
            full,
            cursor: FrameCursor {
                row: self.cursor.row,
                col: self.cursor.col,
                visible: self.cursor.visible,
                shape: shape(self.cursor.shape),
            },
            alternate_screen: self.alternate_screen,
            lines,
        })
    }

    fn replace(&mut self, rows: u16, cols: u16, grid: Vec<Vec<Cell>>) {
        self.rows = rows;
        self.cols = cols;
        self.grid = grid;
        self.grid.resize(rows as usize, vec![Cell::default(); cols as usize]);
        self.dirty = vec![false; rows as usize];
        self.full = true;
        self.received = true;
    }

    fn mark(&mut self, row: u16) {
        if let Some(dirty) = self.dirty.get_mut(row as usize) {
            *dirty = true;
        }
    }

    fn moved(&mut self, cursor: Cursor, alternate_screen: bool) {
        if cursor != self.cursor || alternate_screen != self.alternate_screen {
            self.cursor_moved = true;
        }
        self.cursor = cursor;
        self.alternate_screen = alternate_screen;
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
        screen.apply(&rows(Cursor::default(), &["one", "two", "three"], 8));
        let first = screen.take_frame().unwrap();
        assert!(first.full);
        assert_eq!(lines(&first), [(0, "one".into()), (1, "two".into()), (2, "three".into())]);

        let run = CellRun::encode(1, 0, &row("TWO", 3));
        screen.apply(&damage(None, vec![run], Cursor::default()));
        let next = screen.take_frame().unwrap();
        assert!(!next.full);
        assert_eq!(lines(&next), [(1, "TWO".into())]);
    }

    #[test]
    fn a_scroll_lands_before_the_runs_and_redraws_its_region() {
        let mut screen = Screen::new(PANE);
        screen.apply(&rows(Cursor::default(), &["a", "b", "c", "d"], 4));
        screen.take_frame();

        let scroll = Scroll { top: 0, bottom: 3, up: 1 };
        let run = CellRun::encode(2, 0, &row("new", 3));
        screen.apply(&damage(Some(scroll), vec![run], Cursor::default()));
        let frame = screen.take_frame().unwrap();
        assert_eq!(
            lines(&frame),
            [(0, "b".into()), (1, "c".into()), (2, "new".into())],
            "the run lands on the row the scroll opened"
        );
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
        };
        screen.apply(&msg);
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
        screen.apply(&rows(Cursor::default(), &["x"], 4));
        screen.take_frame();
        let moved = Cursor { col: 3, ..Cursor::default() };
        screen.apply(&damage(None, Vec::new(), moved));
        let frame = screen.take_frame().unwrap();
        assert!(frame.lines.is_empty());
        assert_eq!(frame.cursor.col, 3);
    }

    #[test]
    fn nothing_changed_is_no_frame() {
        let mut screen = Screen::new(PANE);
        assert!(screen.take_frame().is_none(), "nothing received yet");
        screen.apply(&rows(Cursor::default(), &["x"], 4));
        screen.take_frame();
        assert!(screen.take_frame().is_none());
        screen.apply(&damage(None, Vec::new(), Cursor::default()));
        assert!(screen.take_frame().is_none(), "the same cursor is no change");
    }

    #[test]
    fn a_fresh_grid_replaces_the_dimensions() {
        let mut screen = Screen::new(PANE);
        screen.apply(&rows(Cursor::default(), &["x"], 4));
        screen.take_frame();
        screen.apply(&rows(Cursor::default(), &["a", "b"], 9));
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
        };
        screen.apply(&snapshot);
        screen.take_frame();
        let spans = vec![CellSpan { row: 1, col: 1, cells: vec![cell("X")] }];
        let damage = ServerMsg::Damage {
            pane: PANE,
            spans,
            cursor: Cursor::default(),
            mouse: MouseTracking::default(),
            alternate_screen: false,
        };
        screen.apply(&damage);
        assert_eq!(lines(&screen.take_frame().unwrap()), [(1, "cX".into())]);
    }

    #[test]
    fn another_panes_screen_is_left_alone() {
        let mut screen = Screen::new(PaneId(99));
        assert!(!screen.apply(&rows(Cursor::default(), &["x"], 4)));
        assert!(screen.take_frame().is_none());
    }

    #[test]
    fn a_refresh_sends_every_row_again() {
        let mut screen = Screen::new(PANE);
        screen.apply(&rows(Cursor::default(), &["x", "y"], 4));
        screen.take_frame();
        screen.refresh();
        let frame = screen.take_frame().unwrap();
        assert!(frame.full);
        assert_eq!(frame.lines.len(), 2);
    }
}
