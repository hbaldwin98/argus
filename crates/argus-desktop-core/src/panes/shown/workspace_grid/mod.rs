//! The Workspace's grid: the panes it holds — the Workspace's own first,
//! then those split beside it — where each sits and how big, which pane
//! may join it and why another may not, and the page the rail carries of it.
//!
//! Cells are numbered as `split_grid` numbers its panes: 0 the Workspace's,
//! `n + 1` the page's `Side::Beside(n)`. Every cell holds a pane or a panel
//! but the Workspace's, so a side the page names is never an empty split.
//! The Workspace's own cell is a pane's or empty: a panel only splits off a
//! pane, and a panel left first when that pane goes trades places with the
//! first pane after it. With no pane left the panels keep their cells beside
//! an empty Workspace, for the next pane shown there: a session closed is
//! not a request to close what was split off it. Which side has the keys is
//! `shown`'s; this keeps that side's number right as cells come and go.

pub mod gutter;
pub mod head;
pub mod occupant;
pub mod rects;
mod split_grid;
pub mod zones;

use argus_protocol::{ClientMsg, PaneId};
use serde::Serialize;

use super::Slot;
use crate::connection::page::{Shown, Side};
use crate::layout::docks::Panel;
use split_grid::{Layout, MIN_COLS, MIN_ROWS};

pub use occupant::Occupant;
pub use split_grid::{Dir, Rect, Zone};

/// What the rail carries of the Workspace's grid: the first pane is `Rail.shown`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GridPage {
    /// The panes split beside the first, in the order they were opened:
    /// cell `n + 1` of `grid` is `beside[n]`. None for one the tree no
    /// longer names, so its cell keeps its place without a header.
    pub beside: Vec<Option<Shown>>,
    /// The panel each split draws, numbered as `beside` is; none for a
    /// pane's. A cell with a panel has no `beside`.
    pub panels: Vec<Option<Panel>>,
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
    /// The grid has as many cells as a side's number can name.
    Full,
    /// The pane is on a side of the console, or is an extension's console view.
    OnConsole,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Refusal::NoWorkspacePane => f.write_str("show a pane in the Workspace first, then open another beside it"),
            Refusal::NoRoom => write!(f, "no room for another pane: each needs {MIN_COLS} columns by {MIN_ROWS} rows. Close one first"),
            Refusal::Full => f.write_str("the grid holds as many panes as it can: close one first"),
            Refusal::OnConsole => f.write_str("that pane is open in the console: close it there first"),
        }
    }
}

/// Moves `from`'s pane, screen and the size it was last asked onto `to`,
/// whose room stays: the page measures and remembers room by side, so a
/// pane that takes another side takes that side's room.
fn shift(from: &mut Slot, to: &mut Slot) {
    to.pane = from.pane.take();
    to.panel = from.panel.take();
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

    /// The cell holding `moved` dropped on `onto`'s `zone`; false when
    /// either is not in the grid. A panel never takes the Workspace's own
    /// place, so a drop that would swap one there changes nothing.
    pub fn place(&mut self, moved: &Occupant, onto: &Occupant, zone: Zone) -> bool {
        let at = |occupant: &Occupant| self.cells.iter().position(|cell| cell.holds(occupant));
        let (Some(a), Some(b)) = (at(moved), at(onto)) else { return false };
        let panel = |n: usize| self.cells[n].panel.is_some();
        if zone == Zone::Centre && ((a == 0 && panel(b)) || (b == 0 && panel(a))) {
            return false;
        }
        self.layout.place(a, b, zone);
        true
    }

    /// Whether `panel` has a cell of the grid.
    pub fn holds_panel(&self, panel: &Panel) -> bool {
        self.cells.iter().any(|cell| cell.panel.as_ref() == Some(panel))
    }

    /// The panels with a cell, in opening order.
    pub fn panels(&self) -> Vec<Panel> {
        self.cells.iter().filter_map(|cell| cell.panel.clone()).collect()
    }

    /// The page the rail carries of the grid. `keys` is the side with
    /// them, and `describe` names a cell's pane as the header draws it.
    pub fn page(&self, keys: Side, describe: impl Fn(PaneId) -> Option<Shown>) -> GridPage {
        let thousandths = |list: &[f32]| list.iter().map(|s| (s * 1000.0).round() as u16).collect::<Vec<_>>();
        GridPage {
            beside: self.cells[1..].iter().map(|cell| cell.pane.and_then(&describe)).collect(),
            panels: self.cells[1..].iter().map(|cell| cell.panel.clone()).collect(),
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
        if let Some(side) = self.side_of(pane) {
            return Ok(side);
        }
        self.admits()?;
        let mut cell = self.next_cell();
        cell.subscribe(pane, out);
        Ok(self.push(cell))
    }

    /// Splits `panel` off into a new cell, as `join` does a pane, with
    /// nothing to subscribe: the window draws it.
    pub(super) fn join_panel(&mut self, panel: &Panel) -> Result<Side, Refusal> {
        if let Some(at) = self.cells.iter().position(|cell| cell.panel.as_ref() == Some(panel)) {
            return Ok(side(at));
        }
        self.admits()?;
        let cell = Slot { panel: Some(panel.clone()), ..self.next_cell() };
        Ok(self.push(cell))
    }

    /// Takes `panel`'s cell out on the next `close_empty`; false when it has
    /// none.
    pub(super) fn release(&mut self, panel: &Panel) -> bool {
        let Some(cell) = self.cells.iter_mut().find(|cell| cell.panel.as_ref() == Some(panel)) else { return false };
        cell.panel = None;
        true
    }

    /// Why another cell may not split off now, if it may not.
    fn admits(&self) -> Result<(), Refusal> {
        if self.cells[0].pane.is_none() {
            return Err(Refusal::NoWorkspacePane);
        }
        // A grid the page has not measured always has room, so the cells'
        // numbers, which the page reads as bytes, are what bounds it.
        if self.cells.len() > usize::from(u8::MAX) {
            return Err(Refusal::Full);
        }
        if !self.has_room() {
            return Err(Refusal::NoRoom);
        }
        Ok(())
    }

    /// An empty cell sized to the room of the cell before it until the page
    /// measures its own.
    fn next_cell(&self) -> Slot {
        Slot { capacity: self.cells.last().and_then(|cell| cell.capacity), ..Slot::default() }
    }

    fn push(&mut self, cell: Slot) -> Side {
        self.cells.push(cell);
        self.layout.add();
        debug_assert!(self.whole());
        side(self.cells.len() - 1)
    }

    /// Takes every emptied cell out — the Workspace's too while a pane can
    /// take its place — its room going to a neighbour, the panes after it
    /// numbered down onto the rooms of the sides they take and drawn whole;
    /// `keys` follows its pane, and keys on a cell that went go to its
    /// neighbour on the left, or the Workspace's.
    pub(super) fn close_empty(&mut self, keys: &mut Side, out: &mut Vec<ClientMsg>) {
        self.drop_empty(keys, out);
        // The Workspace's cell went only for a pane left to take its place:
        // that pane takes the Workspace's number and the panel its, each
        // keeping its place on screen.
        if self.cells[0].panel.is_some() {
            let k = self.cells.iter().position(|cell| cell.pane.is_some()).expect("a pane is left");
            let (first, rest) = self.cells.split_at_mut(k);
            let (a, b) = (&mut first[0], &mut rest[0]);
            std::mem::swap(&mut a.pane, &mut b.pane);
            std::mem::swap(&mut a.panel, &mut b.panel);
            std::mem::swap(&mut a.screen, &mut b.screen);
            std::mem::swap(&mut a.sized, &mut b.sized);
            self.layout.swap(0, k);
            redraw(&mut self.cells[0], out);
            if *keys == side(k) {
                *keys = Side::Workspace;
            }
        }
        debug_assert!(self.whole());
    }

    fn drop_empty(&mut self, keys: &mut Side, out: &mut Vec<ClientMsg>) {
        let paned = self.cells.iter().any(|cell| cell.pane.is_some());
        for n in (0..self.cells.len()).rev() {
            if self.cells[n].filled() || self.cells.len() < 2 || (n == 0 && !paned) {
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
    /// layout, a pane or a panel in each but the Workspace's, which only a
    /// pane fills and is empty only with no pane in the grid, none with
    /// both, and no more cells than a side's number can name.
    fn whole(&self) -> bool {
        let filled = self.cells.iter().skip(1).all(|cell| cell.filled() && !(cell.pane.is_some() && cell.panel.is_some()));
        let paned = self.cells.iter().any(|cell| cell.pane.is_some());
        let first = self.cells.first().is_some_and(|cell| cell.panel.is_none() && (cell.pane.is_some() || !paned));
        first && filled && self.cells.len() == self.layout.len() && self.cells.len() <= 256
    }
}

#[cfg(test)]
mod tests;
