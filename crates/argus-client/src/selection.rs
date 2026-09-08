//! A mouse selection over one pane's visible terminal cells.

use argus_protocol::{Cell, PaneId};
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellPoint {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSelection {
    pub pane: PaneId,
    pub anchor: CellPoint,
    pub head: CellPoint,
    area: Rect,
    moved: bool,
}

impl TerminalSelection {
    pub fn start(pane: PaneId, column: u16, row: u16, area: Rect) -> Self {
        let point = point_in(column, row, area);
        Self {
            pane,
            anchor: point,
            head: point,
            area,
            moved: false,
        }
    }

    pub fn update(&mut self, column: u16, row: u16) {
        self.head = point_in(column, row, self.area);
        self.moved |= self.head != self.anchor;
    }

    pub fn moved(&self) -> bool {
        self.moved
    }

    pub fn contains(&self, pane: PaneId, row: usize, col: usize) -> bool {
        if self.pane != pane {
            return false;
        }
        let point = CellPoint { row, col };
        let (start, end) = self.bounds();
        point >= start && point <= end
    }

    pub fn text(&self, cells: &[Vec<Cell>]) -> String {
        if !self.moved || cells.is_empty() {
            return String::new();
        }
        let (start, end) = self.bounds();
        let last_row = end.row.min(cells.len().saturating_sub(1));
        let mut lines = Vec::new();
        for (row_index, row) in cells
            .iter()
            .enumerate()
            .take(last_row + 1)
            .skip(start.row.min(last_row))
        {
            let (from, to) = line_bounds(row_index, row.len(), start, end);
            let line: String = row[from..to].iter().map(|cell| cell.ch.as_str()).collect();
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    fn bounds(&self) -> (CellPoint, CellPoint) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

fn line_bounds(
    row: usize,
    len: usize,
    start: CellPoint,
    end: CellPoint,
) -> (usize, usize) {
    let from = if row == start.row { start.col } else { 0 }.min(len);
    let to = if row == end.row {
        end.col.saturating_add(1)
    } else {
        len
    }
    .min(len);
    (from, to)
}

fn point_in(column: u16, row: u16, area: Rect) -> CellPoint {
    CellPoint {
        row: usize::from(
            row.saturating_sub(area.y)
                .min(area.height.saturating_sub(1)),
        ),
        col: usize::from(
            column
                .saturating_sub(area.x)
                .min(area.width.saturating_sub(1)),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::ToCompactString;

    fn cells(lines: &[&str]) -> Vec<Vec<Cell>> {
        lines
            .iter()
            .map(|line| {
                line.chars()
                    .map(|ch| Cell {
                        ch: ch.to_compact_string(),
                        ..Default::default()
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_reverse_multiline_drag_extracts_visible_text_without_terminal_padding() {
        let area = Rect::new(10, 5, 8, 3);
        let mut selection = TerminalSelection::start(PaneId(1), 13, 6, area);
        selection.update(11, 5);

        assert_eq!(
            selection.text(&cells(&["abcdefgh", "ijkl    "])),
            "bcdefgh\nijkl"
        );
    }

    #[test]
    fn a_click_without_a_drag_does_not_copy_a_character() {
        let selection = TerminalSelection::start(PaneId(1), 0, 0, Rect::new(0, 0, 2, 1));
        assert_eq!(selection.text(&cells(&["ab"])), "");
    }

    #[test]
    fn a_drag_outside_the_pane_is_clamped_to_its_last_cell() {
        let mut selection = TerminalSelection::start(PaneId(1), 1, 1, Rect::new(1, 1, 3, 2));
        selection.update(99, 99);
        assert_eq!(selection.head, CellPoint { row: 1, col: 2 });
    }
}
