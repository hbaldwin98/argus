//! Which zone of which cell a pane carried over the grid would be dropped on.
//!
//! A zone is the nearest edge when the pointer is within a quarter of the cell of it, else the
//! centre; what a drop there does is `split_grid`'s (`Layout::place`). The page's `dropZone`,
//! `zoneRect` and the cell search of its `target` are these.

use super::split_grid::{Rect, Zone};

/// The zone a pointer at `(fx, fy)`, fractions of the cell, is over. A tie between edges goes to
/// the first of left, right, top, bottom.
pub fn zone_at(fx: f32, fy: f32) -> Zone {
    let edges = [(fx, Zone::Left), (1.0 - fx, Zone::Right), (fy, Zone::Top), (1.0 - fy, Zone::Bottom)];
    let (mut near, mut zone) = edges[0];
    for (distance, candidate) in edges {
        if distance < near {
            (near, zone) = (distance, candidate);
        }
    }
    if near < 0.25 {
        zone
    } else {
        Zone::Centre
    }
}

/// What `zone` of a cell is lit while a pane is over it, as fractions of the cell.
pub fn zone_rect(zone: Zone) -> Rect {
    let (x, y, w, h) = match zone {
        Zone::Left => (0.0, 0.0, 0.5, 1.0),
        Zone::Right => (0.5, 0.0, 0.5, 1.0),
        Zone::Top => (0.0, 0.0, 1.0, 0.5),
        Zone::Bottom => (0.0, 0.5, 1.0, 0.5),
        Zone::Centre => (0.0, 0.0, 1.0, 1.0),
    };
    Rect { x, y, w, h }
}

/// The cell under `(x, y)` and the zone of it, among `cells` (a number and its rectangle, all in
/// one unit). The cell `own` is the pane's own place and is never a target. A point on a cell's
/// left or top edge is in it, on its right or bottom edge is not.
pub fn cell_under(cells: &[(usize, Rect)], x: f32, y: f32, own: Option<usize>) -> Option<(usize, Zone)> {
    cells.iter().filter(|(cell, _)| Some(*cell) != own).find_map(|(cell, r)| {
        let inside = x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
        inside.then(|| (*cell, zone_at((x - r.x) / r.w, (y - r.y) / r.h)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_the_middle_it_is_the_centre_and_near_an_edge_it_is_that_edge() {
        assert_eq!(zone_at(0.5, 0.5), Zone::Centre);
        assert_eq!(zone_at(0.1, 0.5), Zone::Left);
        assert_eq!(zone_at(0.9, 0.5), Zone::Right);
        assert_eq!(zone_at(0.5, 0.1), Zone::Top);
        assert_eq!(zone_at(0.5, 0.9), Zone::Bottom);
    }

    #[test]
    fn a_quarter_in_is_the_centre_and_just_inside_it_is_not() {
        assert_eq!(zone_at(0.25, 0.5), Zone::Centre);
        assert_eq!(zone_at(0.249, 0.5), Zone::Left);
    }

    #[test]
    fn a_corner_goes_to_the_nearer_edge_and_a_tie_to_the_first_of_left_right_top_bottom() {
        assert_eq!(zone_at(0.1, 0.2), Zone::Left);
        assert_eq!(zone_at(0.2, 0.1), Zone::Top);
        assert_eq!(zone_at(0.1, 0.1), Zone::Left);
        assert_eq!(zone_at(0.9, 0.9), Zone::Right);
        assert_eq!(zone_at(0.5, 0.9), Zone::Bottom);
    }

    #[test]
    fn the_lit_area_is_a_half_of_the_cell_for_a_side_and_all_of_it_for_the_centre() {
        assert_eq!(zone_rect(Zone::Left), Rect { x: 0.0, y: 0.0, w: 0.5, h: 1.0 });
        assert_eq!(zone_rect(Zone::Right), Rect { x: 0.5, y: 0.0, w: 0.5, h: 1.0 });
        assert_eq!(zone_rect(Zone::Top), Rect { x: 0.0, y: 0.0, w: 1.0, h: 0.5 });
        assert_eq!(zone_rect(Zone::Bottom), Rect { x: 0.0, y: 0.5, w: 1.0, h: 0.5 });
        assert_eq!(zone_rect(Zone::Centre), Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 });
    }

    fn two() -> Vec<(usize, Rect)> {
        vec![(0, Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 }), (1, Rect { x: 100.0, y: 0.0, w: 100.0, h: 100.0 })]
    }

    #[test]
    fn the_cell_under_the_pointer_and_the_zone_of_it() {
        assert_eq!(cell_under(&two(), 150.0, 50.0, None), Some((1, Zone::Centre)));
        assert_eq!(cell_under(&two(), 110.0, 50.0, None), Some((1, Zone::Left)));
        assert_eq!(cell_under(&two(), 50.0, 95.0, None), Some((0, Zone::Bottom)));
    }

    #[test]
    fn a_pane_is_never_dropped_on_its_own_cell() {
        assert_eq!(cell_under(&two(), 50.0, 50.0, Some(0)), None);
        assert_eq!(cell_under(&two(), 150.0, 50.0, Some(0)), Some((1, Zone::Centre)));
    }

    #[test]
    fn the_seam_belongs_to_the_cell_after_it_and_outside_the_grid_is_no_cell() {
        assert_eq!(cell_under(&two(), 100.0, 50.0, None).map(|(cell, _)| cell), Some(1));
        assert_eq!(cell_under(&two(), 200.0, 50.0, None), None);
        assert_eq!(cell_under(&two(), -1.0, 50.0, None), None);
    }
}
