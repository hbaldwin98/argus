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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(s: &str) -> Vec<Cell> {
        s.chars()
            .map(|c| Cell {
                ch: c.to_string(),
                ..Default::default()
            })
            .collect()
    }

    fn spans_of(prev: &[&str], cur: &[&str]) -> Vec<CellSpan> {
        let prev: Vec<Vec<Cell>> = prev.iter().map(|r| row(r)).collect();
        let cur: Vec<Vec<Cell>> = cur.iter().map(|r| row(r)).collect();
        diff_grid(Some(&prev), &cur)
    }

    fn text_of(span: &CellSpan) -> String {
        span.cells.iter().map(|c| c.ch.as_str()).collect()
    }

    #[test]
    fn no_change_produces_no_spans() {
        assert!(spans_of(&["abc", "def"], &["abc", "def"]).is_empty());
    }

    #[test]
    fn a_single_changed_cell_ships_only_that_cell() {
        let spans = spans_of(&["abc"], &["aXc"]);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].row, spans[0].col), (0, 1));
        assert_eq!(text_of(&spans[0]), "X");
    }

    #[test]
    fn adjacent_changes_coalesce_into_one_span() {
        let spans = spans_of(&["abcde"], &["aXYZe"]);
        assert_eq!(spans.len(), 1, "one run, not three: {spans:?}");
        assert_eq!((spans[0].row, spans[0].col), (0, 1));
        assert_eq!(text_of(&spans[0]), "XYZ");
    }

    #[test]
    fn separated_changes_stay_separate_spans() {
        let spans = spans_of(&["abcde"], &["XbcdY"]);
        assert_eq!(spans.len(), 2);
        assert_eq!((spans[0].col, text_of(&spans[0])), (0, "X".to_string()));
        assert_eq!((spans[1].col, text_of(&spans[1])), (4, "Y".to_string()));
    }

    #[test]
    fn each_changed_row_gets_its_own_span_tagged_with_the_row() {
        let spans = spans_of(&["aa", "bb", "cc"], &["aa", "bX", "Yc"]);
        assert_eq!(spans.len(), 2);
        assert_eq!((spans[0].row, spans[0].col), (1, 1));
        assert_eq!((spans[1].row, spans[1].col), (2, 0));
    }

    #[test]
    fn no_previous_grid_ships_everything() {
        let cur: Vec<Vec<Cell>> = ["ab", "cd"].iter().map(|r| row(r)).collect();
        let spans = diff_grid(None, &cur);
        assert_eq!(spans.len(), 2, "one full-width span per row");
        assert_eq!(text_of(&spans[0]), "ab");
        assert_eq!(text_of(&spans[1]), "cd");
    }

    #[test]
    fn a_grown_grid_ships_the_newly_exposed_rows_whole() {
        // Resize grows the pane: rows the previous grid never had are
        // entirely new, so the whole row must ship.
        let prev: Vec<Vec<Cell>> = vec![row("ab")];
        let cur: Vec<Vec<Cell>> = vec![row("ab"), row("cd")];
        let spans = diff_grid(Some(&prev), &cur);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].row, text_of(&spans[0])), (1, "cd".to_string()));
    }

    #[test]
    fn a_widened_row_ships_only_the_new_columns() {
        let prev: Vec<Vec<Cell>> = vec![row("ab")];
        let cur: Vec<Vec<Cell>> = vec![row("abcd")];
        let spans = diff_grid(Some(&prev), &cur);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].col, text_of(&spans[0])), (2, "cd".to_string()));
    }

    #[test]
    fn attribute_only_changes_count_as_damage() {
        // Same character, different styling — the client would render the
        // wrong colour if this were treated as unchanged.
        let prev = vec![row("a")];
        let cur = vec![vec![Cell {
            ch: "a".to_string(),
            bold: true,
            ..Default::default()
        }]];
        let spans = diff_grid(Some(&prev), &cur);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].cells[0].bold);
    }

    #[test]
    fn default_cell_is_a_blank_with_default_colors() {
        let c = Cell::default();
        assert_eq!(c.ch, " ");
        assert_eq!(c.fg, Color::Default);
        assert_eq!(c.bg, Color::Default);
    }
}
