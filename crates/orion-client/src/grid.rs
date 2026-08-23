use orion_protocol::{Cell, CellSpan};

pub struct Grid {
    pub cells: Vec<Vec<Cell>>,
}

impl Grid {
    pub fn new(cells: Vec<Vec<Cell>>) -> Self {
        Grid { cells }
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
}
