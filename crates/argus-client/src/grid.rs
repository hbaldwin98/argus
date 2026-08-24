use argus_protocol::{Cell, CellSpan, Cursor};

pub struct Grid {
    pub cells: Vec<Vec<Cell>>,
    pub cursor: Cursor,
}

impl Grid {
    pub fn new(cells: Vec<Vec<Cell>>) -> Self {
        Grid {
            cells,
            cursor: Cursor::default(),
        }
    }

    pub fn with_cursor(cells: Vec<Vec<Cell>>, cursor: Cursor) -> Self {
        Grid { cells, cursor }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(c: char) -> Cell {
        Cell {
            ch: c.to_string(),
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
            CellSpan { row: 0, col: 0, cells: vec![cell('X')] },
            CellSpan { row: 0, col: 0, cells: vec![cell('Y')] },
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
                ch: "a".to_string(),
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
        });
        assert_eq!(g.cursor, Cursor { row: 3, col: 7, visible: false });
    }
}
