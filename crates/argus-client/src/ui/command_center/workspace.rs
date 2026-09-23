//! The Workspace stage: the selected pane's terminal under its breadcrumb
//! header.

use super::*;

pub(super) fn render_workspace(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    th: Theme,
) -> Option<CursorPlacement> {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3.min(area.height)), Constraint::Min(1)])
        .split(area);
    let (repo, checkout) = (
        app.current_repository()
            .map(|r| r.name.as_str())
            .unwrap_or("repository"),
        app.current_checkout()
            .map(|c| c.name.as_str())
            .unwrap_or("checkout"),
    );
    let pane = app.current_pane();
    let mut header = Line::from(vec![
        Span::styled(
            format!("  {repo}"),
            Style::default().fg(th.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" / ", Style::default().fg(th.edge)),
        Span::styled(
            checkout.to_string(),
            Style::default().fg(th.syntax.function),
        ),
    ]);
    let telemetry = pane
        .filter(|p| p.kind == PaneKind::Agent)
        .map(|p| (p.telemetry.clone(), p.status));
    if let Some(pane) = pane {
        header.push_span(Span::styled(" / ", Style::default().fg(th.edge)));
        header.push_span(Span::styled(
            format!("{} #{}", pane.title, pane.id.0),
            Style::default().fg(th.muted),
        ));
        // Plain space before the chip, and one cell of padding inside it.
        header.push_span(Span::raw("  "));
        header.push_span(Span::styled(
            format!(" {} ", short_status(pane.status)),
            filled_badge(status_color(pane.status, th), th),
        ));
    }
    let header_width = header.width();
    f.render_widget(
        Paragraph::new(header).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(th.edge))
                .padding(Padding::new(0, 0, 1, 0)),
        ),
        split[0],
    );
    // Telemetry sits at the right end of the breadcrumb, in whatever room
    // the path leaves it.
    if let (Some((telemetry, status)), true) = (telemetry, split[0].height >= 2) {
        let path_width = header_width + 2;
        let room = (split[0].width as usize).saturating_sub(path_width + 2);
        let line = telemetry_line(&telemetry, status, room, th);
        let width = line.width() as u16;
        if width > 0 {
            f.render_widget(
                Paragraph::new(line),
                Rect::new(split[0].right() - width - 2, split[0].y + 1, width, 1),
            );
        }
    }
    if split[0].x > 0 && split[0].height > 0 {
        f.render_widget(
            Paragraph::new(Span::styled("├", Style::default().fg(th.edge))),
            Rect {
                x: split[0].x - 1,
                y: split[0].bottom() - 1,
                width: 1,
                height: 1,
            },
        );
    }
    // The terminal fills the stage edge to edge, right up to the header rule.
    let terminal = split[1];
    let cursor = if let Some(id) = app.column_pane() {
        render_term(
            f,
            app.grids.get(&id),
            terminal,
            app.focus == Focus::PaneContent,
            Some(id),
            app.selection.as_ref(),
        )
    } else {
        f.render_widget(
            Paragraph::new("No pane selected\n\na  agent    s  shell")
                .style(Style::default().fg(th.dim)),
            terminal,
        );
        None
    };
    app.layout.content = Panel {
        outer: area,
        inner: terminal,
        first: 0,
    };
    cursor
}
