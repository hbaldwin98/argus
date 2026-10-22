//! The cells a drag across a screen covers, in reading order (SHELL-SCREEN-9).
//!
//! It is only drawn: the text a drag copies is read from the session's grid between the same two
//! cells (`Call::Copy`), so what is shown here and what is copied can differ by nothing but the
//! screen having moved on.

use argus_desktop_core::panes::pointer::Cell;

/// A drag's two ends, as pressed and as dragged to; either may come first on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Cell,
    pub head: Cell,
}

impl Selection {
    /// The two ends, the earlier in reading order first.
    pub fn ordered(&self) -> (Cell, Cell) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// The columns `row` has selected, from the first and up to the end of the last: the first
    /// row from its start column, the last to its end column inclusive, whole rows between. A
    /// screen of `rows` by `cols` draws no row it does not have.
    pub fn span(&self, row: u16, rows: u16, cols: u16) -> Option<(u16, u16)> {
        let (start, end) = self.ordered();
        if row >= rows || row < start.0 || row > end.0 {
            return None;
        }
        let from = if row == start.0 { start.1 } else { 0 };
        let to = if row == end.0 { end.1.saturating_add(1) } else { cols };
        Some((from.min(cols), to.min(cols)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(selection: Selection, rows: u16, cols: u16) -> Vec<Option<(u16, u16)>> {
        (0..rows).map(|row| selection.span(row, rows, cols)).collect()
    }

    #[test]
    fn a_drag_within_a_row_covers_both_ends_inclusive() {
        let selection = Selection { anchor: (1, 2), head: (1, 5) };
        assert_eq!(spans(selection, 3, 10), [None, Some((2, 6)), None]);
    }

    #[test]
    fn a_drag_over_rows_covers_the_first_from_its_start_the_last_to_its_end_and_the_rest_whole() {
        let selection = Selection { anchor: (0, 4), head: (3, 1) };
        assert_eq!(spans(selection, 5, 10), [Some((4, 10)), Some((0, 10)), Some((0, 10)), Some((0, 2)), None]);
    }

    #[test]
    fn a_backward_drag_covers_what_a_forward_one_does() {
        let forward = Selection { anchor: (1, 7), head: (2, 3) };
        let backward = Selection { anchor: (2, 3), head: (1, 7) };
        assert_eq!(spans(forward, 4, 20), spans(backward, 4, 20));
        assert_eq!(backward.ordered(), ((1, 7), (2, 3)));
    }

    #[test]
    fn a_row_the_screen_has_not_is_not_drawn() {
        let selection = Selection { anchor: (0, 0), head: (9, 3) };
        assert_eq!(spans(selection, 2, 10), [Some((0, 10)), Some((0, 10))]);
    }

    #[test]
    fn a_column_past_the_screens_width_is_held_to_it() {
        let selection = Selection { anchor: (0, 3), head: (0, u16::MAX) };
        assert_eq!(selection.span(0, 1, 8), Some((3, 8)));
    }
}
