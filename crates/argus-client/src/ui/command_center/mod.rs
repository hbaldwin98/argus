//! The flat command-center shell specified by `Argus Command Center.dc.html`.
//!
//! This module owns presentation only. Every selection still points into the
//! daemon's tree; the shell merely gives that tree a stable rail and focused
//! operational surfaces. Here: the frame that places the rail and a stage,
//! the stage heading and first-run surface, and the status and telemetry
//! vocabulary every stage shares. Each stage is its own submodule.

use super::*;
use crate::app::FeaturePanel;
use argus_protocol::{PaneKind, PaneStatus, TaskState};

mod checkouts;
mod feature;
mod panes;
mod rail;
mod workspace;

pub(crate) use checkouts::checkout_at;
use checkouts::render_checkouts;
pub(crate) use feature::feature_row_at;
use feature::render_feature_document;
pub(crate) use panes::pane_at;
use panes::render_panes;
pub(crate) use rail::{agent_at, project_header_at, rail_target_at, sidebar_contains, RailTarget};
use rail::render_sidebar;
use workspace::render_workspace;

/// The mark and the WORKSPACE tab: the rail's border continues the FEATURE
/// tab's left edge so the workspace highlight runs the full tab width.
/// Wide enough for the workspace summary badges (`repos`, `agents`, `needs
/// you`) on one row at two-digit counts without wrapping.
pub const SIDEBAR_WIDTH: u16 = 50;
const HEADER_HEIGHT: u16 = 2;

fn contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}

pub(super) fn render_command_center(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    th: Theme,
) -> Option<CursorPlacement> {
    let frame = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(HEADER_HEIGHT.min(area.height)),
            Constraint::Min(1),
        ])
        .split(area);
    render_view_tabs(f, app, frame[0], th);

    if app.tree.is_empty() {
        forget_spine(app);
        render_first_run(f, frame[1], th);
        return None;
    }

    let rail_width = rail_width(frame[1].width);
    let sidebar = Rect {
        width: rail_width,
        ..frame[1]
    };
    let stage = Rect {
        x: frame[1].x.saturating_add(rail_width),
        width: frame[1].width.saturating_sub(rail_width),
        ..frame[1]
    };
    render_sidebar(f, app, sidebar, th);
    // The junction only joins a plain rule; over the open tab's accent bar
    // it would cut the bar in two.
    let junction = (
        sidebar.right().saturating_sub(1),
        frame[0].bottom().saturating_sub(1),
    );
    let plain_rule = f
        .buffer_mut()
        .cell(junction)
        .is_some_and(|cell| cell.symbol() == "─");
    if frame[0].height > 1 && rail_width > 0 && plain_rule {
        f.render_widget(
            Paragraph::new(Span::styled("┬", Style::default().fg(th.edge))),
            Rect {
                x: sidebar.right().saturating_sub(1),
                y: frame[0].bottom().saturating_sub(1),
                width: 1,
                height: 1,
            },
        );
    }

    app.layout.repositories = Panel::default();
    app.layout.checkouts = Panel::default();
    app.layout.panes = Panel::default();
    app.layout.content = Panel::default();

    match app.view {
        View::Spine => render_workspace(f, app, stage, th),
        View::Feature => {
            render_feature_document(f, app, stage, th);
            None
        }
        View::Panes => {
            forget_feature_view(app);
            render_panes(f, app, stage, th);
            None
        }
        View::Checkouts => {
            forget_feature_view(app);
            render_checkouts(f, app, stage, th);
            None
        }
    }
}

/// The rail's width for a frame this wide. The tab strip reads it too, so
/// the rail's edge and the start of the FEATURE tab are one line.
pub(super) fn rail_width(total: u16) -> u16 {
    SIDEBAR_WIDTH.min(total.saturating_sub(24).max(1))
}

fn filled_badge(fg: Color, th: Theme) -> Style {
    Style::default().fg(fg).bg(th.surface)
}

fn render_stage_heading(
    f: &mut Frame,
    area: Rect,
    title: &str,
    detail: &str,
    action: &str,
    th: Theme,
) {
    let header = Rect {
        height: 3.min(area.height),
        ..area
    };
    f.render_widget(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(th.edge)),
        header,
    );
    if header.x > 0 && header.height > 0 {
        f.render_widget(
            Paragraph::new(Span::styled("├", Style::default().fg(th.edge))),
            Rect {
                x: header.x - 1,
                y: header.bottom() - 1,
                width: 1,
                height: 1,
            },
        );
    }
    let line_area = Rect {
        x: header.x + 2,
        y: header.y + 1.min(header.height.saturating_sub(1)),
        width: header.width.saturating_sub(4),
        height: 2.min(header.height.saturating_sub(1)),
    };
    let action_width = action.chars().count().min(line_area.width as usize) as u16;
    let title_width = title.chars().count() + 2;
    let inline = title_width as u16 + 2 + detail.chars().count() as u16 + action_width
        <= line_area.width;
    let title_line = Line::from(vec![
        Span::styled(
            title,
            Style::default().fg(th.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {detail}"),
            Style::default().fg(th.dim),
        ),
    ]);
    if inline {
        let detail_width = line_area
            .width
            .saturating_sub(action_width)
            .saturating_sub(title_width as u16) as usize;
        let detail = ellipsize_text(detail, detail_width);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    title,
                    Style::default().fg(th.text).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {detail}"), Style::default().fg(th.dim)),
            ])),
            Rect {
                height: 1,
                ..line_area
            },
        );
        f.render_widget(
            Paragraph::new(action).style(Style::default().fg(th.muted)),
            Rect {
                x: line_area.right().saturating_sub(action_width),
                width: action_width,
                height: 1,
                y: line_area.y,
            },
        );
    } else {
        f.render_widget(
            Paragraph::new(title_line),
            Rect {
                height: 1,
                ..line_area
            },
        );
        f.render_widget(
            Paragraph::new(action).style(Style::default().fg(th.muted)),
            Rect {
                x: line_area.right().saturating_sub(action_width),
                width: action_width,
                height: 1,
                y: line_area.y + 1,
            },
        );
    }
}

fn render_first_run(f: &mut Frame, area: Rect, th: Theme) {
    let width = 52.min(area.width);
    let height = 11.min(area.height);
    let box_area = centered_rect(width, height, area);
    f.render_widget(Paragraph::new(vec![
        Line::styled("■  NO WORKSPACE", Style::default().fg(th.dim)),
        Line::raw(""),
        Line::styled("Point Argus at a directory of repositories.", Style::default().fg(th.text).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::styled("Argus will index checkouts and worktrees, then keep panes attached to the branch they belong to.", Style::default().fg(th.muted)),
        Line::raw(""),
        Line::from(vec![Span::styled("› ", Style::default().fg(th.accent)), Span::styled("n  choose a directory to scan", Style::default().fg(th.muted))]),
        Line::from(vec![Span::styled("› ", Style::default().fg(th.accent)), Span::styled("or run  argus init <dir>", Style::default().fg(th.muted))]),
    ]).wrap(Wrap { trim: false }), box_area);
}

/// A status's glyph: the shared spinner while it works, a still mark
/// otherwise, so progress reads in the rail without opening the pane.
fn status_glyph(app: &App, status: PaneStatus, still: &'static str) -> &'static str {
    if status == PaneStatus::Working {
        crate::motion::spinner(app.frame_now(), app.epoch())
    } else {
        still
    }
}

/// The status most worth seeing among some panes: one asking for a person,
/// then one working, then one finished.
fn loudest_status<'a>(statuses: impl Iterator<Item = &'a PaneStatus>) -> Option<PaneStatus> {
    statuses.copied().max_by_key(|status| match status {
        PaneStatus::Waiting | PaneStatus::NeedsReview | PaneStatus::Failed => 4,
        PaneStatus::Working => 3,
        PaneStatus::Done => 2,
        PaneStatus::Idle => 1,
        PaneStatus::Exited { .. } => 0,
    })
}

pub(super) fn status_color(status: PaneStatus, th: Theme) -> Color {
    match status {
        PaneStatus::Waiting | PaneStatus::NeedsReview | PaneStatus::Failed => th.warn,
        PaneStatus::Working => th.ok,
        PaneStatus::Done => th.ok,
        PaneStatus::Idle | PaneStatus::Exited { .. } => th.dim,
    }
}

/// A token count as a person reads it: `980`, `4.2k`, `41k`, `1.2M`.
pub(super) fn compact_tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=9_999 => format!("{:.1}k", n as f64 / 1_000.0),
        10_000..=999_999 => format!("{}k", n / 1_000),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0),
    }
}

/// An agent's telemetry as short phrases, most useful first: the tool it is
/// in, how full its context is, what it has cost, and its model. Only what
/// the harness reported appears.
pub(super) fn telemetry_parts(
    telemetry: &argus_protocol::AgentTelemetry,
    status: PaneStatus,
    th: Theme,
) -> Vec<Span<'static>> {
    let mut parts = Vec::new();
    if let (PaneStatus::Working, Some(tool)) = (status, telemetry.tool.as_deref()) {
        parts.push(Span::styled(
            format!("▸ {tool}"),
            Style::default().fg(th.accent),
        ));
    }
    if let Some(used) = telemetry.context_tokens {
        let span = match telemetry.context_window.filter(|w| *w > 0) {
            Some(window) => {
                let percent = (used.saturating_mul(100) / window).min(100);
                let color = if percent >= 80 { th.warn } else { th.muted };
                Span::styled(
                    format!(
                        "ctx {}/{} {percent}%",
                        compact_tokens(used),
                        compact_tokens(window)
                    ),
                    Style::default().fg(color),
                )
            }
            None => Span::styled(
                format!("ctx {}", compact_tokens(used)),
                Style::default().fg(th.muted),
            ),
        };
        parts.push(span);
    }
    if let Some(cost) = telemetry.cost_usd {
        parts.push(Span::styled(
            format!("${cost:.2}"),
            Style::default().fg(th.muted),
        ));
    }
    if let Some(model) = &telemetry.model {
        parts.push(Span::styled(model.clone(), Style::default().fg(th.dim)));
    }
    parts
}

/// As many telemetry parts as fit in `width` cells, joined by dots.
pub(super) fn telemetry_line(
    telemetry: &argus_protocol::AgentTelemetry,
    status: PaneStatus,
    width: usize,
    th: Theme,
) -> Line<'static> {
    let mut spans = Vec::new();
    let mut used = 0;
    for part in telemetry_parts(telemetry, status, th) {
        let sep = if spans.is_empty() { 0 } else { 3 };
        let w = part.content.chars().count();
        if used + sep + w > width {
            break;
        }
        if sep > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(th.edge)));
        }
        used += sep + w;
        spans.push(part);
    }
    Line::from(spans)
}

fn short_status(status: PaneStatus) -> &'static str {
    match status {
        PaneStatus::Idle => "IDLE",
        PaneStatus::Working => "RUNNING",
        PaneStatus::Waiting => "NEEDS YOU",
        PaneStatus::NeedsReview => "REVIEW",
        PaneStatus::Done => "DONE",
        PaneStatus::Failed => "FAILED",
        PaneStatus::Exited { .. } => "EXITED",
    }
}
