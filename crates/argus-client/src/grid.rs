use argus_protocol::{Cell, CellSpan, Cursor, MouseTracking};

/// A pane's view parked above its live screen.
///
/// Held alongside the live grid rather than replacing it: damage keeps
/// landing on `Grid::cells` the whole time, so dropping back to the bottom
/// is instant and never needs a fresh subscription.
pub struct Scrollback {
    /// Lines above the live screen. A grid showing the live screen holds
    /// `None` rather than an offset of zero.
    pub offset: u32,
    /// How far back the daemon says the buffer goes, so the client can stop
    /// at the top instead of asking for rows that do not exist.
    pub depth: u32,
    /// The rows the daemon last read at `offset`. Seeded from the live grid
    /// so the first scroll draws something rather than blanking the pane
    /// for the frame it takes the answer to arrive.
    pub cells: Vec<Vec<Cell>>,
}

pub struct Grid {
    pub cells: Vec<Vec<Cell>>,
    pub cursor: Cursor,
    /// What mouse reporting the child behind this grid has asked for.
    /// Nothing draws it; it decides whether a click or a wheel turn is
    /// forwarded into the pty or handled here.
    pub mouse: MouseTracking,
    /// Whether the child is drawing on the alternate screen. A wheel over
    /// that pane is a cursor key when `mouse` is off, so TUIs that never
    /// enable mouse tracking can still scroll.
    pub alternate_screen: bool,
    /// Where this pane is parked in its history, if it is not live.
    pub scrollback: Option<Scrollback>,
}

impl Grid {
    pub fn new(cells: Vec<Vec<Cell>>) -> Self {
        Grid {
            cells,
            cursor: Cursor::default(),
            mouse: MouseTracking::default(),
            alternate_screen: false,
            scrollback: None,
        }
    }

    pub fn with_cursor(cells: Vec<Vec<Cell>>, cursor: Cursor, mouse: MouseTracking) -> Self {
        Grid {
            cells,
            cursor,
            mouse,
            alternate_screen: false,
            scrollback: None,
        }
    }

    pub fn apply(&mut self, spans: &[CellSpan]) {
        for span in spans {
            if let Some(row) = self.cells.get_mut(span.row as usize) {
                for (i, cell) in span.cells.iter().enumerate() {
                    let col = span.col as usize + i;
                    if let Some(slot) = row.get_mut(col) {
                        *slot = cell.clone();
                    }
                }
            }
        }
    }

    pub fn move_cursor(&mut self, cursor: Cursor) {
        self.cursor = cursor;
    }

    /// The rows to draw: the parked view when there is one, otherwise live.
    pub fn view(&self) -> &[Vec<Cell>] {
        match &self.scrollback {
            Some(sb) => &sb.cells,
            None => &self.cells,
        }
    }

    pub fn is_scrolled(&self) -> bool {
        self.scrollback.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::ToCompactString;

    fn cell(c: char) -> Cell {
        Cell {
            ch: c.to_compact_string(),
            ..Default::default()
        }
    }

    fn grid(rows: &[&str]) -> Grid {
        Grid::new(rows.iter().map(|r| r.chars().map(cell).collect()).collect())
    }

    fn render(g: &Grid) -> Vec<String> {
        g.cells
            .iter()
            .map(|r| r.iter().map(|c| c.ch.as_str()).collect())
            .collect()
    }

    #[test]
    fn a_span_overwrites_exactly_its_cells() {
        let mut g = grid(&["abcde", "fghij"]);
        g.apply(&[CellSpan {
            row: 1,
            col: 1,
            cells: vec![cell('X'), cell('Y')],
        }]);
        assert_eq!(render(&g), vec!["abcde", "fXYij"]);
    }

    #[test]
    fn several_spans_apply_in_order() {
        let mut g = grid(&["aaa"]);
        g.apply(&[
            CellSpan {
                row: 0,
                col: 0,
                cells: vec![cell('X')],
            },
            CellSpan {
                row: 0,
                col: 0,
                cells: vec![cell('Y')],
            },
        ]);
        assert_eq!(render(&g), vec!["Yaa"]);
    }

    #[test]
    fn an_out_of_range_row_is_ignored_not_a_panic() {
        // Damage can arrive for a grid the client has since replaced with a
        // smaller snapshot; dropping it must never take the client down.
        let mut g = grid(&["ab"]);
        g.apply(&[CellSpan {
            row: 99,
            col: 0,
            cells: vec![cell('X')],
        }]);
        assert_eq!(render(&g), vec!["ab"]);
    }

    #[test]
    fn a_span_overrunning_the_row_writes_what_fits() {
        let mut g = grid(&["ab"]);
        g.apply(&[CellSpan {
            row: 0,
            col: 1,
            cells: vec![cell('X'), cell('Y'), cell('Z')],
        }]);
        assert_eq!(render(&g), vec!["aX"]);
    }

    #[test]
    fn attributes_are_carried_over_not_just_characters() {
        let mut g = grid(&["a"]);
        g.apply(&[CellSpan {
            row: 0,
            col: 0,
            cells: vec![Cell {
                ch: "a".into(),
                bold: true,
                ..Default::default()
            }],
        }]);
        assert!(g.cells[0][0].bold);
    }

    #[test]
    fn cursor_only_damage_updates_terminal_state() {
        let mut g = grid(&["a"]);
        g.move_cursor(Cursor {
            row: 3,
            col: 7,
            visible: false,
            ..Default::default()
        });
        assert_eq!(
            g.cursor,
            Cursor {
                row: 3,
                col: 7,
                visible: false,
                ..Default::default()
            }
        );
    }
}
