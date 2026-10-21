//! How many rows and columns a size in pixels holds.
//!
//! The room a pane reports to the daemon, as `panes/screen.ts` computes it:
//! whole cells only, and never fewer than one, since a pane sized to nothing
//! is a pane the daemon cannot draw.

/// The (rows, columns) a pixel size holds. The epsilon is for a width that is
/// exactly `n` cells in the font's units and lands a hair under it in `f32`.
pub fn grid_for(width: f32, height: f32, cell_w: f32, cell_h: f32) -> (u16, u16) {
    let fit = |room: f32, cell: f32| ((room / cell + 1e-3).floor().max(1.0)).min(f32::from(u16::MAX)) as u16;
    (fit(height, cell_h), fit(width, cell_w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_and_columns_are_what_whole_cells_fit() {
        assert_eq!(grid_for(1120.0, 800.0, 8.0, 20.0), (40, 140));
        assert_eq!(grid_for(1127.0, 819.0, 8.0, 20.0), (40, 140));
        assert_eq!(grid_for(1119.9, 799.0, 8.0, 20.0), (39, 139));
    }

    #[test]
    fn a_width_a_hair_under_a_whole_number_of_cells_still_counts_it() {
        let cell = 7.8_f32;
        assert_eq!(grid_for(cell * 140.0 - 0.0001, 20.0, cell, 20.0).1, 140);
    }

    #[test]
    fn a_pane_with_no_room_is_one_cell_never_zero() {
        assert_eq!(grid_for(0.0, 0.0, 8.0, 20.0), (1, 1));
        assert_eq!(grid_for(3.0, 5.0, 8.0, 20.0), (1, 1));
    }
}
