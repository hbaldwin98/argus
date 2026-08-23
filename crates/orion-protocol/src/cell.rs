use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Color {
    #[default]
    Default,
    Idx(u8),
    Rgb(u8, u8, u8),
}

/// One character cell of terminal screen state, wire-sized to stay cheap to
/// diff and to ship: a damage span is a contiguous run of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub ch: String,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: " ".to_string(),
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            italic: false,
            underline: false,
            reverse: false,
        }
    }
}

/// A contiguous horizontal run of changed cells starting at (row, col).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellSpan {
    pub row: u16,
    pub col: u16,
    pub cells: Vec<Cell>,
}

/// Diff two equally-sized grids into the minimal set of changed spans.
pub fn diff_grid(prev: Option<&Vec<Vec<Cell>>>, cur: &[Vec<Cell>]) -> Vec<CellSpan> {
    let mut spans = Vec::new();
    for (row_idx, row) in cur.iter().enumerate() {
        let prev_row = prev.and_then(|p| p.get(row_idx));
        let mut col = 0usize;
        while col < row.len() {
            let changed = match prev_row {
                Some(pr) => pr.get(col) != Some(&row[col]),
                None => true,
            };
            if !changed {
                col += 1;
                continue;
            }
            let start = col;
            let mut cells = Vec::new();
            while col < row.len() {
                let still_changed = match prev_row {
                    Some(pr) => pr.get(col) != Some(&row[col]),
                    None => true,
                };
                if !still_changed {
                    break;
                }
                cells.push(row[col].clone());
                col += 1;
            }
            spans.push(CellSpan {
                row: row_idx as u16,
                col: start as u16,
                cells,
            });
        }
    }
    spans
}
