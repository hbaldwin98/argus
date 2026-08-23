use orion_protocol::{Color as PColor, PaneStatus};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Widget};
use ratatui::Frame;

use crate::app::{App, Focus};
use crate::grid::Grid;

pub fn render(f: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    if app.focus == Focus::PaneContent {
        render_pane_content(f, app, root[0]);
    } else {
        render_columns(f, app, root[0]);
    }
    render_status(f, app, root[1]);
}

fn render_columns(f: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(36),
            Constraint::Percentage(36),
        ])
        .split(area);

    let project_items: Vec<ListItem> = app
        .tree
        .iter()
        .map(|p| {
            let agents: usize = p.checkouts.iter().map(|c| c.panes.len()).sum();
            ListItem::new(format!("{}  {} checkouts, {} panes", p.name, p.checkouts.len(), agents))
        })
        .collect();
    render_column(
        f,
        cols[0],
        "projects",
        project_items,
        app.focus == Focus::Projects,
        Some(app.sel_project).filter(|_| !app.tree.is_empty()),
    );

    let checkout_items: Vec<ListItem> = app
        .current_project()
        .map(|p| {
            p.checkouts
                .iter()
                .map(|c| {
                    let status = worst_pane_status(c);
                    ListItem::new(format!("{} {}  {}", status_glyph(status), c.name, c.panes.len()))
                        .style(status_style(status))
                })
                .collect()
        })
        .unwrap_or_default();
    let ncheck = app.current_project().map(|p| p.checkouts.len()).unwrap_or(0);
    render_column(
        f,
        cols[1],
        "checkouts",
        checkout_items,
        app.focus == Focus::Checkouts,
        Some(app.sel_checkout).filter(|_| ncheck > 0),
    );

    let pane_items: Vec<ListItem> = app
        .current_checkout()
        .map(|c| {
            c.panes
                .iter()
                .map(|p| {
                    ListItem::new(format!("{} {:?} #{}", status_glyph(Some(p.status)), p.kind, p.id.0))
                        .style(status_style(Some(p.status)))
                })
                .collect()
        })
        .unwrap_or_default();
    let npane = app.current_checkout().map(|c| c.panes.len()).unwrap_or(0);
    render_column(
        f,
        cols[2],
        "panes",
        pane_items,
        app.focus == Focus::Panes,
        Some(app.sel_pane).filter(|_| npane > 0),
    );
}

fn render_column(
    f: &mut Frame,
    area: Rect,
    title: &str,
    items: Vec<ListItem>,
    active: bool,
    selected: Option<usize>,
) {
    let border_style = if active {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            if Some(i) == selected {
                item.style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                item
            }
        })
        .collect();
    f.render_widget(List::new(items), inner);
}

fn render_pane_content(f: &mut Frame, app: &App, area: Rect) {
    let title = match (app.current_project(), app.current_checkout(), app.current_pane()) {
        (Some(p), Some(c), Some(pane)) => format!(" {} / {} / {:?} #{} ", p.name, c.name, pane.kind, pane.id.0),
        _ => " pane ".to_string(),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(TermView { grid: &app.grid }, inner);
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let hint = if app.focus == Focus::PaneContent {
        if app.leader_pending {
            "leader…  esc: back to panes"
        } else {
            "ctrl-space then esc: back to panes"
        }
    } else {
        app.status.as_str()
    };
    f.render_widget(Paragraph::new(Line::from(Span::raw(hint))), area);
}

fn worst_pane_status(c: &orion_protocol::CheckoutInfo) -> Option<PaneStatus> {
    c.panes.iter().map(|p| p.status).max_by_key(rank)
}

fn rank(status: &PaneStatus) -> u8 {
    match status {
        PaneStatus::Running => 1,
        PaneStatus::Exited { code } if *code == Some(0) => 0,
        PaneStatus::Exited { .. } => 2,
    }
}

fn status_glyph(status: Option<PaneStatus>) -> &'static str {
    match status {
        None => "·",
        Some(PaneStatus::Running) => "◐",
        Some(PaneStatus::Exited { code: Some(0) }) => "✓",
        Some(PaneStatus::Exited { .. }) => "✗",
    }
}

fn status_style(status: Option<PaneStatus>) -> Style {
    match status {
        None => Style::default().fg(Color::DarkGray),
        Some(PaneStatus::Running) => Style::default().fg(Color::Blue),
        Some(PaneStatus::Exited { code: Some(0) }) => Style::default().fg(Color::Green),
        Some(PaneStatus::Exited { .. }) => Style::default().fg(Color::Red),
    }
}

struct TermView<'a> {
    grid: &'a Option<Grid>,
}

impl<'a> Widget for TermView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some(grid) = self.grid else { return };
        for (row_idx, row) in grid.cells.iter().enumerate() {
            if row_idx as u16 >= area.height {
                break;
            }
            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx as u16 >= area.width {
                    break;
                }
                let x = area.x + col_idx as u16;
                let y = area.y + row_idx as u16;
                let bc = &mut buf[(x, y)];
                bc.set_symbol(&cell.ch);
                bc.fg = to_ratatui_color(cell.fg);
                bc.bg = to_ratatui_color(cell.bg);
                let mut modifier = Modifier::empty();
                if cell.bold {
                    modifier |= Modifier::BOLD;
                }
                if cell.italic {
                    modifier |= Modifier::ITALIC;
                }
                if cell.underline {
                    modifier |= Modifier::UNDERLINED;
                }
                if cell.reverse {
                    modifier |= Modifier::REVERSED;
                }
                bc.modifier = modifier;
            }
        }
    }
}

fn to_ratatui_color(c: PColor) -> Color {
    match c {
        PColor::Default => Color::Reset,
        PColor::Idx(i) => Color::Indexed(i),
        PColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
