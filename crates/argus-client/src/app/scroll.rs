//! Moving a pane's view up into what has already scrolled past.
//!
//! The daemon owns the history; this module owns only where the pane is
//! parked in it. An offset is a request, and the rows arrive a frame later
//! as `ScrollbackRows` — so nothing here waits for an answer, and a burst
//! of wheel notches costs one request each rather than a stall apiece.
//!
//! While a pane is parked, its live grid keeps taking damage underneath.
//! The parked rows are deliberately *not* refreshed as that arrives: they
//! are what the operator scrolled up to read, and a pane still producing
//! output would otherwise shift the text out from under them on every
//! frame. Dropping back to the bottom shows the live screen, current.

use super::*;
use argus_protocol::Cell;

/// Wheel notches move three lines, matching what terminals send for one
/// detent of a physical wheel.
const WHEEL_LINES: i32 = 3;

impl App {
    /// Moves `pane`'s view by `delta` lines: negative up into history,
    /// positive back down toward the live screen.
    pub(super) fn scroll_pane(&mut self, pane: PaneId, delta: i32) {
        let Some(grid) = self.grids.get(&pane) else {
            return;
        };
        let current = grid.scrollback.as_ref().map_or(0, |sb| sb.offset);
        // A pane that has never been scrolled has no depth reading yet, so
        // the first request up is unbounded and the daemon's clamp decides
        // where it lands. Every later one is bounded by what it answered.
        let ceiling = grid
            .scrollback
            .as_ref()
            .map_or(i64::MAX, |sb| i64::from(sb.depth));
        let want = (i64::from(current) - i64::from(delta)).clamp(0, ceiling);
        // Clamping at either end turns a run of wheel notches into the same
        // offset over and over; only a move is worth a round trip.
        if want == i64::from(current) {
            return;
        }
        self.park_pane(pane, want as u32);
    }

    /// Moves `pane`'s view by whole screens, which is what Page Up/Down and
    /// the scrollback keys mean.
    pub(super) fn page_pane(&mut self, pane: PaneId, pages: i32) {
        // One line of overlap, so paging keeps a line of context rather
        // than jumping a clean screen and leaving nothing to line up on.
        let height = i32::from(self.layout.content.inner.height).max(2) - 1;
        self.scroll_pane(pane, pages.saturating_mul(height));
    }

    /// Drops `pane` back onto the live screen, if it was anywhere else.
    /// Called wherever the operator's attention returns to the present:
    /// typing, pasting, or leaving the pane.
    pub(super) fn scroll_to_live(&mut self, pane: PaneId) {
        if let Some(grid) = self.grids.get_mut(&pane) {
            grid.scrollback = None;
        }
    }

    /// Parks `pane` at `offset` and asks the daemon for the rows there.
    /// An offset of zero is the live screen, which needs no request: the
    /// grid underneath has been kept current the whole time.
    pub(super) fn park_pane(&mut self, pane: PaneId, offset: u32) {
        let Some(grid) = self.grids.get_mut(&pane) else {
            return;
        };
        if offset == 0 {
            grid.scrollback = None;
            return;
        }
        match &mut grid.scrollback {
            Some(sb) => sb.offset = offset,
            // Seeded from the live rows so the pane keeps drawing text for
            // the frame before the answer lands.
            None => {
                grid.scrollback = Some(crate::grid::Scrollback {
                    offset,
                    depth: 0,
                    cells: grid.cells.clone(),
                })
            }
        }
        let _ = self.out.send(ClientMsg::Scrollback { pane, offset });
    }

    /// The rows the daemon read, applied only if the pane is still parked.
    /// A reply that arrives after the operator dropped back to live is
    /// stale by definition and must not pull the view back up.
    pub(super) fn receive_scrollback(
        &mut self,
        pane: PaneId,
        offset: u32,
        depth: u32,
        cells: Vec<Vec<Cell>>,
    ) {
        let Some(grid) = self.grids.get_mut(&pane) else {
            return;
        };
        let Some(sb) = grid.scrollback.as_mut() else {
            return;
        };
        // The daemon clamps, so the offset it answers with can be
        // shallower than the one asked for; its number is the truth about
        // where these rows came from. Replies arrive in request order on
        // one connection, so the newest is always the current one.
        sb.depth = depth;
        sb.offset = offset;
        sb.cells = cells;
        // Nothing behind the live screen — an empty buffer, or a
        // full-screen child that keeps no history. There is no parked view
        // to hold, so the pane is simply live.
        if offset == 0 {
            grid.scrollback = None;
            self.report("nothing further back");
        }
    }

    /// How a parked pane says so in its title: `↑ 120/4000`.
    pub fn scroll_indicator(&self) -> Option<String> {
        let grid = self.column_pane().and_then(|id| self.grids.get(&id))?;
        let sb = grid.scrollback.as_ref()?;
        Some(format!("↑ {}/{}", sb.offset, sb.depth.max(sb.offset)))
    }
}

/// Lines one wheel notch moves, signed for direction.
pub(super) fn wheel_lines(up: bool) -> i32 {
    if up {
        -WHEEL_LINES
    } else {
        WHEEL_LINES
    }
}
