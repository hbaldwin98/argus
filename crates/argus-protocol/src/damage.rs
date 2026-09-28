//! What changed on a pane's screen, compactly: runs of cells that share a
//! look, and regions that scrolled rather than changed.
//!
//! The daemon diffs each frame against the grid its client holds and sends
//! the difference in these; the client applies it to the grid it has. A
//! client that did not greet with `CELL_RUNS` is sent the older per-cell
//! form (`cell::CellSpan`), built from the same runs.

use std::hash::{Hash, Hasher};

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::cell::{Cell, CellSpan, Color, BLANK};

/// Stretches of unchanged cells shorter than this are sent anyway, inside
/// the run either side of them: a run costs more to start than a few cells
/// cost to repeat.
const MERGE_GAP: usize = 8;

/// A cell's look apart from its grapheme, shared by a run of cells.
///
/// Three small integers on the wire rather than a record: a styled screen
/// has a few of these a row, and a record would name its fields each time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "(u32, u32, u8)", into = "(u32, u32, u8)")]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub flags: u8,
}

const BOLD: u8 = 1;
const ITALIC: u8 = 2;
const UNDERLINE: u8 = 4;
const REVERSE: u8 = 8;

impl Style {
    pub fn of(cell: &Cell) -> Style {
        let flag = |on: bool, bit: u8| if on { bit } else { 0 };
        Style {
            fg: cell.fg,
            bg: cell.bg,
            flags: flag(cell.bold, BOLD)
                | flag(cell.italic, ITALIC)
                | flag(cell.underline, UNDERLINE)
                | flag(cell.reverse, REVERSE),
        }
    }

    fn cell(self, ch: CompactString) -> Cell {
        Cell {
            ch,
            fg: self.fg,
            bg: self.bg,
            bold: self.flags & BOLD != 0,
            italic: self.flags & ITALIC != 0,
            underline: self.flags & UNDERLINE != 0,
            reverse: self.flags & REVERSE != 0,
        }
    }
}

impl From<Style> for (u32, u32, u8) {
    fn from(style: Style) -> Self {
        (pack(style.fg), pack(style.bg), style.flags)
    }
}

impl From<(u32, u32, u8)> for Style {
    fn from((fg, bg, flags): (u32, u32, u8)) -> Self {
        Style {
            fg: unpack(fg),
            bg: unpack(bg),
            flags,
        }
    }
}

/// A colour in one integer: the kind in the top byte, the value below.
fn pack(color: Color) -> u32 {
    match color {
        Color::Default => 0,
        Color::Idx(i) => 1 << 24 | i as u32,
        Color::Rgb(r, g, b) => 2 << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32,
    }
}

/// A kind this build does not know reads as the default colour rather than
/// failing the frame it came in.
fn unpack(packed: u32) -> Color {
    match packed >> 24 {
        1 => Color::Idx(packed as u8),
        2 => Color::Rgb((packed >> 16) as u8, (packed >> 8) as u8, packed as u8),
        _ => Color::Default,
    }
}

/// A horizontal run of cells starting at (row, col).
///
/// Positional on the wire, like `Style`: a frame is mostly runs, and field
/// names would cost more than a short line's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RunWire", into = "RunWire")]
pub struct CellRun {
    pub row: u16,
    pub col: u16,
    /// The graphemes of the run's cells, one after another.
    pub text: String,
    /// How many chars each cell's grapheme holds, when any holds other than
    /// one — a combining mark. Empty otherwise, which is nearly always.
    pub lens: Vec<u16>,
    /// The cells' looks as (count, style), in order, covering `text`.
    pub styles: Vec<(u16, Style)>,
    /// Default cells after the text, so a cleared stretch costs a count
    /// rather than a cell each.
    pub blank: u16,
}

#[derive(Serialize, Deserialize)]
struct RunWire(u16, u16, String, Vec<u16>, Vec<(u16, Style)>, u16);

impl From<CellRun> for RunWire {
    fn from(run: CellRun) -> Self {
        RunWire(run.row, run.col, run.text, run.lens, run.styles, run.blank)
    }
}

impl From<RunWire> for CellRun {
    fn from(RunWire(row, col, text, lens, styles, blank): RunWire) -> Self {
        CellRun {
            row,
            col,
            text,
            lens,
            styles,
            blank,
        }
    }
}

impl CellRun {
    pub fn encode(row: u16, col: u16, cells: &[Cell]) -> CellRun {
        let default = Cell::default();
        let blank = cells.iter().rev().take_while(|c| **c == default).count();
        let body = &cells[..cells.len() - blank];

        let text = body.iter().map(|c| c.ch.as_str()).collect();
        let lens = if body.iter().all(|c| c.ch.chars().count() == 1) {
            Vec::new()
        } else {
            body.iter().map(|c| c.ch.chars().count() as u16).collect()
        };
        let mut styles: Vec<(u16, Style)> = Vec::new();
        for cell in body {
            let style = Style::of(cell);
            match styles.last_mut() {
                Some((count, last)) if *last == style => *count += 1,
                _ => styles.push((1, style)),
            }
        }

        CellRun {
            row,
            col,
            text,
            lens,
            styles,
            blank: blank as u16,
        }
    }

    /// The run's cells. Text that runs short of its styles leaves blanks;
    /// nothing a run says can make this panic.
    pub fn cells(&self) -> Vec<Cell> {
        let styled: usize = self.styles.iter().map(|(count, _)| *count as usize).sum();
        let mut cells = Vec::with_capacity(styled + self.blank as usize);
        let mut chars = self.text.chars();
        let mut index = 0;
        for &(count, style) in &self.styles {
            for _ in 0..count {
                let len = self.lens.get(index).map_or(1, |len| *len as usize);
                let ch: CompactString = chars.by_ref().take(len).collect();
                cells.push(style.cell(if ch.is_empty() { BLANK } else { ch }));
                index += 1;
            }
        }
        cells.extend(std::iter::repeat_n(Cell::default(), self.blank as usize));
        cells
    }

    /// The same cells in the older per-cell form.
    pub fn span(&self) -> CellSpan {
        CellSpan {
            row: self.row,
            col: self.col,
            cells: self.cells(),
        }
    }

    /// Writes the run's cells into `grid`, dropping any that fall outside it.
    pub fn apply(&self, grid: &mut [Vec<Cell>]) {
        let Some(row) = grid.get_mut(self.row as usize) else {
            return;
        };
        let start = self.col as usize;
        for (slot, cell) in row.iter_mut().skip(start).zip(self.cells()) {
            *slot = cell;
        }
    }
}

/// Rows `top..bottom` moved up by `up`, as the terminal moved them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scroll {
    pub top: u16,
    pub bottom: u16,
    pub up: u16,
}

impl Scroll {
    /// Moves the region as the terminal did: what leaves its top is gone,
    /// and the rows it opens at the bottom are blank until a run fills
    /// them. A scroll that does not fit the grid moves nothing.
    pub fn apply(&self, grid: &mut [Vec<Cell>]) {
        let (top, bottom, up) = (self.top as usize, self.bottom as usize, self.up as usize);
        if up == 0 || bottom > grid.len() || top + up > bottom {
            return;
        }
        let region = &mut grid[top..bottom];
        region.rotate_left(up);
        let opened = region.len() - up;
        for row in &mut region[opened..] {
            row.fill(Cell::default());
        }
    }
}

/// What a client holding `prev` needs to reach `cur`: the scroll that moves
/// most of it into place, when one does, and the runs left over after it.
/// `prev` is moved along with the scroll, since that is the grid the runs
/// apply to. With no `prev` every cell is sent.
pub fn damage(
    prev: Option<&mut Vec<Vec<Cell>>>,
    cur: &[Vec<Cell>],
) -> (Option<Scroll>, Vec<CellRun>) {
    let Some(prev) = prev else {
        return (None, changed_runs(None, cur));
    };
    let scroll = detect_scroll(prev, cur);
    if let Some(scroll) = scroll {
        scroll.apply(prev);
    }
    (scroll, changed_runs(Some(prev), cur))
}

/// A whole grid as runs, for a client starting from nothing: every cell
/// that is not a default blank.
pub fn grid_runs(cur: &[Vec<Cell>]) -> Vec<CellRun> {
    let blank: Vec<Vec<Cell>> = cur
        .iter()
        .map(|row| vec![Cell::default(); row.len()])
        .collect();
    changed_runs(Some(&blank), cur)
}

/// The grid a client starting from nothing gets from `runs`.
pub fn grid_from_runs(rows: u16, cols: u16, runs: &[CellRun]) -> Vec<Vec<Cell>> {
    let mut grid = vec![vec![Cell::default(); cols as usize]; rows as usize];
    for run in runs {
        run.apply(&mut grid);
    }
    grid
}

/// Runs of the cells in `cur` that differ from `prev`, a short unchanged
/// gap riding along inside a run rather than splitting it. A row `prev`
/// does not have, or has at another width, is sent whole.
fn changed_runs(prev: Option<&[Vec<Cell>]>, cur: &[Vec<Cell>]) -> Vec<CellRun> {
    let mut runs = Vec::new();
    for (r, row) in cur.iter().enumerate() {
        let before = prev.and_then(|p| p.get(r)).filter(|p| p.len() == row.len());
        let changed = |c: usize| before.is_none_or(|b| b[c] != row[c]);

        let mut c = 0;
        while c < row.len() {
            if !changed(c) {
                c += 1;
                continue;
            }
            let start = c;
            let mut end = c + 1;
            let mut gap = 0;
            c += 1;
            while c < row.len() && gap < MERGE_GAP {
                if changed(c) {
                    end = c + 1;
                    gap = 0;
                } else {
                    gap += 1;
                }
                c += 1;
            }
            runs.push(CellRun::encode(r as u16, start as u16, &row[start..end]));
            c = end;
        }
    }
    runs
}

/// The scroll that saves the most rows between `prev` and `cur`, if any
/// saves one.
///
/// Rows are compared by hash. For each distance, every stretch of rows that
/// sit that far below where they were is a candidate region; what it saves
/// is the rows in it that would otherwise be sent, less the rows its
/// opened bottom blanks that were fine where they were. A region rather
/// than the whole screen, because a program with a fixed footer — an
/// agent's input box — scrolls everything above it and nothing below.
fn detect_scroll(prev: &[Vec<Cell>], cur: &[Vec<Cell>]) -> Option<Scroll> {
    let rows = cur.len();
    if prev.len() != rows || rows < 2 {
        return None;
    }
    let before: Vec<u64> = prev.iter().map(|row| row_hash(row)).collect();
    let after: Vec<u64> = cur.iter().map(|row| row_hash(row)).collect();
    let blank = row_hash(&vec![Cell::default(); cur[0].len()]);

    let mut best: Option<(usize, Scroll)> = None;
    for up in 1..rows {
        let mut r = 0;
        while r + up < rows {
            if after[r] != before[r + up] {
                r += 1;
                continue;
            }
            let top = r;
            while r + up < rows && after[r] == before[r + up] {
                r += 1;
            }
            let moved = top..r;
            let opened = r..r + up;
            let saved = moved.filter(|&i| after[i] != before[i]).count();
            let lost = opened
                .filter(|&i| after[i] == before[i] && after[i] != blank)
                .count();
            let gain = saved.saturating_sub(lost);
            if gain > 0 && best.is_none_or(|(most, _)| gain > most) {
                let scroll = Scroll {
                    top: top as u16,
                    bottom: (r + up) as u16,
                    up: up as u16,
                };
                best = Some((gain, scroll));
            }
        }
    }
    best.map(|(_, scroll)| scroll)
}

fn row_hash(row: &[Cell]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    row.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use compact_str::ToCompactString;

    fn row(text: &str, width: usize) -> Vec<Cell> {
        let mut cells: Vec<Cell> = text
            .chars()
            .map(|c| Cell {
                ch: c.to_compact_string(),
                ..Default::default()
            })
            .collect();
        cells.resize(width, Cell::default());
        cells
    }

    fn grid(lines: &[&str], width: usize) -> Vec<Vec<Cell>> {
        lines.iter().map(|line| row(line, width)).collect()
    }

    /// What a client holding `prev` ends up with after the damage for
    /// `cur`: the whole point, whatever the damage looks like.
    fn applied(prev: &[Vec<Cell>], cur: &[Vec<Cell>]) -> Vec<Vec<Cell>> {
        let mut daemon = prev.to_vec();
        let (scroll, runs) = damage(Some(&mut daemon), cur);
        let mut client = prev.to_vec();
        if let Some(scroll) = scroll {
            scroll.apply(&mut client);
        }
        for run in &runs {
            run.apply(&mut client);
        }
        client
    }

    fn numbered(from: usize, count: usize, width: usize) -> Vec<Vec<Cell>> {
        (from..from + count)
            .map(|n| row(&format!("{n:>4} the quick brown fox {}", n * 7919), width))
            .collect()
    }

    #[test]
    fn a_run_reads_back_as_the_cells_it_was_made_from() {
        let mut cells = row("héllo 世", 12);
        cells[0].bold = true;
        cells[1].fg = Color::Rgb(200, 100, 50);
        cells[2].bg = Color::Idx(4);
        cells[3].reverse = true;
        cells[4].italic = true;
        cells[5].underline = true;
        cells[8] = Cell {
            ch: "e\u{301}".into(),
            ..Default::default()
        };

        let run = CellRun::encode(3, 2, &cells);
        let bytes = rmp_serde::to_vec_named(&run).unwrap();
        let read: CellRun = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(read.cells(), cells);
        assert_eq!((read.row, read.col), (3, 2));
    }

    #[test]
    fn a_cleared_stretch_is_a_count_not_cells() {
        let run = CellRun::encode(0, 0, &row("ab", 200));
        assert_eq!(run.text, "ab");
        assert_eq!(run.blank, 198);
        assert_eq!(run.cells().len(), 200);
    }

    #[test]
    fn plain_text_needs_no_lengths() {
        assert!(CellRun::encode(0, 0, &row("plain", 5)).lens.is_empty());
    }

    #[test]
    fn a_run_that_says_too_little_reads_as_blanks_rather_than_panicking() {
        let run = CellRun {
            row: 0,
            col: 0,
            text: "a".into(),
            lens: vec![4, 4],
            styles: vec![(3, Style::default())],
            blank: 0,
        };
        assert_eq!(run.cells().len(), 3);
    }

    #[test]
    fn a_run_past_the_grid_writes_only_what_fits() {
        let mut g = grid(&["abc"], 3);
        CellRun::encode(0, 2, &row("XYZ", 3)).apply(&mut g);
        CellRun::encode(9, 0, &row("XYZ", 3)).apply(&mut g);
        assert_eq!(g, grid(&["abX"], 3));
    }

    #[test]
    fn an_unchanged_screen_costs_nothing() {
        let prev = numbered(0, 10, 40);
        let mut daemon = prev.clone();
        let (scroll, runs) = damage(Some(&mut daemon), &prev);
        assert_eq!(scroll, None);
        assert!(runs.is_empty());
    }

    #[test]
    fn a_screen_that_scrolled_one_line_sends_the_scroll_and_the_new_line() {
        let prev = numbered(0, 50, 200);
        let cur = numbered(1, 50, 200);
        let mut daemon = prev.clone();

        let (scroll, runs) = damage(Some(&mut daemon), &cur);

        assert_eq!(
            scroll,
            Some(Scroll {
                top: 0,
                bottom: 50,
                up: 1
            })
        );
        assert_eq!(runs.len(), 1, "{runs:?}");
        assert_eq!(runs[0].row, 49);
        assert_eq!(applied(&prev, &cur), cur);
    }

    #[test]
    fn scrolling_above_a_fixed_footer_scrolls_only_the_region() {
        // An agent's output scrolling above its input box.
        let mut prev = numbered(0, 40, 80);
        prev.extend(grid(&["─────", "> type here", "─────"], 80));
        let mut cur = numbered(3, 40, 80);
        cur.extend(grid(&["─────", "> type here", "─────"], 80));
        let mut daemon = prev.clone();

        let (scroll, runs) = damage(Some(&mut daemon), &cur);

        assert_eq!(
            scroll,
            Some(Scroll {
                top: 0,
                bottom: 40,
                up: 3
            })
        );
        assert!(runs.iter().all(|r| (37..40).contains(&r.row)), "{runs:?}");
        assert_eq!(applied(&prev, &cur), cur);
    }

    #[test]
    fn a_blank_screen_filling_in_is_not_mistaken_for_a_scroll() {
        let prev = grid(&["", "", "", ""], 20);
        let cur = grid(&["$ ls", "", "", ""], 20);
        let mut daemon = prev.clone();
        let (scroll, _) = damage(Some(&mut daemon), &cur);
        assert_eq!(scroll, None);
        assert_eq!(applied(&prev, &cur), cur);
    }

    #[test]
    fn whatever_the_screen_did_the_client_ends_up_with_it() {
        // Scrolls up, down, by several, partial rewrites, clears: the
        // damage must reproduce the grid every time.
        let base = numbered(0, 12, 30);
        let mut cases: Vec<Vec<Vec<Cell>>> = vec![
            numbered(5, 12, 30),
            numbered(11, 12, 30),
            numbered(100, 12, 30),
            grid(&[""; 12], 30),
        ];
        let mut down = numbered(0, 12, 30);
        down.rotate_right(2);
        cases.push(down);
        let mut rewritten = numbered(1, 12, 30);
        rewritten[4] = row("something else entirely", 30);
        rewritten[11] = row("", 30);
        cases.push(rewritten);

        for cur in cases {
            assert_eq!(applied(&base, &cur), cur);
        }
    }

    #[test]
    fn with_no_previous_grid_every_cell_is_sent() {
        let cur = numbered(0, 3, 10);
        let (scroll, runs) = damage(None, &cur);
        assert_eq!(scroll, None);
        let mut client = grid(&["xxxxxxxxxx"; 3], 10);
        for run in &runs {
            run.apply(&mut client);
        }
        assert_eq!(client, cur);
    }

    #[test]
    fn a_whole_grid_rebuilds_from_its_runs() {
        let mut cur = numbered(0, 5, 30);
        cur.push(row("", 30));
        let runs = grid_runs(&cur);
        assert!(runs.iter().all(|r| r.row != 5), "a blank row sends nothing");
        assert_eq!(grid_from_runs(6, 30, &runs), cur);
    }

    #[test]
    fn a_scroll_that_does_not_fit_moves_nothing() {
        let mut g = grid(&["a", "b"], 1);
        Scroll { top: 0, bottom: 5, up: 1 }.apply(&mut g);
        Scroll { top: 1, bottom: 2, up: 2 }.apply(&mut g);
        assert_eq!(g, grid(&["a", "b"], 1));
    }

    #[test]
    fn a_one_line_scroll_of_a_full_screen_costs_about_one_line() {
        // Measured before: 413 KB of per-cell damage for this, for the
        // ~150 bytes the child wrote.
        let prev = numbered(0, 50, 200);
        let cur = numbered(1, 50, 200);
        let mut daemon = prev.clone();
        let (scroll, runs) = damage(Some(&mut daemon), &cur);
        let bytes = rmp_serde::to_vec_named(&(scroll, runs)).unwrap().len();
        assert!(bytes < 200, "{bytes} bytes");
    }

    #[test]
    fn a_full_screen_of_text_costs_its_text_and_a_little_a_row() {
        // Measured before: 620 KB for this grid as per-cell records.
        let cur = numbered(0, 50, 200);
        let runs = grid_runs(&cur);
        let text: usize = runs.iter().map(|r| r.text.len()).sum();
        let bytes = rmp_serde::to_vec_named(&runs).unwrap().len();
        assert!(bytes < text + 16 * cur.len(), "{bytes} bytes for {text} of text");
    }
}
