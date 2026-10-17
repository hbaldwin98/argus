//! The Views stage: what agents showed and the saved views, as a list above
//! the open one, which is drawn as text below it.
//!
//! The list is on top rather than beside the view so a narrow stage keeps
//! the view's full width, which a table needs more than a title does. The
//! live views come first, as the daemon sent them, and the saved ones after.

use argus_protocol::{SavedView, ViewScope};

use super::*;
use crate::app::Links;

/// Views listed at once; more scroll, following the open one.
const LISTED: usize = 5;

/// The stage item (its place among the live views, then the saved ones)
/// under a click on the list.
pub(crate) fn agent_view_at(app: &App, x: u16, y: u16) -> Option<usize> {
    let rows = app.layout.agent_views.inner;
    if !contains(rows, x, y) {
        return None;
    }
    let index = app.layout.agent_views.first + usize::from(y - rows.y);
    (index < app.stage_len()).then_some(index)
}

pub(super) fn render_agent_views(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    let held = app.stage_len();
    let live = app.agent_views.all().len();
    let detail = match held {
        0 => "what agents showed".to_string(),
        _ => format!("{live} shown by agents · {} saved", app.saved_views.all().len()),
    };
    render_stage_heading(
        f,
        area,
        "views",
        &detail,
        "n/p VIEW · l LINKS · r REFRESH · x CLOSE",
        th,
    );
    let body = Rect {
        x: area.x + 2,
        y: area.y + 4.min(area.height),
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(5),
    };
    if held == 0 || body.height == 0 {
        app.set_stage_window(0, 0);
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

    let open = app.stage_open().unwrap_or(0);
    let listed = held.min(LISTED).min(usize::from(body.height.saturating_sub(3)).max(1));
    let first = scrolled_to_show(app.layout.agent_views.first, Some(open), listed, held);
    let list = Rect { height: listed as u16, ..body };
    let now = argus_protocol::ago::now();
    for (row, index) in (first..held).take(listed).enumerate() {
        let selected = index == open;
        let bg = if selected { th.surface } else { th.bg };
        let mut spans = vec![Span::styled(
            if selected { MARKER } else { GUTTER },
            Style::default().fg(th.accent).bg(bg),
        )];
        if index < live {
            spans.extend(live_row(app, index, selected, bg, th, now));
        } else {
            spans.extend(saved_row(&app.saved_views.all()[index - live], selected, bg, th, now));
        }
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

    let rule_y = list.bottom();
    if rule_y >= body.bottom() {
        return;
    }
    let lines = stage_body(app, body.width as usize, th);
    if lines.is_empty() {
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
    let visible = text.height as usize;
    let top = app.stage_scroll().min(lines.len().saturating_sub(visible));
    app.set_stage_window(top, visible);
    let window: Vec<Line> = lines.iter().skip(top).take(visible).cloned().collect();
    f.render_widget(Paragraph::new(window), text);
    render_overflow(f, text, top, visible, lines.len(), app.focus_lit(Focus::View), th);
}

/// A live view's row after its marker: title, status, the pane and when.
fn live_row(
    app: &App,
    index: usize,
    selected: bool,
    bg: Color,
    th: Theme,
    now: u64,
) -> Vec<Span<'static>> {
    let shown = &app.agent_views.all()[index];
    let mut spans = vec![Span::styled(
        format!(" {}", one_line(&shown.view.title)),
        Style::default()
            .fg(th.text)
            .bg(bg)
            .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
    )];
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
    spans
}

/// A saved view's row after its marker: title, a stale mark, where it
/// belongs and when it was built.
fn saved_row(view: &SavedView, selected: bool, bg: Color, th: Theme, now: u64) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(
        format!(" {}", one_line(&view.view.title)),
        Style::default()
            .fg(th.text)
            .bg(bg)
            .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
    )];
    if view.stale {
        spans.push(Span::styled("  stale", Style::default().fg(th.warn).bg(bg)));
    }
    let ago = argus_protocol::ago::ago(u64::try_from(view.built_at).ok(), now);
    spans.push(Span::styled(
        format!("  {} · {ago}", scope_text(&view.scope)),
        Style::default().fg(th.dim).bg(bg),
    ));
    spans
}

/// Where a saved view belongs, as a person reads it.
fn scope_text(scope: &ViewScope) -> String {
    match scope {
        ViewScope::Global => "global".to_string(),
        ViewScope::Repository { .. } => "repository".to_string(),
        ViewScope::Feature { number, .. } => format!("feature #{number}"),
    }
}

/// What the open item draws under the rule: its heading and what it holds,
/// or the links list in its place while that is up. Empty when nothing is open.
fn stage_body(app: &App, width: usize, th: Theme) -> Vec<Line<'static>> {
    if app.saved_views.is_open() {
        return saved_body(app, width, th);
    }
    let Some(shown) = app.agent_views.opened() else {
        return Vec::new();
    };
    let now = argus_protocol::ago::now();
    let mut head = vec![bold_span(&shown.view.title, th)];
    if let Some(chip) = &shown.view.status {
        head.push(Span::raw(" "));
        head.push(chip_span(chip, th));
    }
    let mut lines = vec![Line::from(ellipsize_spans(head, width))];
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
        width,
    )));
    lines.push(Line::raw(""));
    lines.extend(block_lines(
        &app.saved_views.embedded(&shown.view.id, &shown.view.blocks),
        width,
        th,
    ));
    lines
}

fn saved_body(app: &App, width: usize, th: Theme) -> Vec<Line<'static>> {
    if let Some(links) = &app.saved_views.links {
        return links_lines(app, links, width, th);
    }
    let Some(view) = app.saved_views.opened() else {
        let id = app.saved_views.open_id().unwrap_or_default();
        return vec![
            Line::styled(format!("view {id} is not saved"), Style::default().fg(th.text)),
            Line::styled(
                "A link or an embed to it shows it once an agent saves it.",
                Style::default().fg(th.muted),
            ),
        ];
    };
    let now = argus_protocol::ago::now();
    let mut head = vec![bold_span(&view.view.title, th)];
    if view.stale {
        head.push(Span::raw(" "));
        head.push(Span::styled(
            "stale",
            Style::default().fg(th.warn).add_modifier(Modifier::BOLD),
        ));
    }
    let mut origin = format!(
        "{} · built {} ago",
        scope_text(&view.scope),
        argus_protocol::ago::ago(u64::try_from(view.built_at).ok(), now)
    );
    if let Some(commit) = &view.built_commit {
        origin.push_str(&format!(" · at {}", commit.chars().take(8).collect::<String>()));
    }
    let mut lines = vec![
        Line::from(ellipsize_spans(head, width)),
        Line::from(ellipsize_spans(
            vec![Span::styled(origin, Style::default().fg(th.dim))],
            width,
        )),
        Line::raw(""),
    ];
    lines.extend(block_lines(
        &app.saved_views.embedded(&view.id, &view.view.blocks),
        width,
        th,
    ));
    lines
}

/// The links an open saved view offers, the cursor on one of them.
fn links_lines(app: &App, links: &Links, width: usize, th: Theme) -> Vec<Line<'static>> {
    let from = app
        .saved_views
        .opened()
        .map_or_else(String::new, |view| one_line(&view.view.title));
    let mut lines = vec![
        Line::from(ellipsize_spans(
            vec![bold_span(&format!("links from {from}"), th)],
            width,
        )),
        Line::styled(
            "enter opens the one under the cursor; esc closes this list",
            Style::default().fg(th.muted),
        ),
        Line::raw(""),
    ];
    for (at, id) in links.ids.iter().enumerate() {
        let selected = at == links.at;
        let bg = if selected { th.surface } else { th.bg };
        let label = match app.saved_views.get(id) {
            Some(view) => one_line(&view.view.title),
            None => "not saved yet".to_string(),
        };
        let spans = vec![
            Span::styled(
                if selected { MARKER } else { GUTTER },
                Style::default().fg(th.accent).bg(bg),
            ),
            Span::styled(
                format!(" {id}"),
                Style::default()
                    .fg(th.text)
                    .bg(bg)
                    .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
            ),
            Span::styled(format!("  {label}"), Style::default().fg(th.dim).bg(bg)),
        ];
        lines.push(Line::from(ellipsize_spans(spans, width)));
    }
    lines
}

fn bold_span(text: &str, th: Theme) -> Span<'static> {
    Span::styled(
        one_line(text),
        Style::default().fg(th.text).add_modifier(Modifier::BOLD),
    )
}
