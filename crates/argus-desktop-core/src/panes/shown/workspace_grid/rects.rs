//! Where each cell of the Workspace's grid falls, from the sizes the rail carries.
//!
//! The rail sends each row's height and each pane's width in its row in thousandths
//! (`GridPage`); a window draws them as fractions of the grid, row by row, left to right. The
//! page's `cellRects` and this are the same sum.

use super::split_grid::Rect;

/// Every cell's rectangle as fractions of the grid, row by row. A row with no widths has no
/// cells; a width list longer than the row's panes is the caller's to have checked.
pub fn rects(heights: &[u16], widths: &[Vec<u16>]) -> Vec<Vec<Rect>> {
    let mut y = 0.0;
    heights
        .iter()
        .enumerate()
        .map(|(r, &height)| {
            let h = f32::from(height) / 1000.0;
            let mut x = 0.0;
            let row = widths
                .get(r)
                .map(|row| {
                    row.iter()
                        .map(|&width| {
                            let w = f32::from(width) / 1000.0;
                            let rect = Rect { x, y, w, h };
                            x += w;
                            rect
                        })
                        .collect()
                })
                .unwrap_or_default();
            y += h;
            row
        })
        .collect()
}

/// Each cell's rectangle by its number: `grid` says which cell sits where, as the rail sends it.
pub fn cells(grid: &[Vec<u8>], heights: &[u16], widths: &[Vec<u16>]) -> Vec<(u8, Rect)> {
    let rects = rects(heights, widths);
    grid.iter()
        .enumerate()
        .flat_map(|(r, row)| row.iter().enumerate().filter_map(|(c, &cell)| Some((cell, *rects.get(r)?.get(c)?))).collect::<Vec<_>>())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_tile_the_grid_row_by_row() {
        let rects = rects(&[600, 400], &[vec![300, 700], vec![1000]]);
        assert_eq!(rects[0][0], Rect { x: 0.0, y: 0.0, w: 0.3, h: 0.6 });
        assert_eq!(rects[0][1], Rect { x: 0.3, y: 0.0, w: 0.7, h: 0.6 });
        assert_eq!(rects[1][0], Rect { x: 0.0, y: 0.6, w: 1.0, h: 0.4 });
    }

    #[test]
    fn a_row_the_widths_do_not_name_has_no_cells_and_the_rows_below_still_start_under_it() {
        let rects = rects(&[500, 500], &[vec![1000]]);
        assert!(rects[1].is_empty());
        assert_eq!(rects[0][0].h, 0.5);
    }

    #[test]
    fn each_cell_number_gets_the_rectangle_of_its_place() {
        // Three panes: the Workspace's and the second above, the third under.
        let placed = cells(&[vec![0, 1], vec![2]], &[500, 500], &[vec![500, 500], vec![1000]]);
        assert_eq!(placed.iter().map(|(n, _)| *n).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(placed[1].1, Rect { x: 0.5, y: 0.0, w: 0.5, h: 0.5 });
        assert_eq!(placed[2].1, Rect { x: 0.0, y: 0.5, w: 1.0, h: 0.5 });
    }

    #[test]
    fn a_grid_that_names_more_cells_than_it_has_sizes_for_places_only_the_ones_it_can() {
        let placed = cells(&[vec![0, 1, 2]], &[1000], &[vec![500, 500]]);
        assert_eq!(placed.len(), 2);
    }
}
