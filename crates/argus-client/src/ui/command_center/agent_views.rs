//! The Views stage: what agents showed, as a list of the views beside the
//! rail's content area and the open one drawn as text below it.
//!
//! The list is on top rather than beside the view so a narrow stage keeps
//! the view's full width, which a table needs more than a title does.

use super::*;

/// Views listed at once; more scroll, following the open one.
const LISTED: usize = 5;

/// The view (its place in the held list) under a click on the list.
pub(crate) fn agent_view_at(app: &App, x: u16, y: u16) -> Option<usize> {
    let rows = app.layout.agent_views.inner;
    if !contains(rows, x, y) {
        return None;
    }
    let index = app.layout.agent_views.first + usize::from(y - rows.y);
    (index < app.agent_views.all().len()).then_some(index)
}

pub(super) fn render_agent_views(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let held = app.agent_views.all().len();
    let detail = match held {
        0 => "what agents showed".to_string(),
        1 => "1 shown by an agent".to_string(),
        n => format!("{n} shown by agents"),
    };
    render_stage_heading(f, area, "views", &detail, "n/p VIEW · r REFRESH · x CLOSE", th);
    let body = Rect {
        x: area.x + 2,
        y: area.y + 4.min(area.height),
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(5),
    };
    if held == 0 || body.height == 0 {
        app.agent_views.page = 0;
        if body.height > 0 {
            f.render_widget(
                Paragraph::new(vec![
                    Line::styled("No agent has shown a view yet.", Style::default().fg(th.text)),
                    Line::styled(
                        "A view is what an agent read — a ticket, a run, a query — put in front of you. It appears here as it is shown.",
                        Style::default().fg(th.muted),
                    ),
                ])
                .wrap(Wrap { trim: false }),
                body,
            );
        }
        return;
    }

    let open = app.agent_views.open_index().unwrap_or(0);
    let listed = held.min(LISTED).min(usize::from(body.height.saturating_sub(3)).max(1));
    let first = scrolled_to_show(app.layout.agent_views.first, Some(open), listed, held);
    let list = Rect { height: listed as u16, ..body };
    let now = argus_protocol::ago::now();
    for (row, index) in (first..held).take(listed).enumerate() {
        let shown = &app.agent_views.all()[index];
        let selected = index == open;
        let bg = if selected { th.surface } else { th.bg };
        let mut spans = vec![
            Span::styled(
                if selected { MARKER } else { GUTTER },
                Style::default().fg(th.accent).bg(bg),
            ),
            Span::styled(
                format!(" {}", one_line(&shown.view.title)),
                Style::default()
                    .fg(th.text)
                    .bg(bg)
                    .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
            ),
        ];
        if let Some(chip) = &shown.view.status {
            spans.push(Span::styled(" ", Style::default().bg(bg)));
            let mut chip = chip_span(chip, th);
            chip.style = chip.style.bg(bg);
            spans.push(chip);
        }
        let from = app.pane_title(shown.pane).unwrap_or("a pane that has gone");
        let ago = argus_protocol::ago::ago(u64::try_from(shown.at).ok(), now);
        spans.push(Span::styled(
            format!("  {} · {ago}", one_line(from)),
            Style::default().fg(th.dim).bg(bg),
        ));
        let spans = ellipsize_spans(spans, list.width as usize);
        let used: usize = spans.iter().map(Span::width).sum();
        let mut line = spans;
        line.push(Span::styled(
            " ".repeat((list.width as usize).saturating_sub(used)),
            Style::default().bg(bg),
        ));
        f.render_widget(
            Paragraph::new(Line::from(line)),
            Rect { y: list.y + row as u16, height: 1, ..list },
        );
    }
    app.layout.agent_views = Panel { outer: area, inner: list, first };
    render_overflow(
        f,
        list,
        first,
        listed,
        held,
        app.focus_lit(Focus::View),
        th,
    );

    let Some(shown) = app.agent_views.opened() else {
        return;
    };
    let rule_y = list.bottom();
    if rule_y >= body.bottom() {
        return;
    }
    f.render_widget(
        Paragraph::new(Span::styled(
            "─".repeat(body.width as usize),
            Style::default().fg(th.edge),
        )),
        Rect { y: rule_y, height: 1, ..body },
    );

    // The open view's heading is part of what scrolls, so a short stage
    // does not spend its rows on a title it cannot also read under.
    let text = Rect {
        y: rule_y + 1,
        height: body.bottom().saturating_sub(rule_y + 1),
        ..body
    };
    let mut head = vec![bold_span(&shown.view.title, th)];
    if let Some(chip) = &shown.view.status {
        head.push(Span::raw(" "));
        head.push(chip_span(chip, th));
    }
    let mut lines = vec![Line::from(ellipsize_spans(head, text.width as usize))];
    let from = app.pane_title(shown.pane).unwrap_or("a pane that has gone");
    let mut origin = format!(
        "read by {} · {} ago",
        one_line(from),
        argus_protocol::ago::ago(u64::try_from(shown.at).ok(), now)
    );
    if let Some(subject) = &shown.subject {
        origin.push_str(&format!(" · {}", subject.key));
    }
    if let Some(secs) = shown.view.refresh_every {
        origin.push_str(&format!(" · asks again every {}", duration_text(u64::from(secs) * 1000)));
    }
    lines.push(Line::from(ellipsize_spans(
        vec![Span::styled(origin, Style::default().fg(th.dim))],
        text.width as usize,
    )));
    lines.push(Line::raw(""));
    lines.extend(block_lines(&shown.view.blocks, text.width as usize, th));

    let visible = text.height as usize;
    let top = app.agent_views.scroll.min(lines.len().saturating_sub(visible));
    app.agent_views.scroll = top;
    app.agent_views.page = visible;
    let window: Vec<Line> = lines.iter().skip(top).take(visible).cloned().collect();
    f.render_widget(Paragraph::new(window), text);
    render_overflow(f, text, top, visible, lines.len(), app.focus_lit(Focus::View), th);
}

fn bold_span(text: &str, th: Theme) -> Span<'static> {
    Span::styled(
        one_line(text),
        Style::default().fg(th.text).add_modifier(Modifier::BOLD),
    )
}
