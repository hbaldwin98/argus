//! The Feature stage's frame: where it is said to be, the open feature's
//! heading and lenses, the line being typed, and which row a click lands on.
//! What fills the frame is a submodule each — the index, the three lenses,
//! and the note a feature is written as.

use super::*;
use crate::app::{FeatureHit, FeatureRow, Lens, Standing};
use argus_protocol::board::outline::{Outline, WorkRef};
use argus_protocol::{Decision, DecisionState, DecisionTreeRow, Task, TaskTreeRow};

mod capture;
mod flows;
mod index;
mod record;
mod work;

/// Below this the detail column costs the list more than it says.
const TWO_COLUMNS_FROM: u16 = 72;

// ---- drawing vocabulary the stage's parts share -------------------------

fn span(text: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(color))
}

fn strong(text: impl Into<String>, th: Theme) -> Span<'static> {
    Span::styled(
        text.into(),
        Style::default().fg(th.text).add_modifier(Modifier::BOLD),
    )
}

fn width_of(spans: &[Span<'_>]) -> usize {
    spans.iter().map(Span::width).sum()
}

/// `left`, then `right` pushed to the far edge. When both cannot fit it is
/// the left that gives way: what is on the right is the short fact a
/// column of rows is scanned for.
fn spread(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: u16) -> Vec<Span<'static>> {
    let width = usize::from(width);
    let room = width.saturating_sub(width_of(&right) + usize::from(!right.is_empty()));
    let mut spans = ellipsize_spans(left, room);
    let gap = width.saturating_sub(width_of(&spans) + width_of(&right));
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);
    spans
}

/// One row of a list: the cursor's is a bar across the column, marked at
/// its edge, so a selection reads from across the room.
fn row(spans: Vec<Span<'static>>, selected: bool, width: u16, th: Theme) -> Line<'static> {
    let mut line = vec![span(if selected { "▌ " } else { "  " }, th.accent)];
    line.extend(spans);
    let pad = usize::from(width).saturating_sub(width_of(&line));
    line.push(Span::raw(" ".repeat(pad)));
    let bg = if selected { th.surface } else { th.bg };
    Line::from(line).style(Style::default().bg(bg))
}

/// A section's label, with what it is about pushed to the right.
fn heading(label: &str, lit: bool, about: &str, width: u16, th: Theme) -> Line<'static> {
    let color = if lit { th.accent } else { th.muted };
    Line::from(spread(
        vec![span(format!("  {label}"), color)],
        vec![span(about.to_string(), th.dim)],
        width,
    ))
}

/// Prose wrapped to a column, indented to sit under its heading.
fn prose(text: &str, width: u16, color: Color) -> Vec<Line<'static>> {
    text.lines()
        .flat_map(|line| wrap(line, width.saturating_sub(3)))
        .map(|line| Line::from(span(format!("  {line}"), color)))
        .collect()
}

/// Draws `lines` from `first` into `area`, one per row.
fn paint(f: &mut Frame, lines: &[Line<'static>], area: Rect, first: usize) {
    let visible: Vec<Line<'static>> = lines
        .iter()
        .skip(first)
        .take(usize::from(area.height))
        .cloned()
        .collect();
    f.render_widget(Paragraph::new(visible), area);
}

/// A rule across the stage, joined to the rail's edge.
fn rule(f: &mut Frame, area: Rect, y: u16, th: Theme) {
    if y >= area.bottom() {
        return;
    }
    let line = "─".repeat(usize::from(area.width));
    f.render_widget(
        Paragraph::new(span(line, th.edge)),
        Rect { y, height: 1, ..area },
    );
    if area.x > 0 {
        f.render_widget(
            Paragraph::new(span("├", th.edge)),
            Rect {
                x: area.x - 1,
                y,
                width: 1,
                height: 1,
            },
        );
    }
}

/// How far along, as cells: what is done lit, what is left a track.
fn progress(done: usize, total: usize, cells: usize, th: Theme) -> Vec<Span<'static>> {
    let lit = if total == 0 { 0 } else { (done * cells).div_ceil(total).min(cells) };
    vec![
        span("█".repeat(lit), th.ok),
        span("█".repeat(cells - lit), th.surface_focus),
    ]
}

fn task_mark(task: &Task, held: bool, th: Theme) -> Span<'static> {
    match (task.state, held) {
        (TaskState::Done, _) => span("✓", th.ok),
        (_, true) => span("⊘", th.err),
        (TaskState::Doing, _) => span("◐", th.warn),
        (TaskState::Todo, _) => span("○", th.dim),
    }
}

/// Whether a decision no longer stands — replaced, taken back, or turned
/// down. Drawn receded either way: still there to be read, no longer what
/// the work is built on.
fn set_aside(decision: &Decision) -> bool {
    decision.superseded() || decision.withdrawn() || decision.state == DecisionState::Rejected
}

fn decision_mark(decision: &Decision, th: Theme) -> Span<'static> {
    if decision.withdrawn() || decision.superseded() {
        return span("✗", th.dim);
    }
    match decision.state {
        DecisionState::Accepted => span("✓", th.ok),
        DecisionState::Open => span("?", th.warn),
        DecisionState::Rejected => span("✗", th.err),
    }
}

/// How a decision stands, in a word.
fn decision_status(decision: &Decision, th: Theme) -> Span<'static> {
    if decision.withdrawn() {
        return span("withdrawn", th.dim);
    }
    if decision.superseded() {
        return span("superseded", th.dim);
    }
    match decision.state {
        DecisionState::Accepted => span("accepted", th.ok),
        DecisionState::Open => span("open", th.warn),
        DecisionState::Rejected => span("rejected", th.err),
    }
}

/// A decision's name, struck through once it is set aside.
fn decision_title(decision: &Decision, color: Color) -> Span<'static> {
    let style = Style::default().fg(color);
    Span::styled(
        decision.chose.clone(),
        match set_aside(decision) {
            true => style.add_modifier(Modifier::CROSSED_OUT),
            false => style,
        },
    )
}

fn pane_label(pane: argus_protocol::PaneId) -> String {
    format!("pane #{}", pane.0)
}

/// The keys the detail column offers, along its last row: as many as fit
/// whole, since half a hint names a key that is not there.
fn key_line(keys: &[(&str, &str)], width: u16, th: Theme) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    for (key, does) in keys {
        let hint = [strong(*key, th), span(format!(" {does}   "), th.muted)];
        if width_of(&spans) + width_of(&hint) > usize::from(width) + 3 {
            break;
        }
        spans.extend(hint);
    }
    Line::from(spans)
}

/// The two columns of a lens: the list, and the detail beside it. On a
/// stage too narrow for both, whichever has the keys takes all of it.
fn columns(app: &App, body: Rect, list_share: u16) -> (Rect, Rect) {
    let none = Rect { width: 0, ..body };
    if body.width < TWO_COLUMNS_FROM {
        return match app.in_detail {
            true => (none, body),
            false => (body, none),
        };
    }
    let list_width = body.width * list_share / 100;
    let list = Rect {
        width: list_width,
        ..body
    };
    let detail = Rect {
        x: body.x + list_width + 1,
        width: body.width - list_width - 1,
        ..body
    };
    (list, detail)
}

fn divider(f: &mut Frame, list: Rect, detail: Rect, th: Theme) {
    if list.width == 0 || detail.width == 0 {
        return;
    }
    let bar = vec![Line::from(span("│", th.edge)); usize::from(list.height)];
    f.render_widget(
        Paragraph::new(bar),
        Rect {
            x: list.right(),
            width: 1,
            ..list
        },
    );
}

/// A list column: its heading, then its rows scrolled to keep the cursor
/// on screen. Returns the panel a click is resolved against.
fn list_column(
    f: &mut Frame,
    area: Rect,
    head: Line<'static>,
    rows: &[Line<'static>],
    selected: Option<usize>,
    was_first: usize,
) -> Panel {
    if area.width == 0 || area.height == 0 {
        return Panel::default();
    }
    paint(f, &[head], Rect { height: 1, ..area }, 0);
    let inner = Rect {
        y: area.y + 2.min(area.height),
        height: area.height.saturating_sub(2),
        ..area
    };
    let first = scrolled_to_show(was_first, selected, usize::from(inner.height), rows.len());
    paint(f, rows, inner, first);
    Panel {
        outer: area,
        inner,
        first,
    }
}

/// A detail column: its lines from the top, and its keys on the last row.
fn detail_column(
    f: &mut Frame,
    area: Rect,
    lines: &[Line<'static>],
    keys: &[(&str, &str)],
    cursor_line: Option<usize>,
    th: Theme,
) -> Panel {
    if area.width == 0 || area.height < 2 {
        return Panel::default();
    }
    let text = Rect {
        height: area.height - 1,
        ..area
    };
    let first = scrolled_to_show(0, cursor_line, usize::from(text.height), lines.len());
    paint(f, lines, text, first);
    paint(
        f,
        &[key_line(keys, area.width, th)],
        Rect {
            y: area.bottom() - 1,
            height: 1,
            ..area
        },
        0,
    );
    Panel {
        outer: area,
        inner: text,
        first,
    }
}

// ---- the frame ----------------------------------------------------------

/// The Feature stage, with the line being typed on its last row.
///
/// A row taken off the bottom rather than a window floating over the
/// stage: what you are writing and what is already there have to be
/// readable at the same time.
pub(super) fn render_feature_stage(f: &mut Frame, app: &mut App, area: Rect, th: Theme) {
    forget_feature_view(app);
    let mut stage = area;
    if let Some(input) = app.line.as_ref() {
        stage.height = area.height.saturating_sub(1);
        let line = Line::from(vec![
            span(format!("  {} ", input.label()), th.accent),
            span(input.text.clone(), th.text),
            span("_", th.accent),
        ]);
        paint(
            f,
            &[line],
            Rect {
                y: area.bottom().saturating_sub(1),
                height: 1.min(area.height),
                ..area
            },
            0,
        );
    }
    if stage.height < 4 || stage.width < 12 {
        return;
    }

    let repo = app
        .current_repository()
        .map(|r| r.name.clone())
        .or_else(|| app.board_follower.board().map(|b| b.name.clone()))
        .unwrap_or_else(|| "features".into());
    let crumbs = crumbs(app, &repo);
    let mut trail = vec![Span::raw("  ")];
    for (at, crumb) in crumbs.iter().enumerate() {
        let last = at + 1 == crumbs.len();
        if at > 0 {
            trail.push(span(" › ", th.dim));
        }
        trail.push(match last {
            true => strong(crumb.clone(), th),
            false => span(crumb.clone(), th.muted),
        });
    }
    let up = match crumbs.len() {
        0..=2 if !app.feature_open && app.capture.is_none() => Vec::new(),
        _ => vec![strong("esc", th), span(" up one level  ", th.dim)],
    };
    paint(
        f,
        &[Line::from(spread(trail, up, stage.width))],
        Rect { height: 1, ..stage },
        0,
    );
    rule(f, stage, stage.y + 1, th);
    let below = Rect {
        y: stage.y + 2,
        height: stage.height - 2,
        ..stage
    };

    if app.capture.is_some() {
        return capture::render(f, app, below, &repo, th);
    }
    if !app.feature_open || app.selected_feature().is_none() {
        return index::render(f, app, below, &repo, th);
    }
    let body = render_feature_heading(f, app, below, th);
    if body.height == 0 {
        return;
    }
    match app.lens {
        Lens::Work => work::render(f, app, body, th),
        Lens::Record => record::render(f, app, body, th),
        Lens::Flows => flows::render(f, app, body, th),
    }
}

/// Where the stage is, outermost first: the repository, the feature, the
/// lens, and the row under the cursor with the row it is under.
fn crumbs(app: &App, repo: &str) -> Vec<String> {
    let mut crumbs = vec![repo.to_string()];
    if let Some(note) = &app.capture {
        if matches!(note.target, crate::brief::BriefTarget::Capture { into: None, .. }) {
            crumbs.extend(["features".to_string(), "new".to_string()]);
            return crumbs;
        }
    }
    let (true, Some(feature)) = (app.feature_open, app.current_feature_row()) else {
        crumbs.push("features".into());
        return crumbs;
    };
    crumbs.push(format!("#{} {}", feature.number, feature.title));
    crumbs.push(app.lens.label().to_string());
    if app.capture.is_some() {
        crumbs.push("note".into());
        return crumbs;
    }
    let outline = app.outline();
    match (app.lens, app.selected_work()) {
        (Lens::Flows, _) => {
            if let Some(diagram) = app.selected_diagram() {
                crumbs.push(diagram.title.clone());
                if let Some(step) = app.flow_steps().get(app.step_sel) {
                    crumbs.push(format!("step {}", step.number));
                }
            }
        }
        (_, Some(WorkRef::Task(id))) => {
            let Some(task) = outline.task(id) else {
                return crumbs;
            };
            match task.task.parent.and_then(|parent| outline.task(parent)) {
                Some(parent) => {
                    crumbs.push(format!("{} {}", parent.label(), parent.task.title));
                    crumbs.push(task.label());
                }
                None => crumbs.push(format!("{} {}", task.label(), task.task.title)),
            }
        }
        (_, Some(WorkRef::Decision(id))) => {
            if let Some(decision) = outline.decision(id) {
                crumbs.push(decision.label());
            }
        }
        (_, None) => {}
    }
    crumbs
}

fn standing_span(row: &FeatureRow, th: Theme) -> Span<'static> {
    let (mark, color) = match row.standing {
        Standing::New | Standing::Queued => ("○", th.muted),
        Standing::Active => ("◐", th.warn),
        Standing::Done => ("✓", th.ok),
        Standing::Held => ("⊘", th.warn),
        Standing::Archived => ("✓", th.dim),
    };
    span(format!("{mark} {}", row.standing.label()), color)
}

/// The open feature's heading: what it is, how it stands, how far along,
/// and the lenses it can be read through. Returns what is left for a lens.
fn render_feature_heading(f: &mut Frame, app: &mut App, area: Rect, th: Theme) -> Rect {
    let Some(feature) = app.current_feature_row() else {
        return area;
    };
    let text = inset(area, 2, 0);
    let mut lines = Vec::new();
    let mut facts = vec![standing_span(&feature, th)];
    if let Some(branch) = &feature.branch {
        facts.extend([span(" · ", th.dim), span(branch.clone(), th.accent)]);
    }
    if let Some(pane) = feature.pane {
        let color = if feature.attention.is_some() { th.warn } else { th.muted };
        facts.extend([span(" · ", th.dim), span(pane_label(pane), color)]);
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(spread(
        vec![
            span(format!("#{} ", feature.number), th.accent),
            strong(feature.title.clone(), th),
        ],
        facts,
        text.width,
    )));

    // On every lens: `t` opens one from any of them.
    let links = app.selected_feature().map(|f| f.links.clone()).unwrap_or_default();
    if !links.is_empty() {
        let mut row = vec![span("⇄ ", th.accent)];
        for (at, link) in links.iter().enumerate() {
            if at > 0 {
                row.push(span(" · ", th.dim));
            }
            row.extend([span(format!("{} ", link.tracker), th.muted), span(link.key.clone(), th.accent)]);
        }
        lines.push(Line::from(spread(row, vec![span("t open", th.dim)], text.width)));
    }

    // The brief sits over the work, which is what it explains; the record
    // and the flows say their own why and keep the room.
    if app.lens == Lens::Work {
        let held = app.selected_feature().and_then(|f| f.held.clone());
        if let Some(reason) = held {
            lines.push(Line::from(span(format!("held: {reason}"), th.warn)));
        }
        let brief = app.selected_feature().map(|f| f.body.trim().to_string());
        match brief.filter(|brief| !brief.is_empty()) {
            Some(brief) => {
                let wrapped: Vec<String> = brief
                    .lines()
                    .flat_map(|line| wrap(line, text.width.min(100)))
                    .collect();
                let more = wrapped.len() > 3;
                for (at, line) in wrapped.into_iter().take(3).enumerate() {
                    let tail = if more && at == 2 { " …" } else { "" };
                    lines.push(Line::from(span(format!("{line}{tail}"), th.muted)));
                }
            }
            None => lines.push(Line::from(span("no description yet", th.dim))),
        }
    }

    let tasks = feature.tasks;
    let mut standing = Vec::new();
    if tasks.total() > 0 {
        standing.extend(progress(tasks.done, tasks.total(), 20, th));
        standing.push(span(
            format!("  {} of {}", tasks.done, plural(tasks.total(), "task")),
            th.muted,
        ));
    } else {
        standing.push(span("no tasks", th.dim));
    }
    match (feature.decided, feature.open) {
        (0, 0) => standing.extend([span(" · ", th.dim), span("no decisions", th.dim)]),
        (decided, open) => {
            if decided > 0 {
                standing.extend([span(" · ", th.dim), span(format!("{decided} decided"), th.ok)]);
            }
            if open > 0 {
                standing.extend([span(" · ", th.dim), span(format!("{open} open"), th.warn)]);
            }
        }
    }
    let strip = lens_strip(app.lens, th);
    let strip_width = width_of(&strip) as u16;
    lines.push(Line::from(spread(standing, strip, text.width)));

    let height = (lines.len() as u16 + 1).min(area.height);
    paint(
        f,
        &lines,
        Rect {
            height: height.saturating_sub(1),
            ..text
        },
        0,
    );
    let strip_row = Rect {
        x: text.right().saturating_sub(strip_width),
        y: text.y + lines.len() as u16 - 1,
        width: strip_width.min(text.width),
        height: 1,
    };
    if strip_row.y + 1 < area.bottom() {
        app.layout.feature_lenses = Panel {
            outer: strip_row,
            inner: strip_row,
            first: 0,
        };
    }
    rule(f, area, area.y + height - 1, th);
    Rect {
        y: area.y + height,
        height: area.height - height,
        ..area
    }
}

/// Each lens as its digit and its name, the open one lit.
fn lens_cells(open: Lens) -> impl Iterator<Item = (Lens, String, bool)> {
    Lens::ALL
        .into_iter()
        .map(move |lens| (lens, format!(" {} {} ", lens.digit(), lens.label()), lens == open))
}

fn lens_strip(open: Lens, th: Theme) -> Vec<Span<'static>> {
    lens_cells(open)
        .map(|(_, cell, lit)| match lit {
            true => Span::styled(
                cell,
                Style::default()
                    .fg(th.text)
                    .bg(th.surface)
                    .add_modifier(Modifier::BOLD),
            ),
            false => span(cell, th.muted),
        })
        .collect()
}

/// What a click on the stage landed on, read against the same panels the
/// last frame drew.
pub(crate) fn feature_hit_at(app: &App, x: u16, y: u16) -> Option<FeatureHit> {
    let layout = &app.layout;
    let line_in = |panel: &Panel| {
        contains(panel.inner, x, y).then(|| panel.first + usize::from(y - panel.inner.y))
    };
    if contains(layout.feature_lenses.inner, x, y) {
        let mut left = layout.feature_lenses.inner.x;
        for (lens, cell, _) in lens_cells(app.lens) {
            let right = left + cell.chars().count() as u16;
            if x < right {
                return Some(FeatureHit::Lens(lens));
            }
            left = right;
        }
    }
    if let Some(line) = line_in(&layout.features) {
        return index::feature_at_line(app, line).map(FeatureHit::Feature);
    }
    if let Some(line) = line_in(&layout.feature_flows) {
        return (line < app.feature_diagrams().len()).then_some(FeatureHit::Flow(line));
    }
    let line = line_in(&layout.feature_list)?;
    let row = match app.lens {
        Lens::Work => (line < app.outline().rows.len()).then_some(line),
        Lens::Record => (line < app.board_rows().len()).then_some(line),
        Lens::Flows => flows::step_at_line(app, line),
    };
    row.map(FeatureHit::Row)
}

/// The detail of a task: what it is, what done means, what binds it, and
/// what has been said on it.
fn task_detail(
    app: &App,
    outline: &Outline<'_>,
    at: &TaskTreeRow<'_>,
    width: u16,
    th: Theme,
) -> (Vec<Line<'static>>, Option<usize>) {
    let task = at.task;
    let held = outline
        .rows
        .iter()
        .find(|listed| listed.at == WorkRef::Task(task.id))
        .and_then(|listed| listed.waits_on.clone());
    let under = task
        .parent
        .and_then(|parent| outline.task(parent))
        .map(|parent| format!(" · under {} {}", parent.label(), parent.task.title))
        .unwrap_or_default();
    let mut lines = vec![heading("TASK", !app.in_detail, &format!("{}{under}", at.label()), width, th)];
    lines.extend(
        wrap(&task.title, width.saturating_sub(3))
            .into_iter()
            .map(|line| Line::from(vec![Span::raw("  "), strong(line, th)])),
    );
    let mut facts = vec![Span::raw("  "), task_mark(task, held.is_some(), th), Span::raw(" ")];
    facts.push(match (&held, task.state) {
        (_, TaskState::Done) => span("done", th.ok),
        (Some(decision), _) => span(format!("waits on {decision}"), th.err),
        (None, TaskState::Doing) => span("in progress", th.warn),
        (None, TaskState::Todo) => span("to do", th.muted),
    });
    if let Some(pane) = task.claimed_pane {
        facts.extend([span(" · ", th.dim), span(pane_label(pane), th.warn)]);
    }
    if let Some(key) = &task.external {
        facts.extend([span(" · ", th.dim), span(key.clone(), th.muted)]);
    }
    lines.push(Line::from(facts));

    lines.push(Line::raw(""));
    lines.push(heading("DESCRIPTION", false, "", width, th));
    match task.body.as_deref().filter(|body| !body.trim().is_empty()) {
        Some(body) => lines.extend(prose(body, width, th.muted)),
        None => lines.push(Line::from(span("  none yet — enter writes one", th.dim))),
    }

    lines.push(Line::raw(""));
    lines.push(heading("DONE WHEN", app.in_detail, "", width, th));
    let mut cursor_line = None;
    for (index, criterion) in task.criteria.iter().enumerate() {
        let selected = app.in_detail && index == app.detail_sel;
        if selected {
            cursor_line = Some(lines.len());
        }
        let (mark, color) = match criterion.done {
            true => (span("✓ ", th.ok), th.muted),
            false => (span("○ ", th.dim), th.text),
        };
        // A criterion is read whole or not at all: it wraps under itself.
        let wrapped = wrap(&criterion.text, width.saturating_sub(6));
        for (line, text) in wrapped.into_iter().enumerate() {
            let lead = match line {
                0 => mark.clone(),
                _ => Span::raw("  "),
            };
            lines.push(row(vec![lead, span(text, color)], selected, width, th));
        }
    }
    if task.criteria.is_empty() {
        lines.push(Line::from(span("  nothing listed — c adds a criterion", th.dim)));
    }

    let bound = outline.bound_by(task.id);
    if !bound.is_empty() {
        lines.push(Line::raw(""));
        lines.push(heading("BOUND BY", false, "", width, th));
        lines.extend(bound.iter().map(|decision| decision_ref(decision, width, th)));
    }

    if !task.notes.is_empty() {
        lines.push(Line::raw(""));
        lines.push(heading("NOTES", false, "", width, th));
        for note in &task.notes {
            let who = match (note.pane, &note.session) {
                (Some(pane), _) => pane_label(pane),
                (None, Some(_)) => "agent".to_string(),
                (None, None) => "you".to_string(),
            };
            let hang = who.chars().count() + 4;
            let wrapped = wrap(&note.text, width.saturating_sub(hang as u16 + 1));
            for (line, text) in wrapped.into_iter().enumerate() {
                let lead = match line {
                    0 => span(format!("  {who}  "), th.dim),
                    _ => Span::raw(" ".repeat(hang)),
                };
                lines.push(Line::from(vec![lead, span(text, th.muted)]));
            }
        }
    }
    (lines, cursor_line)
}

/// A decision named in passing: its mark, what a reader calls it, and its
/// name.
fn decision_ref(decision: &DecisionTreeRow<'_>, width: u16, th: Theme) -> Line<'static> {
    Line::from(ellipsize_spans(
        vec![
            Span::raw("  "),
            decision_mark(decision.decision, th),
            span(format!(" {} ", decision.label()), th.accent),
            decision_title(decision.decision, th.muted),
        ],
        usize::from(width),
    ))
}

/// A task named in passing.
fn task_ref(task: &TaskTreeRow<'_>, width: u16, th: Theme) -> Line<'static> {
    let color = match task.task.state {
        TaskState::Done => th.muted,
        _ => th.text,
    };
    Line::from(ellipsize_spans(
        vec![
            Span::raw("  "),
            task_mark(task.task, false, th),
            span(format!(" {} ", task.label()), th.dim),
            span(task.task.title.clone(), color),
        ],
        usize::from(width),
    ))
}
