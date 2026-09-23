//! The Panes stage: a responsive grid of pane cards, laid out by
//! `pane_card_rect` for both drawing and clicks.

use super::*;

fn pane_card_rect(body: Rect, index: usize) -> Rect {
    let columns = if body.width >= 90 {
        3
    } else if body.width >= 56 {
        2
    } else {
        1
    };
    let card_width = body.width / columns;
    let card_height = 7;
    let col = index as u16 % columns;
    let row = index as u16 / columns;
    Rect {
        x: body.x + col * card_width,
        y: body.y + row * card_height,
        width: card_width,
        height: card_height.min(body.bottom().saturating_sub(body.y + row * card_height)),
    }
}


pub(crate) fn pane_at(app: &App, x: u16, y: u16) -> Option<PaneLocation> {
    let body = app.layout.panes.inner;
    app.overview_pane_locations()
        .into_iter()
        .enumerate()
        .find_map(|(index, location)| {
            contains(pane_card_rect(body, index), x, y).then_some(location)
        })
}

pub(super) fn render_panes(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let repo = app
        .current_repository()
        .map(|repo| repo.name.as_str())
        .unwrap_or("repository");
    let detail = if app.show_all_panes {
        "all workspace panes".to_string()
    } else {
        format!("{repo} · repository panes")
    };
    render_stage_heading(f, area, "panes", &detail, "A ALL · a AGENT · s SHELL", th);
    let body = Rect {
        x: area.x + 2,
        y: area.y + 4.min(area.height),
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(5),
    };
    let locations = app.overview_pane_locations();
    app.layout.panes = Panel {
        outer: area,
        inner: body,
        first: 0,
    };
    for (index, location) in locations.iter().enumerate() {
        let rect = pane_card_rect(body, index);
        if rect.height == 0 {
            break;
        }
        let Some(pane) = app.pane_at(*location) else {
            continue;
        };
        let selected = app.pane_location() == Some(*location);
        let color = status_color(pane.status, th);
        let path = app
            .pane_path(*location)
            .map(|(_, r, c)| format!("{r} · {c}"))
            .unwrap_or_default();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if selected { th.accent } else { th.edge }))
            // Cards sit on the page itself; selection is the accent border.
            .style(Style::default().bg(th.bg));
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("■ ", Style::default().fg(color)),
                    Span::styled(short_status(pane.status), Style::default().fg(color)),
                    Span::styled(format!("  #{}", pane.id.0), Style::default().fg(th.dim)),
                ]),
                Line::styled(
                    ellipsize_text(&pane.title, inner.width as usize),
                    Style::default().fg(th.text).add_modifier(Modifier::BOLD),
                ),
                match (&pane.note, pane.telemetry.is_empty()) {
                    (None, false) => {
                        telemetry_line(&pane.telemetry, pane.status, inner.width as usize, th)
                    }
                    (note, _) => Line::styled(
                        note.clone().unwrap_or_else(|| "running pane".into()),
                        Style::default().fg(th.dim),
                    ),
                },
                Line::styled(path, Style::default().fg(th.muted)),
            ]),
            inner,
        );
    }
}
