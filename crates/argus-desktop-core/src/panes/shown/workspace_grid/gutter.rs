//! What dragging a gutter does to the two panes it parts, as arithmetic.
//!
//! A drag changes the sizes of the two panes either side of the gutter and nothing else; neither
//! goes below the least a pane may be, and a pair with room for less than two of them does not
//! move. Sizes are turned back into whole thousandths summing to exactly 1000, which is what
//! `set_shares` carries and the rail echoes. The page's `dragGutter` and `toThousandths`, and the
//! pixel minimums of its `minimum`, are these.

/// Which way a gutter divides: a vertical gutter parts the widths of one row, a horizontal one
/// the heights of the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Across,
    Down,
}

/// The sizes of the rows and of each row's panes, in thousandths, as the rail carries them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shares {
    pub heights: Vec<u16>,
    pub widths: Vec<Vec<u16>>,
}

/// The sizes of a row's panes in pixels once the gutter after pane `at` is dragged by `delta`.
pub fn drag(sizes: &[f32], at: usize, delta: f32, min: f32) -> Vec<f32> {
    let mut next = sizes.to_vec();
    let (Some(&before), Some(&after)) = (sizes.get(at), sizes.get(at + 1)) else { return next };
    let total = before + after;
    if total < 2.0 * min {
        return next;
    }
    let first = (before + delta).max(min).min(total - min);
    next[at] = first;
    next[at + 1] = total - first;
    next
}

/// Sizes as whole thousandths summing to exactly 1000, the rounding left over going to the
/// largest remainders (the earlier one on a tie).
pub fn thousandths(sizes: &[f32]) -> Vec<u16> {
    let total: f32 = sizes.iter().sum();
    if sizes.is_empty() {
        return Vec::new();
    }
    // Sizes that add to nothing (or to something unusable) are made even; the same pass then
    // spreads the thousandths an even split cannot, so three entries still sum to 1000.
    if total <= 0.0 || !total.is_finite() {
        return thousandths(&vec![1.0; sizes.len()]);
    }
    let exact: Vec<f32> = sizes.iter().map(|s| s / total * 1000.0).collect();
    let mut whole: Vec<u16> = exact.iter().map(|e| e.floor() as u16).collect();
    let mut left = 1000 - i32::from(whole.iter().sum::<u16>());
    let mut order: Vec<usize> = (0..exact.len()).collect();
    order.sort_by(|&a, &b| (exact[b] - exact[b].floor()).total_cmp(&(exact[a] - exact[a].floor())).then(a.cmp(&b)));
    for i in order {
        if left <= 0 {
            break;
        }
        whole[i] += 1;
        left -= 1;
    }
    whole
}

/// The fewest pixels a pane may have across: its columns and the room round them.
pub fn least_across(min_cols: u16, cell_w: f32, padding: f32) -> f32 {
    f32::from(min_cols) * cell_w + padding
}

/// The fewest pixels a pane may have down: its rows, the room round them and its header.
pub fn least_down(min_rows: u16, cell_h: f32, padding: f32, head: f32) -> f32 {
    f32::from(min_rows) * cell_h + padding + head
}

impl Shares {
    /// The sizes once the gutter after pane `at` of `row` (`Across`), or under row `row`
    /// (`Down`; `at` is then the row), is dragged by `delta` pixels in a grid `total` pixels
    /// wide or high. A gutter that is not there changes nothing.
    pub fn dragged(&self, axis: Axis, row: usize, at: usize, delta: f32, total: f32, least: f32) -> Shares {
        let pixels = |list: &[u16]| list.iter().map(|&t| f32::from(t) / 1000.0 * total).collect::<Vec<_>>();
        match axis {
            Axis::Across => {
                let Some(sizes) = self.widths.get(row) else { return self.clone() };
                let mut widths = self.widths.clone();
                widths[row] = thousandths(&drag(&pixels(sizes), at, delta, least));
                Shares { heights: self.heights.clone(), widths }
            }
            Axis::Down => Shares { heights: thousandths(&drag(&pixels(&self.heights), at, delta, least)), widths: self.widths.clone() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_a_gutter_moves_the_edge_between_its_two_panes_and_leaves_the_rest() {
        assert_eq!(drag(&[100.0, 100.0, 100.0], 0, 30.0, 20.0), [130.0, 70.0, 100.0]);
        assert_eq!(drag(&[100.0, 100.0, 100.0], 1, -30.0, 20.0), [100.0, 70.0, 130.0]);
    }

    #[test]
    fn a_drag_stops_at_the_least_a_pane_may_be() {
        assert_eq!(drag(&[100.0, 100.0], 0, 500.0, 20.0), [180.0, 20.0]);
        assert_eq!(drag(&[100.0, 100.0], 0, -500.0, 20.0), [20.0, 180.0]);
    }

    #[test]
    fn a_pair_with_room_for_less_than_two_least_sizes_stays_as_it_is() {
        assert_eq!(drag(&[30.0, 30.0], 0, 10.0, 40.0), [30.0, 30.0]);
    }

    #[test]
    fn a_gutter_that_is_not_there_changes_nothing() {
        assert_eq!(drag(&[100.0, 100.0], 1, 10.0, 20.0), [100.0, 100.0]);
        assert_eq!(drag(&[], 0, 10.0, 20.0), Vec::<f32>::new());
    }

    #[test]
    fn rounding_left_over_goes_to_the_largest_remainders() {
        assert_eq!(thousandths(&[1.0, 1.0, 1.0]), [334, 333, 333]);
        assert_eq!(thousandths(&[2.0, 1.0]), [667, 333]);
        assert_eq!(thousandths(&[1.0]), [1000]);
    }

    #[test]
    fn sizes_that_add_to_nothing_are_made_even() {
        assert_eq!(thousandths(&[0.0, 0.0]), [500, 500]);
        assert_eq!(thousandths(&[0.0, 0.0, 0.0]), [334, 333, 333]);
        assert_eq!(thousandths(&[0.0; 7]).iter().map(|&t| u32::from(t)).sum::<u32>(), 1000);
        assert!(thousandths(&[]).is_empty());
    }

    #[test]
    fn the_least_is_the_cells_the_rail_names_and_the_room_round_them() {
        assert_eq!(least_across(20, 8.0, 12.0), 172.0);
        assert_eq!(least_down(6, 20.0, 12.0, 32.0), 164.0);
    }

    fn shares() -> Shares {
        Shares { heights: vec![500, 500], widths: vec![vec![500, 500], vec![1000]] }
    }

    #[test]
    fn a_vertical_gutter_sizes_the_panes_of_its_row_only() {
        let dragged = shares().dragged(Axis::Across, 0, 0, 100.0, 1000.0, 100.0);
        assert_eq!(dragged.widths, [vec![600, 400], vec![1000]]);
        assert_eq!(dragged.heights, [500, 500]);
    }

    #[test]
    fn a_horizontal_gutter_sizes_the_rows_and_no_pane() {
        let dragged = shares().dragged(Axis::Down, 0, 0, -100.0, 800.0, 100.0);
        assert_eq!(dragged.heights, [375, 625]);
        assert_eq!(dragged.widths, shares().widths);
    }

    #[test]
    fn a_row_that_is_not_there_has_no_gutter() {
        assert_eq!(shares().dragged(Axis::Across, 5, 0, 100.0, 1000.0, 100.0), shares());
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // The daemon declines a set that is not the grid's shape, so what a drag sends must
            // add up (SHELL-GRID-8, `dragged_sizes_must_keep_the_shape_and_are_normalised`).
            #[test]
            fn thousandths_always_add_to_exactly_a_thousand(sizes in proptest::collection::vec(0.1f32..2000.0, 1..8)) {
                let list = thousandths(&sizes);
                prop_assert_eq!(list.len(), sizes.len());
                prop_assert_eq!(list.iter().map(|&t| u32::from(t)).sum::<u32>(), 1000);
            }

            // The same, for sizes that add to nothing (a pane with no room reported yet).
            #[test]
            fn zero_sizes_still_add_to_exactly_a_thousand(len in 1usize..12) {
                prop_assert_eq!(thousandths(&vec![0.0; len]).iter().map(|&t| u32::from(t)).sum::<u32>(), 1000);
            }

            // SHELL-GRID-8: only the two panes either side change, and neither goes below its least.
            #[test]
            fn a_drag_changes_only_the_pair_keeps_the_total_and_the_least(
                sizes in proptest::collection::vec(60.0f32..500.0, 2..6),
                delta in -2000.0f32..2000.0,
                pick in 0usize..5,
            ) {
                let at = pick % (sizes.len() - 1);
                let next = drag(&sizes, at, delta, 50.0);
                for (i, (a, b)) in sizes.iter().zip(&next).enumerate() {
                    if i != at && i != at + 1 {
                        prop_assert_eq!(a, b);
                    }
                }
                prop_assert!((sizes[at] + sizes[at + 1] - next[at] - next[at + 1]).abs() < 0.01);
                prop_assert!(next[at] >= 50.0 - 0.01 && next[at + 1] >= 50.0 - 0.01);
            }
        }
    }
}
