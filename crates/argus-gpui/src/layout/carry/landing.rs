//! Where a carried pane would land if it were let go at a point, and the call that makes it so.
//!
//! The order is the page's (`dragPane`): another checkout's row first, then a cell's zone, then
//! the stage with no cell under it (SHELL-GRID-11 to -13, SHELL-RAIL-17). Which cell and zone is
//! `argus-desktop-core`'s (`workspace_grid::zones`); this reads the places the views published
//! and names the call. Nothing here is predicted: a landing is a request, and the grid changes
//! when the session's rail says so.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::layout::docks::DockGesture;
use argus_desktop_core::panes::shown::workspace_grid::zones::cell_under;
use argus_desktop_core::panes::shown::workspace_grid::{Occupant, Rect, Zone};
use gpui_kit::{Bounds, Pixels, Point};

use super::Carried;
use crate::kit::spots::{Spot, Spots};

/// Where a carried pane would land.
#[derive(Debug, Clone, PartialEq)]
pub enum Landing {
    /// On `zone` of the cell numbered `slot`, which holds `onto`.
    Cell { slot: u8, onto: Occupant, zone: Zone, bounds: Bounds<Pixels> },
    /// On the stage with no cell under it.
    Stage,
    /// On another checkout's row.
    Checkout(u64),
    Nowhere,
}

/// What the Workspace's grid is, for deciding a landing: whether it is on screen, and what each
/// cell holds by its number (none for a cell the tree no longer names).
#[derive(Debug, Clone, Copy)]
pub struct Grid<'a> {
    pub up: bool,
    pub panes: &'a [Option<Occupant>],
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
        Carried::Rail { .. } | Carried::Panel { .. } => None,
    };
    let cells: Vec<(u8, Bounds<Pixels>)> = spots.cells();
    let rects: Vec<(usize, Rect)> = cells.iter().map(|(n, bounds)| (usize::from(*n), rect(*bounds))).collect();
    if let Some((slot, zone)) = cell_under(&rects, at.x.as_f32(), at.y.as_f32(), own) {
        let onto = grid.panes.get(slot).cloned().flatten();
        let bounds = cells.iter().find(|(n, _)| usize::from(*n) == slot).map(|(_, bounds)| *bounds);
        // A cell the tree no longer names is not a pane to drop on; it is the stage under it.
        if let (Some(onto), Some(bounds)) = (onto, bounds) {
            return Landing::Cell { slot: slot as u8, onto, zone, bounds };
        }
    }
    // The stage is outlined for either kind (SHELL-GRID-11), though only a pane from the rail opens
    // on it: `call` sends nothing for a header let go there.
    let on_stage = spots.get(Spot::Stage).is_some_and(|stage| stage.contains(&at));
    if on_stage {
        Landing::Stage
    } else {
        Landing::Nowhere
    }
}

/// The call that lets `carried` go at `landing`, if it is to be let go there at all. A panel let
/// go anywhere but a cell is the dock drag's to answer.
pub fn call(carried: &Carried, landing: Landing) -> Option<Call> {
    if matches!(&landing, Landing::Cell { onto, .. } if *onto == carried.occupant()) {
        return None;
    }
    match (carried, landing) {
        (Carried::Cell { held, .. }, Landing::Cell { onto, zone, .. }) => Some(Call::MovePane { pane: held.clone(), onto, zone }),
        (Carried::Rail { pane, .. }, Landing::Cell { onto, zone, .. }) => Some(Call::DropPane { pane: *pane, onto: Some(onto), zone }),
        (Carried::Rail { pane, .. }, Landing::Stage) => Some(Call::DropPane { pane: *pane, onto: None, zone: Zone::Centre }),
        (Carried::Rail { pane, .. }, Landing::Checkout(checkout)) => Some(Call::MoveToCheckout { pane: *pane, checkout }),
        (Carried::Panel { panel, .. }, Landing::Cell { onto, zone, .. }) => {
            Some(Call::Docks { gesture: DockGesture::Grid { panel: panel.clone(), onto, zone } })
        }
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

    const PANES: [Option<Occupant>; 2] = [Some(Occupant::Pane(1)), Some(Occupant::Pane(2))];

    fn grid() -> Grid<'static> {
        Grid { up: true, panes: &PANES }
    }

    fn header() -> Carried {
        Carried::Cell { slot: 0, held: Occupant::Pane(1), title: "one".into() }
    }

    fn browsers() -> Carried {
        Carried::Panel { panel: Panel::Browsers, title: "Browsers".into() }
    }

    #[test]
    fn a_panel_over_a_cell_splits_off_by_the_zone_and_over_its_own_cell_asks_for_nothing() {
        let landed = landing(&browsers(), at(295.0, 50.0), grid(), &spots());
        let asked = Call::Docks { gesture: DockGesture::Grid { panel: Panel::Browsers, onto: Occupant::Pane(2), zone: Zone::Right } };
        assert_eq!(said(call(&browsers(), landed)), said(Some(asked)));
        assert!(call(&browsers(), Landing::Stage).is_none(), "the stage is the dock drag's");
        let held = [Some(Occupant::Pane(1)), Some(Occupant::Panel(Panel::Browsers))];
        let landed = landing(&browsers(), at(250.0, 50.0), Grid { up: true, panes: &held }, &spots());
        assert!(call(&browsers(), landed).is_none());
    }

    #[test]
    fn a_panels_header_moves_its_cell_as_a_panes_does() {
        let held = [Some(Occupant::Pane(1)), Some(Occupant::Panel(Panel::Browsers))];
        let carried = Carried::Cell { slot: 1, held: Occupant::Panel(Panel::Browsers), title: "Browsers".into() };
        let landed = landing(&carried, at(105.0, 50.0), Grid { up: true, panes: &held }, &spots());
        let asked = Call::MovePane { pane: Occupant::Panel(Panel::Browsers), onto: Occupant::Pane(1), zone: Zone::Left };
        assert_eq!(said(call(&carried, landed)), said(Some(asked)));
    }

    fn row(pane: u64, checkout: Option<u64>) -> Carried {
        Carried::Rail { pane, title: "p".into(), checkout }
    }

    #[test]
    fn a_header_over_another_cell_lands_on_the_zone_of_it() {
        let landed = landing(&header(), at(250.0, 50.0), grid(), &spots());
        assert!(matches!(landed, Landing::Cell { slot: 1, onto: Occupant::Pane(2), zone: Zone::Centre, .. }), "{landed:?}");
        let near = landing(&header(), at(205.0, 50.0), grid(), &spots());
        assert!(matches!(near, Landing::Cell { zone: Zone::Left, .. }), "{near:?}");
        let asked = Call::MovePane { pane: Occupant::Pane(1), onto: Occupant::Pane(2), zone: Zone::Centre };
        assert_eq!(said(call(&header(), landed)), said(Some(asked)));
    }

    #[test]
    fn a_header_is_never_dropped_on_its_own_cell_which_is_the_stage_under_it() {
        let landed = landing(&header(), at(150.0, 50.0), grid(), &spots());
        assert_eq!(landed, Landing::Stage);
        assert!(call(&header(), landed).is_none());
    }

    use argus_desktop_core::layout::docks::Panel;

    #[test]
    fn a_header_over_the_stage_on_no_cell_outlines_it_but_asks_for_nothing() {
        let spots = spots();
        spots.set(Spot::Stage, area(100.0, 0.0, 200.0, 300.0));
        let landed = landing(&header(), at(150.0, 200.0), grid(), &spots);
        assert_eq!(landed, Landing::Stage, "SHELL-GRID-11: the body is marked as a stage drop");
        assert!(call(&header(), landed.clone()).is_none(), "and letting go there sends nothing");
        assert_eq!(landing(&header(), at(500.0, 200.0), grid(), &spots), Landing::Nowhere, "off the stage there is no outline");
    }

    #[test]
    fn a_pane_from_the_rail_lands_on_a_cell_beside_it_by_the_zone() {
        let carried = row(9, Some(7));
        let landed = landing(&carried, at(295.0, 50.0), grid(), &spots());
        assert_eq!(said(call(&carried, landed)), said(Some(Call::DropPane { pane: 9, onto: Some(Occupant::Pane(2)), zone: Zone::Right })));
    }

    #[test]
    fn a_pane_from_the_rail_let_go_over_its_own_cell_asks_for_nothing() {
        let carried = row(1, Some(7));
        let landed = landing(&carried, at(150.0, 50.0), grid(), &spots());
        assert!(matches!(landed, Landing::Cell { onto: Occupant::Pane(1), .. }));
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
        let gone: [Option<Occupant>; 2] = [Some(Occupant::Pane(1)), None];
        let grid = Grid { up: true, panes: &gone };
        let carried = row(9, Some(7));
        assert_eq!(landing(&carried, at(250.0, 50.0), grid, &spots()), Landing::Stage);
        assert_eq!(landing(&header(), at(250.0, 50.0), grid, &spots()), Landing::Stage);
    }

    #[test]
    fn a_landing_nowhere_asks_for_nothing() {
        assert!(call(&header(), Landing::Nowhere).is_none());
        assert!(call(&row(1, Some(7)), Landing::Nowhere).is_none());
        assert!(call(&header(), Landing::Stage).is_none(), "a header cannot open a pane on the stage");
    }
}
