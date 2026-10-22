//! Where a carried pane would land if it were let go at a point, and the call that makes it so.
//!
//! The order is the page's (`dragPane`): another checkout's row first, then a cell's zone, then
//! the stage with no cell under it (SHELL-GRID-11 to -13, SHELL-RAIL-17). Which cell and zone is
//! `argus-desktop-core`'s (`workspace_grid::zones`); this reads the places the views published
//! and names the call. Nothing here is predicted: a landing is a request, and the grid changes
//! when the session's rail says so.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::panes::shown::workspace_grid::zones::cell_under;
use argus_desktop_core::panes::shown::workspace_grid::{Rect, Zone};
use gpui_kit::{Bounds, Pixels, Point};

use super::Carried;
use crate::kit::spots::{Spot, Spots};

/// Where a carried pane would land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Landing {
    /// On `zone` of the cell numbered `slot`, which holds `onto`.
    Cell { slot: u8, onto: u64, zone: Zone, bounds: Bounds<Pixels> },
    /// On the stage with no cell under it.
    Stage,
    /// On another checkout's row.
    Checkout(u64),
    Nowhere,
}

/// What the Workspace's grid is, for deciding a landing: whether it is on screen, and the pane in
/// each cell by its number (none for a cell the tree no longer names).
#[derive(Debug, Clone, Copy)]
pub struct Grid<'a> {
    pub up: bool,
    pub panes: &'a [Option<u64>],
}

fn rect(bounds: Bounds<Pixels>) -> Rect {
    Rect { x: bounds.origin.x.as_f32(), y: bounds.origin.y.as_f32(), w: bounds.size.width.as_f32(), h: bounds.size.height.as_f32() }
}

/// The landing of `carried` let go at `at`.
pub fn landing(carried: &Carried, at: Point<Pixels>, grid: Grid, spots: &Spots) -> Landing {
    if let Carried::Rail { checkout: Some(home), .. } = carried {
        if let Some(row) = spots.checkout_at(at).filter(|row| row != home) {
            return Landing::Checkout(row);
        }
    }
    if !grid.up {
        return Landing::Nowhere;
    }
    let own = match carried {
        Carried::Cell { slot, .. } => Some(usize::from(*slot)),
        Carried::Rail { .. } => None,
    };
    let cells: Vec<(u8, Bounds<Pixels>)> = spots.cells();
    let rects: Vec<(usize, Rect)> = cells.iter().map(|(n, bounds)| (usize::from(*n), rect(*bounds))).collect();
    if let Some((slot, zone)) = cell_under(&rects, at.x.as_f32(), at.y.as_f32(), own) {
        let onto = grid.panes.get(slot).copied().flatten();
        let bounds = cells.iter().find(|(n, _)| usize::from(*n) == slot).map(|(_, bounds)| *bounds);
        // A cell the tree no longer names is not a pane to drop on; it is the stage under it.
        if let (Some(onto), Some(bounds)) = (onto, bounds) {
            return Landing::Cell { slot: slot as u8, onto, zone, bounds };
        }
    }
    // Only a pane from the rail opens on the stage; a header let go on no cell goes nowhere.
    let on_stage = matches!(carried, Carried::Rail { .. }) && spots.get(Spot::Stage).is_some_and(|stage| stage.contains(&at));
    if on_stage {
        Landing::Stage
    } else {
        Landing::Nowhere
    }
}

/// The call that lets `carried` go at `landing`, if it is to be let go there at all.
pub fn call(carried: &Carried, landing: Landing) -> Option<Call> {
    let pane = carried.pane();
    match (carried, landing) {
        (Carried::Cell { .. }, Landing::Cell { onto, zone, .. }) => Some(Call::MovePane { pane, onto, zone }),
        (Carried::Rail { .. }, Landing::Cell { onto, zone, .. }) if onto != pane => Some(Call::DropPane { pane, onto: Some(onto), zone }),
        (Carried::Rail { .. }, Landing::Stage) => Some(Call::DropPane { pane, onto: None, zone: Zone::Centre }),
        (Carried::Rail { .. }, Landing::Checkout(checkout)) => Some(Call::MoveToCheckout { pane, checkout }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px, size};

    fn at(x: f32, y: f32) -> Point<Pixels> {
        point(px(x), px(y))
    }

    fn area(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(at(x, y), size(px(w), px(h)))
    }

    /// Two cells side by side in a stage 200 wide, a checkout row to the left of them.
    fn spots() -> Spots {
        let spots = Spots::default();
        spots.set(Spot::Stage, area(100.0, 0.0, 200.0, 100.0));
        spots.set(Spot::Cell(0), area(100.0, 0.0, 100.0, 100.0));
        spots.set(Spot::Cell(1), area(200.0, 0.0, 100.0, 100.0));
        spots.set(Spot::Checkout(7), area(0.0, 0.0, 90.0, 24.0));
        spots.set(Spot::Checkout(8), area(0.0, 24.0, 90.0, 24.0));
        spots
    }

    /// `Call` has no equality; its words are what is compared.
    fn said(call: Option<Call>) -> String {
        format!("{call:?}")
    }

    const PANES: [Option<u64>; 2] = [Some(1), Some(2)];

    fn grid() -> Grid<'static> {
        Grid { up: true, panes: &PANES }
    }

    fn header() -> Carried {
        Carried::Cell { slot: 0, pane: 1, title: "one".into() }
    }

    fn row(pane: u64, checkout: Option<u64>) -> Carried {
        Carried::Rail { pane, title: "p".into(), checkout }
    }

    #[test]
    fn a_header_over_another_cell_lands_on_the_zone_of_it() {
        let landed = landing(&header(), at(250.0, 50.0), grid(), &spots());
        assert!(matches!(landed, Landing::Cell { slot: 1, onto: 2, zone: Zone::Centre, .. }), "{landed:?}");
        let near = landing(&header(), at(205.0, 50.0), grid(), &spots());
        assert!(matches!(near, Landing::Cell { zone: Zone::Left, .. }), "{near:?}");
        assert_eq!(said(call(&header(), landed)), said(Some(Call::MovePane { pane: 1, onto: 2, zone: Zone::Centre })));
    }

    #[test]
    fn a_header_is_never_dropped_on_its_own_cell() {
        assert_eq!(landing(&header(), at(150.0, 50.0), grid(), &spots()), Landing::Nowhere);
    }

    #[test]
    fn a_header_let_go_on_no_cell_goes_nowhere_even_over_the_stage() {
        let spots = spots();
        spots.set(Spot::Stage, area(100.0, 0.0, 200.0, 300.0));
        assert_eq!(landing(&header(), at(150.0, 200.0), grid(), &spots), Landing::Nowhere);
    }

    #[test]
    fn a_pane_from_the_rail_lands_on_a_cell_beside_it_by_the_zone() {
        let carried = row(9, Some(7));
        let landed = landing(&carried, at(295.0, 50.0), grid(), &spots());
        assert_eq!(said(call(&carried, landed)), said(Some(Call::DropPane { pane: 9, onto: Some(2), zone: Zone::Right })));
    }

    #[test]
    fn a_pane_from_the_rail_let_go_over_its_own_cell_asks_for_nothing() {
        let carried = row(1, Some(7));
        let landed = landing(&carried, at(150.0, 50.0), grid(), &spots());
        assert!(matches!(landed, Landing::Cell { onto: 1, .. }));
        assert!(call(&carried, landed).is_none());
    }

    #[test]
    fn a_pane_from_the_rail_over_the_stage_with_no_cell_opens_there() {
        let spots = spots();
        spots.set(Spot::Cell(1), area(200.0, 0.0, 50.0, 100.0));
        let carried = row(9, Some(7));
        let landed = landing(&carried, at(280.0, 50.0), grid(), &spots);
        assert_eq!(landed, Landing::Stage);
        assert_eq!(said(call(&carried, landed)), said(Some(Call::DropPane { pane: 9, onto: None, zone: Zone::Centre })));
    }

    #[test]
    fn another_checkouts_row_is_a_target_and_the_panes_own_is_not() {
        let carried = row(9, Some(7));
        let other = landing(&carried, at(10.0, 30.0), grid(), &spots());
        assert_eq!(other, Landing::Checkout(8));
        assert_eq!(said(call(&carried, other)), said(Some(Call::MoveToCheckout { pane: 9, checkout: 8 })));
        assert_eq!(landing(&carried, at(10.0, 10.0), grid(), &spots()), Landing::Nowhere, "its own row");
    }

    #[test]
    fn a_pane_that_cannot_move_between_checkouts_has_no_row_to_land_on() {
        assert_eq!(landing(&row(9, None), at(10.0, 30.0), grid(), &spots()), Landing::Nowhere);
        assert_eq!(landing(&header(), at(10.0, 30.0), grid(), &spots()), Landing::Nowhere);
    }

    #[test]
    fn the_cells_are_no_targets_while_the_grid_is_not_on_screen() {
        let off = Grid { up: false, panes: &PANES };
        assert_eq!(landing(&header(), at(250.0, 50.0), off, &spots()), Landing::Nowhere);
        assert_eq!(landing(&row(9, Some(7)), at(250.0, 50.0), off, &spots()), Landing::Nowhere);
        assert_eq!(landing(&row(9, Some(7)), at(10.0, 30.0), off, &spots()), Landing::Checkout(8), "the rail is still there");
    }

    #[test]
    fn a_cell_the_tree_no_longer_names_is_the_stage_under_it() {
        let gone: [Option<u64>; 2] = [Some(1), None];
        let grid = Grid { up: true, panes: &gone };
        let carried = row(9, Some(7));
        assert_eq!(landing(&carried, at(250.0, 50.0), grid, &spots()), Landing::Stage);
        assert_eq!(landing(&header(), at(250.0, 50.0), grid, &spots()), Landing::Nowhere);
    }

    #[test]
    fn a_landing_nowhere_asks_for_nothing() {
        assert!(call(&header(), Landing::Nowhere).is_none());
        assert!(call(&row(1, Some(7)), Landing::Nowhere).is_none());
        assert!(call(&header(), Landing::Stage).is_none(), "a header cannot open a pane on the stage");
    }
}
