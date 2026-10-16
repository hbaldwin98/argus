//! The Workspace's grid: the panes it holds — the Workspace's own first,
//! then those split beside it — where each sits and how big, which pane
//! may join it and why another may not, and the page the rail carries of it.
//!
//! Cells are numbered as `split_grid` numbers its panes: 0 the Workspace's,
//! `n + 1` the page's `Side::Beside(n)`. Every cell holds a pane but the
//! Workspace's when it is the only one, so a side the page names is never
//! an empty split. Which side has the keys is `shown`'s; this keeps that
//! side's number right as cells come and go.

use argus_protocol::{ClientMsg, PaneId};
use serde::Serialize;

use super::Slot;
use crate::page::{Shown, Side};
use split_grid::{Layout, MIN_COLS, MIN_ROWS};

mod split_grid;

pub use split_grid::{Dir, Zone};

/// What the rail carries of the Workspace's grid: the first pane is `Rail.shown`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GridPage {
    /// The panes split beside the first, in the order they were opened:
    /// cell `n + 1` of `grid` is `beside[n]`. None for one the tree no
    /// longer names, so its cell keeps its place without a header.
    pub beside: Vec<Option<Shown>>,
    /// Which cells, the first's too, sit in each row of the grid.
    pub grid: Vec<Vec<u8>>,
    /// Each row's height, in thousandths of the grid's.
    pub heights: Vec<u16>,
    /// Each pane's width in its row, in thousandths of the row's.
    pub widths: Vec<Vec<u16>>,
    /// The cell with the keys, numbered as `grid` numbers them; none while
    /// the keys are off the grid, on the console or an extension's view.
    pub keyed: Option<u8>,
    /// The least a pane of the grid may be, in columns and rows.
    pub min_cols: u16,
    pub min_rows: u16,
}

/// Why a pane may not join the grid. Its words are the note the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The Workspace has no pane of its own to split from.
    NoWorkspacePane,
    /// Another pane would leave one under `MIN_COLS` by `MIN_ROWS`.
    NoRoom,
    /// The pane is on a side of the console, or is an extension's console view.
    OnConsole,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Refusal::NoWorkspacePane => f.write_str("show a pane in the Workspace first, then open another beside it"),
            Refusal::NoRoom => write!(f, "no room for another pane: each needs {MIN_COLS} columns by {MIN_ROWS} rows. Close one first"),
            Refusal::OnConsole => f.write_str("that pane is open in the console: close it there first"),
        }
    }
}

/// Moves `from`'s pane, screen and the size it was last asked onto `to`,
/// whose room stays: the page measures and remembers room by side, so a
/// pane that takes another side takes that side's room.
fn shift(from: &mut Slot, to: &mut Slot) {
    to.pane = from.pane.take();
    to.screen = from.screen.take();
    to.sized = from.sized.take();
}

/// A cell whose pane came from another: drawn whole, and resized only
/// where its new room differs from what it was last asked.
fn redraw(cell: &mut Slot, out: &mut Vec<ClientMsg>) {
    if let Some(screen) = &mut cell.screen {
        screen.refresh();
    }
    cell.send_size(out);
}

pub struct WorkspaceGrid {
    /// `cells[0]` the Workspace's, `cells[n + 1]` the `Side::Beside(n)`.
    cells: Vec<Slot>,
    /// The window's own layout: the daemon only learns each pane's rows
    /// and columns.
    layout: Layout,
}

impl Default for WorkspaceGrid {
    fn default() -> Self {
        WorkspaceGrid { cells: vec![Slot::default()], layout: Layout::default() }
    }
}

fn number(side: Side) -> Option<usize> {
    match side {
        Side::Workspace => Some(0),
        Side::Beside(n) => Some(usize::from(n) + 1),
        _ => None,
    }
}

fn side(n: usize) -> Side {
    n.checked_sub(1).map_or(Side::Workspace, |n| Side::Beside(n as u8))
}

impl WorkspaceGrid {
    /// The grid's sides in reading order, left to right and down.
    pub fn sides(&self) -> Vec<Side> {
        self.layout.order().into_iter().map(side).collect()
    }

    /// Whether `pane` is one of the grid's.
    pub fn holds(&self, pane: PaneId) -> bool {
        self.side_of(pane).is_some()
    }

    /// The sizes the person dragged the gutters to, in thousandths: each
    /// row's height and each pane's width in its row. False, and nothing
    /// changes, for sizes that are not the grid's shape.
    pub fn share(&mut self, heights: &[u16], widths: &[Vec<u16>]) -> bool {
        // The layout normalises the shares, so thousandths need no division.
        let shares = |list: &[u16]| list.iter().map(|&p| f32::from(p)).collect::<Vec<_>>();
        let widths: Vec<Vec<f32>> = widths.iter().map(|w| shares(w)).collect();
        let Some(layout) = self.layout.resized(&shares(heights), &widths) else { return false };
        self.layout = layout;
        true
    }

    /// Evens one row's panes out, or with none the rows' heights.
    pub fn even_out(&mut self, row: Option<usize>) {
        match row {
            Some(row) => self.layout.even_row(row),
            None => self.layout.even_rows(),
        }
    }

    /// `pane` dropped on `onto`'s `zone`; false when either is not in the
    /// grid.
    pub fn place(&mut self, pane: PaneId, onto: PaneId, zone: Zone) -> bool {
        let at = |pane| self.cells.iter().position(|cell| cell.pane == Some(pane));
        let (Some(a), Some(b)) = (at(pane), at(onto)) else { return false };
        self.layout.place(a, b, zone);
        true
    }

    /// The page the rail carries of the grid. `keys` is the side with
    /// them, and `describe` names a cell's pane as the header draws it.
    pub fn page(&self, keys: Side, describe: impl Fn(PaneId) -> Option<Shown>) -> GridPage {
        let thousandths = |list: &[f32]| list.iter().map(|s| (s * 1000.0).round() as u16).collect::<Vec<_>>();
        GridPage {
            beside: self.cells[1..].iter().map(|cell| cell.pane.and_then(&describe)).collect(),
            grid: self.layout.rows().iter().map(|row| row.iter().map(|&n| n as u8).collect()).collect(),
            heights: thousandths(self.layout.heights()),
            widths: self.layout.widths().iter().map(|w| thousandths(w)).collect(),
            keyed: number(keys).map(|n| n as u8),
            min_cols: MIN_COLS,
            min_rows: MIN_ROWS,
        }
    }

    /// The cell on `side`; none for a side that is not the grid's.
    pub(super) fn cell(&self, side: Side) -> Option<&Slot> {
        self.cells.get(number(side)?)
    }

    pub(super) fn cell_mut(&mut self, side: Side) -> Option<&mut Slot> {
        self.cells.get_mut(number(side)?)
    }

    /// Every cell with its side, in opening order.
    pub(super) fn cells_mut(&mut self) -> impl Iterator<Item = (Side, &mut Slot)> {
        self.cells.iter_mut().enumerate().map(|(n, cell)| (side(n), cell))
    }

    /// The grid's panes in opening order, the Workspace's first.
    pub(super) fn panes(&self) -> impl Iterator<Item = PaneId> + '_ {
        self.cells.iter().filter_map(|cell| cell.pane)
    }

    pub(super) fn side_of(&self, pane: PaneId) -> Option<Side> {
        self.cells.iter().position(|cell| cell.pane == Some(pane)).map(side)
    }

    /// Splits `pane` off into a new cell placed by the grid rule, sized to
    /// the room of the cell before it until the page measures its own. A
    /// pane the grid holds already is not split again; its side is returned.
    pub(super) fn join(&mut self, pane: PaneId, out: &mut Vec<ClientMsg>) -> Result<Side, Refusal> {
        if self.cells[0].pane.is_none() {
            return Err(Refusal::NoWorkspacePane);
        }
        if let Some(side) = self.side_of(pane) {
            return Ok(side);
        }
        if !self.has_room() {
            return Err(Refusal::NoRoom);
        }
        let mut cell = Slot { capacity: self.cells.last().and_then(|cell| cell.capacity), ..Slot::default() };
        cell.subscribe(pane, out);
        self.cells.push(cell);
        self.layout.add();
        debug_assert!(self.whole());
        Ok(side(self.cells.len() - 1))
    }

    /// Takes every emptied cell out — the Workspace's too while another
    /// can take its place — its room going to a neighbour, the panes after
    /// it numbered down onto the rooms of the sides they take and drawn
    /// whole; `keys` follows its pane, and keys on a cell that went go to
    /// its neighbour on the left, or the Workspace's.
    pub(super) fn close_empty(&mut self, keys: &mut Side, out: &mut Vec<ClientMsg>) {
        for n in (0..self.cells.len()).rev() {
            if self.cells[n].pane.is_some() || self.cells.len() < 2 {
                continue;
            }
            for j in n + 1..self.cells.len() {
                let (left, right) = self.cells.split_at_mut(j);
                shift(&mut right[0], &mut left[j - 1]);
            }
            self.cells.pop();
            self.layout.remove(n);
            if let Some(k) = number(*keys) {
                if k >= n {
                    *keys = side(k.saturating_sub(1));
                }
            }
            for cell in &mut self.cells[n..] {
                redraw(cell, out);
            }
        }
        debug_assert!(self.whole());
    }

    /// The side next to `side`'s in `dir`.
    pub(super) fn neighbour(&self, side: Side, dir: Dir) -> Option<Side> {
        self.layout.neighbour(number(side)?, dir).map(self::side)
    }

    /// The pane on `side` trades places with its neighbour in `dir`; false
    /// when `side` is not the grid's.
    pub(super) fn step(&mut self, side: Side, dir: Dir) -> bool {
        let Some(at) = number(side) else { return false };
        self.layout.step(at, dir);
        true
    }

    /// Whether the grid has room for one more pane without any dropping
    /// below `MIN_COLS` by `MIN_ROWS`, judged by the size the page last
    /// reported for the panes; a grid not yet measured has room.
    fn has_room(&self) -> bool {
        let Some((cols, rows)) = self.area() else { return true };
        let mut next = self.layout.clone();
        next.add();
        next.fits(cols, rows)
    }

    /// The cells the grid spans: the widest row's columns added up, and each
    /// row's first pane's rows. None until the page has measured them all.
    fn area(&self) -> Option<(u16, u16)> {
        let room = |n: usize| self.cells.get(n)?.capacity;
        let cols = self.layout.rows().iter().map(|row| row.iter().map(|&n| Some(u32::from(room(n)?.1))).sum::<Option<u32>>()).collect::<Option<Vec<_>>>()?;
        let rows = self.layout.rows().iter().map(|row| Some(u32::from(room(row[0])?.0))).sum::<Option<u32>>()?;
        let cols = cols.into_iter().max()?;
        Some((cols.min(u32::from(u16::MAX)) as u16, rows.min(u32::from(u16::MAX)) as u16))
    }

    /// The invariants every move leaves: a cell for each place in the
    /// layout, a pane in each but a lone Workspace's, and no more cells
    /// than a side's number can name.
    fn whole(&self) -> bool {
        let filled = self.cells.iter().skip(1).all(|cell| cell.pane.is_some());
        let first = self.cells.first().is_some_and(|cell| cell.pane.is_some() || self.cells.len() == 1);
        first && filled && self.cells.len() == self.layout.len() && self.cells.len() <= 256
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(out: &[ClientMsg]) -> Vec<String> {
        out.iter()
            .map(|m| match m {
                ClientMsg::Subscribe { pane } => format!("subscribe {pane}"),
                ClientMsg::Unsubscribe { pane } => format!("unsubscribe {pane}"),
                ClientMsg::Resize { pane, rows, cols } => format!("resize {pane} {rows}x{cols}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    /// Pane 1 in the Workspace's cell with `room`, and nothing beside it.
    fn workspace(room: (u16, u16)) -> WorkspaceGrid {
        let mut grid = WorkspaceGrid::default();
        let cell = grid.cell_mut(Side::Workspace).unwrap();
        cell.capacity = Some(room);
        cell.subscribe(PaneId(1), &mut Vec::new());
        grid
    }

    fn room(grid: &mut WorkspaceGrid, side: Side, room: (u16, u16)) {
        grid.cell_mut(side).unwrap().capacity = Some(room);
    }

    /// `n` panes, 1 the Workspace's and from 2 on beside it, in a grid the
    /// page has not measured.
    fn unmeasured(n: u64) -> WorkspaceGrid {
        let mut grid = WorkspaceGrid::default();
        grid.cell_mut(Side::Workspace).unwrap().subscribe(PaneId(1), &mut Vec::new());
        for pane in 2..=n {
            grid.join(PaneId(pane), &mut Vec::new()).unwrap();
        }
        grid
    }

    fn rows(grid: &WorkspaceGrid) -> Vec<Vec<u8>> {
        grid.page(Side::Workspace, |_| None).grid
    }

    /// The Workspace's pane 1 and 4, 5 and 6 beside it, each cell's room
    /// 20 rows by 30 columns and one more for each cell after the first.
    fn four() -> WorkspaceGrid {
        let mut grid = workspace((20, 60));
        for pane in [4, 5, 6] {
            grid.join(PaneId(pane), &mut Vec::new()).unwrap();
        }
        for (n, (side, cell)) in grid.cells_mut().enumerate() {
            assert_eq!(side, self::side(n));
            cell.capacity = Some((20, 30 + n as u16));
            cell.send_size(&mut Vec::new());
        }
        grid
    }

    #[test]
    fn a_pane_joins_by_the_grid_rule_at_the_room_of_the_cell_before_it() {
        let mut grid = workspace((20, 60));
        let mut out = Vec::new();
        assert_eq!(grid.join(PaneId(4), &mut out), Ok(Side::Beside(0)));
        assert_eq!(names(&out), ["subscribe 4", "resize 4 20x60"]);
        assert_eq!(rows(&grid), [vec![0, 1]]);
        grid.join(PaneId(5), &mut Vec::new()).unwrap();
        assert_eq!(rows(&grid), [vec![0, 1], vec![2]], "two over one");
        assert_eq!(grid.panes().collect::<Vec<_>>(), [PaneId(1), PaneId(4), PaneId(5)]);
    }

    #[test]
    fn a_pane_already_in_the_row_is_not_split_again() {
        let mut grid = four();
        let mut out = Vec::new();
        assert_eq!(grid.join(PaneId(5), &mut out), Ok(Side::Beside(1)));
        assert!(out.is_empty());
        assert_eq!(grid.sides().len(), 4);
    }

    #[test]
    fn a_grid_refuses_a_pane_that_would_leave_one_smaller_than_the_least() {
        let mut grid = workspace((20, 70));
        assert_eq!(grid.join(PaneId(4), &mut Vec::new()), Ok(Side::Beside(0)), "70 columns make two of 35");
        room(&mut grid, Side::Beside(0), (20, 35));
        assert!(grid.has_room(), "two over one of 20 rows makes 10 each");
        room(&mut grid, Side::Workspace, (20, 20));
        room(&mut grid, Side::Beside(0), (20, 19));
        let mut out = Vec::new();
        assert_eq!(grid.join(PaneId(5), &mut out), Err(Refusal::NoRoom), "39 columns cannot make two of 20 over a third");
        room(&mut grid, Side::Workspace, (11, 40));
        room(&mut grid, Side::Beside(0), (11, 40));
        assert_eq!(grid.join(PaneId(5), &mut out), Err(Refusal::NoRoom), "11 rows cannot make two of 6");
        assert!(out.is_empty(), "nothing subscribed");
        assert_eq!(grid.sides(), [Side::Workspace, Side::Beside(0)]);
    }

    #[test]
    fn nothing_splits_off_a_workspace_with_no_pane() {
        let mut grid = WorkspaceGrid::default();
        let mut out = Vec::new();
        assert_eq!(grid.join(PaneId(4), &mut out), Err(Refusal::NoWorkspacePane));
        assert!(out.is_empty());
        assert_eq!(grid.sides(), [Side::Workspace]);
    }

    #[test]
    fn the_grids_page_reads_as_the_page_expects() {
        let page = unmeasured(2).page(Side::Workspace, |_| None);
        assert_eq!(
            serde_json::to_value(page).unwrap(),
            serde_json::json!({
                "beside": [null],
                "grid": [[0, 1]],
                "heights": [1000],
                "widths": [[500, 500]],
                "keyed": 0,
                "minCols": 20,
                "minRows": 6,
            })
        );
    }

    #[test]
    fn the_page_sends_sizes_in_thousandths() {
        let page = unmeasured(5).page(Side::Workspace, |_| None);
        assert_eq!(page.grid, [vec![0, 1, 4], vec![2, 3]]);
        assert_eq!((page.widths[0].clone(), page.heights), (vec![333, 333, 333], vec![500, 500]));
    }

    #[test]
    fn the_page_names_the_cell_with_the_keys_as_the_grid_numbers_it() {
        let grid = unmeasured(3);
        assert_eq!(grid.page(Side::Workspace, |_| None).keyed, Some(0));
        assert_eq!(grid.page(Side::Beside(1), |_| None).keyed, Some(2));
    }

    #[test]
    fn no_cell_is_keyed_while_the_keys_are_off_the_grid() {
        let grid = unmeasured(3);
        for keys in [Side::First, Side::Second, Side::View(0)] {
            assert_eq!(grid.page(keys, |_| None).keyed, None, "{keys:?}");
        }
    }

    #[test]
    fn a_split_the_tree_no_longer_names_keeps_its_place_without_a_header() {
        let grid = unmeasured(3);
        let describe = |pane: PaneId| {
            let shown = Shown {
                pane: pane.0,
                repository: String::new(),
                checkout: String::new(),
                branch: String::new(),
                agent: false,
                title: format!("pane {pane}"),
                status: "idle",
                resumed: None,
            };
            (pane != PaneId(2)).then_some(shown)
        };
        let page = grid.page(Side::Workspace, describe);
        let titles: Vec<Option<String>> = page.beside.iter().map(|b| b.as_ref().map(|s| s.title.clone())).collect();
        assert_eq!(titles, [None, Some("pane 3".to_string())], "pane 3's header stays on its own cell");
        assert_eq!(page.beside.len(), grid.sides().len() - 1);
    }

    #[test]
    fn dragged_sizes_that_do_not_fit_the_grid_change_nothing() {
        let mut grid = unmeasured(4);
        let page = |grid: &WorkspaceGrid| grid.page(Side::Workspace, |_| None);
        assert!(grid.share(&[250, 750], &[vec![750, 250], vec![500, 500]]));
        assert_eq!((page(&grid).heights, page(&grid).widths[0].clone()), (vec![250, 750], vec![750, 250]));
        assert!(grid.share(&[1, 3], &[vec![1, 1], vec![1, 1]]), "normalised");
        assert_eq!(page(&grid).widths[0], [500, 500]);
        assert!(!grid.share(&[1000, 0], &[vec![500, 500], vec![500, 500]]), "a zero share");
        assert!(!grid.share(&[1000], &[vec![1000]]), "the wrong shape");
        assert_eq!(page(&grid).heights, [250, 750]);
        grid.share(&[250, 750], &[vec![750, 250], vec![500, 500]]);
        grid.even_out(Some(0));
        assert_eq!(page(&grid).widths[0], [500, 500]);
        grid.even_out(None);
        assert_eq!(page(&grid).heights, [500, 500]);
    }

    #[test]
    fn a_refusal_says_why_in_the_words_the_note_shows() {
        assert!(Refusal::NoWorkspacePane.to_string().contains("in the Workspace first"));
        assert!(Refusal::NoRoom.to_string().contains("20 columns by 6 rows"));
        assert!(Refusal::OnConsole.to_string().contains("open in the console"));
    }

    #[test]
    fn an_emptied_cell_goes_and_the_cells_after_it_number_down_with_the_keys() {
        let mut grid = four();
        let mut keys = Side::Beside(2);
        grid.cell_mut(Side::Beside(1)).unwrap().clear(true, &mut Vec::new());
        let mut out = Vec::new();
        grid.close_empty(&mut keys, &mut out);
        assert_eq!(keys, Side::Beside(1), "the keys follow pane 6");
        assert_eq!(grid.cell(Side::Beside(1)).and_then(|cell| cell.pane), Some(PaneId(6)));
        assert_eq!(grid.cell(Side::Beside(1)).and_then(|cell| cell.capacity), Some((20, 32)), "the room stays with the number");
        assert_eq!(names(&out), ["resize 6 20x32"]);
        assert_eq!(rows(&grid), [vec![0, 1], vec![2]]);

        grid.cell_mut(Side::Beside(1)).unwrap().clear(true, &mut Vec::new());
        grid.close_empty(&mut keys, &mut Vec::new());
        assert_eq!(keys, Side::Beside(0), "keys on the cell that went go to its left");
        assert_eq!(grid.sides(), [Side::Workspace, Side::Beside(0)]);
    }

    #[test]
    fn the_workspaces_cell_alone_stays_when_emptied() {
        let mut grid = workspace((20, 60));
        grid.join(PaneId(4), &mut Vec::new()).unwrap();
        let mut keys = Side::Workspace;
        grid.cell_mut(Side::Workspace).unwrap().clear(true, &mut Vec::new());
        grid.close_empty(&mut keys, &mut Vec::new());
        assert_eq!((grid.side_of(PaneId(4)), keys), (Some(Side::Workspace), Side::Workspace), "the split takes its place");
        grid.cell_mut(Side::Workspace).unwrap().clear(true, &mut Vec::new());
        grid.close_empty(&mut keys, &mut Vec::new());
        assert_eq!(grid.sides(), [Side::Workspace]);
        assert_eq!(grid.panes().count(), 0);
    }
}
