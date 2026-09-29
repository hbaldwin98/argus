//! A pane's screen as a phone draws it: the grid the daemon streams, kept
//! here, and sent on as the rows that changed since the last look.
//!
//! The phone never resizes a pane — it draws the grid at whatever size the
//! desktop gave it — so this never sends `Resize`, and the daemon's size
//! reconciliation never hears of the phone at all. A terminal can redraw
//! far faster than a phone on a cellular link wants to be told, so changes
//! are gathered and sent at most every [`FLUSH`], a row at a time.

use std::hash::{Hash, Hasher};
use std::time::Duration;

use argus_protocol::{grid_from_runs, Cell, CellRun, Color, Cursor, Scroll};
use serde::Serialize;

/// The most often a watched screen's changes are sent on.
pub const FLUSH: Duration = Duration::from_millis(100);

/// One watched pane's screen.
pub struct Screen {
    rows: u16,
    cols: u16,
    grid: Vec<Vec<Cell>>,
    cursor: Cursor,
    /// A hash of each row as last sent, to send only the rows that changed.
    sent: Vec<u64>,
    /// Whether anything changed since the last send.
    dirty: bool,
    /// Whether the next send is the whole screen: a phone started watching
    /// or fell behind.
    fresh: bool,
}

/// What a phone is sent of a screen.
#[derive(Debug, Clone, Serialize)]
pub struct ScreenUpdate {
    pub rows: u16,
    pub cols: u16,
    pub fresh: bool,
    pub cursor: Option<(u16, u16)>,
    /// Each changed row by number, as runs of `[text, fg, bg, flags]`.
    pub lines: Vec<(u16, Vec<Run>)>,
}

/// A stretch of a row in one look: its text, foreground and background as
/// CSS colours (`None` for the page's own), and bold, italic, underline and
/// reverse as the bits 1, 2, 4 and 8.
pub type Run = (String, Option<String>, Option<String>, u8);

impl Screen {
    /// A screen from the whole grid the daemon sends on subscribing.
    pub fn new(rows: u16, cols: u16, runs: &[CellRun], cursor: Cursor) -> Screen {
        Screen {
            rows,
            cols,
            grid: grid_from_runs(rows, cols, runs),
            cursor,
            sent: Vec::new(),
            dirty: true,
            fresh: true,
        }
    }

    /// The whole grid again: the daemon catching this connection up.
    pub fn replace(&mut self, rows: u16, cols: u16, runs: &[CellRun], cursor: Cursor) {
        self.rows = rows;
        self.cols = cols;
        self.grid = grid_from_runs(rows, cols, runs);
        self.cursor = cursor;
        self.dirty = true;
        self.fresh = true;
    }

    /// What changed, as the terminal changed it: the scroll first, then the
    /// runs written over it.
    pub fn damage(&mut self, scroll: Option<Scroll>, runs: &[CellRun], cursor: Cursor) {
        if let Some(scroll) = scroll {
            scroll.apply(&mut self.grid);
        }
        for run in runs {
            run.apply(&mut self.grid);
        }
        self.cursor = cursor;
        self.dirty = true;
    }

    /// Sends the whole screen next time, for a phone that has none of it.
    pub fn refresh(&mut self) {
        self.fresh = true;
        self.dirty = true;
    }

    /// What has changed since the last call, if anything.
    pub fn flush(&mut self) -> Option<ScreenUpdate> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        let fresh = std::mem::take(&mut self.fresh) || self.sent.len() != self.grid.len();
        let hashes: Vec<u64> = self.grid.iter().map(|row| row_hash(row)).collect();
        let lines = self
            .grid
            .iter()
            .enumerate()
            .filter(|(i, _)| fresh || self.sent.get(*i) != Some(&hashes[*i]))
            .map(|(i, row)| (i as u16, runs_of(row)))
            .collect::<Vec<_>>();
        self.sent = hashes;
        if lines.is_empty() && !fresh {
            return None;
        }
        Some(ScreenUpdate {
            rows: self.rows,
            cols: self.cols,
            fresh,
            cursor: self.cursor.visible.then_some((self.cursor.row, self.cursor.col)),
            lines,
        })
    }
}

fn row_hash(row: &[Cell]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for cell in row {
        cell.ch.as_str().hash(&mut hasher);
        cell.fg.hash(&mut hasher);
        cell.bg.hash(&mut hasher);
        (cell.bold, cell.italic, cell.underline, cell.reverse).hash(&mut hasher);
    }
    hasher.finish()
}

/// A row as runs of cells that look alike, with trailing blanks dropped.
fn runs_of(row: &[Cell]) -> Vec<Run> {
    let end = row
        .iter()
        .rposition(|c| !c.ch.as_str().trim().is_empty() || c.bg != Color::Default || c.reverse)
        .map_or(0, |i| i + 1);
    let mut runs: Vec<Run> = Vec::new();
    for cell in &row[..end] {
        let look = (css(cell.fg), css(cell.bg), flags(cell));
        match runs.last_mut() {
            Some((text, fg, bg, f)) if (fg.clone(), bg.clone(), *f) == look => text.push_str(&cell.ch),
            _ => runs.push((cell.ch.to_string(), look.0, look.1, look.2)),
        }
    }
    runs
}

fn flags(cell: &Cell) -> u8 {
    u8::from(cell.bold)
        | u8::from(cell.italic) << 1
        | u8::from(cell.underline) << 2
        | u8::from(cell.reverse) << 3
}

/// A terminal colour as CSS. The sixteen named colours are the page's to
/// choose, so they stay variables its theme sets; the rest are fixed.
fn css(color: Color) -> Option<String> {
    match color {
        Color::Default => None,
        Color::Idx(i) if i < 16 => Some(format!("var(--ansi-{i})")),
        Color::Idx(i) if i < 232 => {
            let n = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Some(hex(level(n / 36), level(n / 6 % 6), level(n % 6)))
        }
        Color::Idx(i) => {
            let v = 8 + (i - 232) * 10;
            Some(hex(v, v, v))
        }
        Color::Rgb(r, g, b) => Some(hex(r, g, b)),
    }
}

fn hex(r: u8, g: u8, b: u8) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// What a key on the phone's key bar types. Only these: the bar exists to
/// answer a harness's menus and prompts, not to be a keyboard.
pub fn key_bytes(key: &str) -> Option<&'static [u8]> {
    Some(match key {
        "enter" => b"\r",
        "esc" => b"\x1b",
        "tab" => b"\t",
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "left" => b"\x1b[D",
        "right" => b"\x1b[C",
        "space" => b" ",
        "backspace" => b"\x7f",
        "y" => b"y",
        "n" => b"n",
        "1" => b"1",
        "2" => b"2",
        "3" => b"3",
        "4" => b"4",
        "5" => b"5",
        "6" => b"6",
        "7" => b"7",
        "8" => b"8",
        "9" => b"9",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(row: u16, text: &str) -> CellRun {
        let cells: Vec<Cell> = text
            .chars()
            .map(|c| Cell {
                ch: c.to_string().into(),
                ..Default::default()
            })
            .collect();
        CellRun::encode(row, 0, &cells)
    }

    #[test]
    fn a_new_screen_is_sent_whole_then_only_what_changed() {
        let mut screen = Screen::new(3, 10, &[run(0, "hello"), run(1, "world")], Cursor::default());
        let first = screen.flush().unwrap();
        assert!(first.fresh);
        assert_eq!(first.lines.len(), 3);
        assert_eq!(first.lines[0].1[0].0, "hello");
        assert!(screen.flush().is_none(), "nothing changed");

        screen.damage(None, &[run(1, "there")], Cursor::default());
        let next = screen.flush().unwrap();
        assert!(!next.fresh);
        assert_eq!(next.lines.len(), 1);
        assert_eq!(next.lines[0].0, 1);
        assert_eq!(next.lines[0].1[0].0, "there");
    }

    #[test]
    fn a_scroll_moves_rows_before_the_runs_land() {
        let mut screen = Screen::new(3, 10, &[run(0, "a"), run(1, "b"), run(2, "c")], Cursor::default());
        screen.flush();
        screen.damage(Some(Scroll { top: 0, bottom: 3, up: 1 }), &[run(2, "d")], Cursor::default());
        let update = screen.flush().unwrap();
        let text: Vec<String> = update.lines.iter().map(|(_, runs)| runs.iter().map(|r| r.0.clone()).collect()).collect();
        assert_eq!(text, ["b", "c", "d"]);
    }

    #[test]
    fn a_phone_that_joins_late_is_sent_the_whole_screen() {
        let mut screen = Screen::new(2, 10, &[run(0, "x")], Cursor::default());
        screen.flush();
        screen.refresh();
        let update = screen.flush().unwrap();
        assert!(update.fresh);
        assert_eq!(update.lines.len(), 2);
    }

    #[test]
    fn the_named_colours_stay_the_pages_and_the_rest_are_fixed() {
        assert_eq!(css(Color::Default), None);
        assert_eq!(css(Color::Idx(1)).as_deref(), Some("var(--ansi-1)"));
        assert_eq!(css(Color::Idx(196)).as_deref(), Some("#ff0000"));
        assert_eq!(css(Color::Idx(232)).as_deref(), Some("#080808"));
        assert_eq!(css(Color::Rgb(1, 2, 3)).as_deref(), Some("#010203"));
    }

    #[test]
    fn only_the_key_bars_keys_type_anything() {
        assert_eq!(key_bytes("enter"), Some(&b"\r"[..]));
        assert_eq!(key_bytes("up"), Some(&b"\x1b[A"[..]));
        assert_eq!(key_bytes("rm -rf"), None);
        assert_eq!(key_bytes("ctrl-c"), None);
    }
}
