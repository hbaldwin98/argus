//! A pane's live screen, drawn cell by cell, and where its cursor goes.

use super::*;

pub(super) struct TermView<'a> {
    grid: Option<&'a Grid>,
}

/// Draws the pane and reports where the hardware cursor would go if this
/// pane owned it. Reporting rather than placing: `render` makes one cursor
/// decision for the whole frame (see [`render`]), so a pane drawn
/// underneath something else cannot strand its cursor on top of it.
/// Where the hardware cursor goes this frame, and what the child asked it
/// to look like. The two travel together because they come from the same
/// grid: the pane that owns the cursor owns its shape too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPlacement {
    pub position: Position,
    pub shape: CursorShape,
}

pub(super) fn render_term(
    f: &mut Frame,
    grid: Option<&Grid>,
    area: Rect,
    focused: bool,
) -> Option<CursorPlacement> {
    f.render_widget(TermView { grid }, area);
    term_cursor(grid, area, focused)
}

/// The child cursor mapped into `area`, or `None` when it must not be drawn.
///
/// Bounded by the grid as well as by the area. The two disagree for a frame
/// whenever a pane's on-screen size changes: the client draws the grid it
/// has, and only asks for a matching pty size afterwards, so a grid still
/// at the pty's 24x80 default can be drawn into a much larger box. Placing
/// the cursor at a coordinate the drawn rows don't reach puts it in empty
/// space — better to skip a frame than to point at nothing.
pub(super) fn term_cursor(grid: Option<&Grid>, area: Rect, focused: bool) -> Option<CursorPlacement> {
    // A parked view is history: the child's cursor belongs to the live
    // screen, which is not the one being drawn.
    let grid = grid.filter(|grid| focused && grid.cursor.visible && !grid.is_scrolled())?;
    let rows = grid.view().len();
    let cols = grid.view().first().map_or(0, Vec::len);
    let row = usize::from(grid.cursor.row);
    let col = usize::from(grid.cursor.col);
    if row >= rows.min(usize::from(area.height)) || col >= cols.min(usize::from(area.width)) {
        return None;
    }
    Some(CursorPlacement {
        position: Position::new(area.x + grid.cursor.col, area.y + grid.cursor.row),
        shape: grid.cursor.shape,
    })
}

impl Widget for TermView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some(grid) = self.grid else { return };
        for (r, row) in grid.view().iter().enumerate() {
            if r as u16 >= area.height {
                break;
            }
            for (c, cell) in row.iter().enumerate() {
                if c as u16 >= area.width {
                    break;
                }
                let Some(target) = buf.cell_mut((area.x + c as u16, area.y + r as u16)) else {
                    continue;
                };
                target.set_symbol(&cell.ch);
                let mut style = Style::default()
                    .fg(to_ratatui_color(cell.fg))
                    .bg(to_ratatui_color(cell.bg));
                if cell.bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.italic {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if cell.reverse {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                target.set_style(style);
            }
        }
    }
}

pub(super) fn to_ratatui_color(c: PColor) -> Color {
    match c {
        PColor::Default => Color::Reset,
        PColor::Idx(i) => Color::Indexed(i),
        PColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
