//! The feature index: every feature of the repository grouped by how it
//! stands, with the one under the cursor peeked at beside the list — and
//! what the stage says when there are none.

use super::*;
use crate::app::GROUPS;

/// One line of the index's list. The list is drawn from these and a click
/// is resolved against them, so the two cannot disagree about which line a
/// feature is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexLine {
    Blank,
    Group(Standing),
    /// A place in `feature_rows`.
    Feature(usize),
    /// What the feature above comes after.
    After(usize),
    /// Whether the held and archived features are folded away.
    Folding,
}

fn index_lines(rows: &[FeatureRow]) -> Vec<IndexLine> {
    let mut lines = Vec::new();
    for group in GROUPS {
        let listed = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.standing.group() == group);
        for (at, (index, row)) in listed.enumerate() {
            if at == 0 {
                if !lines.is_empty() {
                    lines.push(IndexLine::Blank);
                }
                lines.push(IndexLine::Group(group));
            }
            lines.push(IndexLine::Feature(index));
            if !row.after.is_empty() {
                lines.push(IndexLine::After(index));
            }
        }
    }
    lines.extend([IndexLine::Blank, IndexLine::Folding]);
    lines
}

/// The feature drawn on a line of the list, if the line is a feature's.
pub(super) fn feature_at_line(app: &App, line: usize) -> Option<usize> {
    match index_lines(&app.feature_rows()).get(line)? {
        IndexLine::Feature(index) | IndexLine::After(index) => Some(*index),
        _ => None,
    }
}

/// What each column of the list is given, right to left; the title takes
/// what is left. A narrow list keeps the counts and drops where and who.
struct Columns {
    tasks: usize,
    decisions: usize,
    on: usize,
    pane: usize,
}

impl Columns {
    fn for_width(width: u16) -> Columns {
        match width {
            0..=47 => Columns { tasks: 6, decisions: 0, on: 0, pane: 0 },
            48..=87 => Columns { tasks: 15, decisions: 9, on: 0, pane: 0 },
            _ => Columns { tasks: 15, decisions: 9, on: 10, pane: 5 },
        }
    }

    fn cells(&self, cells: [Vec<Span<'static>>; 4]) -> Vec<Span<'static>> {
        let widths = [self.tasks, self.decisions, self.on, self.pane];
        let mut out = Vec::new();
        for (cell, width) in cells.into_iter().zip(widths) {
            if width == 0 {
                continue;
            }
            let cell = ellipsize_spans(cell, width);
            let pad = width.saturating_sub(width_of(&cell));
            out.push(Span::raw("  "));
            out.extend(cell);
            out.push(Span::raw(" ".repeat(pad)));
        }
        out
    }
}

fn feature_line(row: &FeatureRow, selected: bool, columns: &Columns, width: u16, th: Theme) -> Line<'static> {
    let quiet = matches!(row.standing, Standing::Done | Standing::Held | Standing::Archived);
    let title = match (selected, quiet) {
        (true, _) => strong(row.title.clone(), th),
        (false, true) => span(row.title.clone(), th.muted),
        (false, false) => span(row.title.clone(), th.text),
    };
    let tasks = match (row.tasks.total(), columns.tasks >= 15) {
        (0, _) => vec![span("—", th.dim)],
        (total, true) => {
            let mut cell = progress(row.tasks.done, total, 8, th);
            cell.push(span(format!(" {}/{total}", row.tasks.done), th.muted));
            cell
        }
        (total, false) => vec![span(format!("{}/{total}", row.tasks.done), th.muted)],
    };
    let decisions = match (row.open, row.decided) {
        (0, 0) => span("—", th.dim),
        (0, decided) => span(format!("{decided} decided"), th.ok),
        (open, _) => span(format!("{open} open"), th.warn),
    };
    let on = match &row.branch {
        Some(branch) => span(branch.clone(), if quiet { th.dim } else { th.accent }),
        None => span("—", th.dim),
    };
    let pane = match row.pane {
        Some(pane) => span(
            format!("#{}", pane.0),
            if row.attention.is_some() { th.warn } else { th.muted },
        ),
        None => span("—", th.dim),
    };
    let number = span(
        format!("{:<5}", format!("#{}", row.number)),
        if selected { th.accent } else { th.dim },
    );
    let cells = columns.cells([tasks, vec![decisions], vec![on], vec![pane]]);
    super::row(
        spread(vec![number, title], cells, width.saturating_sub(3)),
        selected,
        width,
        th,
    )
}

pub(super) fn render(f: &mut Frame, app: &mut App, area: Rect, repo: &str, th: Theme) {
    let rows = app.feature_rows();
    let count = |standing: Standing| {
        rows.iter()
            .filter(|row| row.standing.group() == standing)
            .count()
    };
    let mut tally = Vec::new();
    for (standing, word, color) in [
        (Standing::Active, "active", th.warn),
        (Standing::Queued, "queued", th.muted),
        (Standing::Done, "done", th.ok),
    ] {
        if count(standing) == 0 {
            continue;
        }
        if !tally.is_empty() {
            tally.push(span(" · ", th.dim));
        }
        tally.push(span(format!("{} {word}", count(standing)), color));
    }
    if tally.is_empty() {
        tally.push(span("none yet", th.dim));
    }
    // The one thing about the tracker map that waits on the person, so
    // the heading says so wherever the cursor is.
    let draft = app.board_follower.board().and_then(|board| board.tracker_map.draft.as_ref());
    if let Some(draft) = draft {
        let mut cue = vec![
            span(format!("tracker map drafted by {}", draft.by), th.warn),
            span(" M", th.accent),
            span(" review", th.dim),
            span("   ", th.dim),
        ];
        cue.append(&mut tally);
        tally = cue;
    }
    let text = inset(area, 2, 0);
    paint(
        f,
        &[
            Line::raw(""),
            Line::from(spread(
                vec![strong(format!("Features in {repo}"), th)],
                tally,
                text.width,
            )),
        ],
        Rect {
            height: 2.min(text.height),
            ..text
        },
        0,
    );
    rule(f, area, area.y + 2, th);
    let body = Rect {
        y: area.y + 3.min(area.height),
        height: area.height.saturating_sub(3),
        ..area
    };
    if body.height == 0 {
        return;
    }
    let folded_exist = app.board_follower.board().is_some_and(|board| {
        board.features.len() > rows.len()
    });
    if rows.is_empty() && !folded_exist {
        return render_empty(f, app, body, repo, th);
    }

    let (list, peek) = columns(app, body, 64);
    divider(f, list, peek, th);
    let list = inset(list, 0, 1);
    let columns = Columns::for_width(list.width);
    let head = Line::from(spread(
        vec![span("       FEATURE", th.dim)],
        columns.cells([
            vec![span("TASKS", th.dim)],
            vec![span("DECISIONS", th.dim)],
            vec![span("ON", th.dim)],
            vec![span("PANE", th.dim)],
        ]),
        list.width.saturating_sub(1),
    ));
    let lines = index_lines(&rows);
    let drawn: Vec<Line<'static>> = lines
        .iter()
        .map(|line| match *line {
            IndexLine::Blank => Line::raw(""),
            IndexLine::Group(group) => {
                let color = match group {
                    Standing::Active => th.warn,
                    Standing::Done => th.ok,
                    _ => th.muted,
                };
                Line::from(span(format!("  {}", group.label()), color))
            }
            IndexLine::Feature(index) => {
                feature_line(&rows[index], index == app.feature_sel, &columns, list.width, th)
            }
            IndexLine::After(index) => {
                let after: Vec<String> = rows[index].after.iter().map(|n| format!("#{n}")).collect();
                Line::from(vec![
                    span("       after ", th.dim),
                    span(after.join(", "), th.muted),
                ])
            }
            IndexLine::Folding => Line::from(match (app.show_folded_features, folded_exist) {
                (true, _) => vec![
                    span("       held and archived features are shown · ", th.dim),
                    strong("z", th),
                    span(" folds them away", th.dim),
                ],
                (false, true) => vec![
                    span("       held and archived features are folded · ", th.dim),
                    strong("z", th),
                    span(" shows them", th.dim),
                ],
                (false, false) => Vec::new(),
            }),
        })
        .collect();
    let cursor = lines
        .iter()
        .position(|line| *line == IndexLine::Feature(app.feature_sel));
    app.layout.features = list_column(f, list, head, &drawn, cursor, app.layout.features.first);

    let peek = inset(peek, 0, 1);
    let lines = peek_lines(app, peek.width, th);
    paint(f, &lines, peek, 0);
}

/// The feature under the cursor, at a glance: what it is for, what comes
/// next, what is waiting on a person, what is being done, and the last
/// thing decided.
fn peek_lines(app: &App, width: u16, th: Theme) -> Vec<Line<'static>> {
    let (Some(row), Some(feature)) = (app.current_feature_row(), app.selected_feature()) else {
        return Vec::new();
    };
    let mut lines = vec![heading("PEEK", false, &format!("#{}", row.number), width, th)];
    if let Some(reason) = &feature.held {
        lines.extend(prose(&format!("held: {reason}"), width, th.warn));
    }
    match feature.body.trim() {
        "" => lines.push(Line::from(span("  no description yet", th.dim))),
        body => lines.extend(prose(body, width, th.muted).into_iter().take(8)),
    }

    let outline = app.outline();
    if let Some(next) = app.next_task() {
        lines.push(Line::raw(""));
        lines.push(heading("NEXT", false, "", width, th));
        lines.push(task_ref(&next, width, th));
    }

    let open: Vec<&DecisionTreeRow<'_>> = outline
        .decisions
        .iter()
        .filter(|decision| decision.decision.open())
        .collect();
    let stopped: Vec<&argus_protocol::PaneInfo> = app
        .panes_on(feature)
        .into_iter()
        .filter(|pane| pane.loudest_state().status.needs_you())
        .collect();
    if !open.is_empty() || !stopped.is_empty() {
        lines.push(Line::raw(""));
        lines.push(heading("NEEDS YOU", false, "", width, th));
        for decision in open {
            lines.push(Line::from(ellipsize_spans(
                vec![
                    span("  ? ", th.warn),
                    span(format!("{} ", decision.label()), th.warn),
                    span(decision.decision.chose.clone(), th.text),
                ],
                usize::from(width),
            )));
        }
        for pane in stopped {
            let state = pane.loudest_state();
            let note = state.note.map(|note| format!(" {note}")).unwrap_or_default();
            lines.push(Line::from(ellipsize_spans(
                vec![
                    span("  ■ ", th.warn),
                    span(pane_label(pane.id), th.warn),
                    span(format!(" {}{note}", short_status(state.status).to_lowercase()), th.muted),
                ],
                usize::from(width),
            )));
        }
    }

    let doing: Vec<&TaskTreeRow<'_>> = outline
        .tasks
        .iter()
        .filter(|task| task.task.state == TaskState::Doing && !task.has_children)
        .collect();
    if !doing.is_empty() {
        lines.push(Line::raw(""));
        lines.push(heading("IN FLIGHT", false, "", width, th));
        for task in doing {
            lines.push(task_ref(task, width, th));
            if let Some(pane) = task.task.claimed_pane {
                lines.push(Line::from(span(format!("    {}", pane_label(pane)), th.dim)));
            }
        }
    }

    let last = outline
        .decisions
        .iter()
        .filter(|decision| decision.decision.stands())
        .max_by_key(|decision| (decision.decision.at, decision.decision.id));
    if let Some(decision) = last {
        lines.push(Line::raw(""));
        lines.push(heading("LAST DECIDED", false, "", width, th));
        lines.push(decision_ref(decision, width, th));
    }
    lines
}

/// A repository with no features: what one is, the one key that starts
/// one, and a pane it could be started from.
fn render_empty(f: &mut Frame, app: &mut App, area: Rect, repo: &str, th: Theme) {
    // The stage is the index's even with nothing listed, so a click on it
    // is a click on the Feature stage; there is no row for one to land on.
    app.layout.features = Panel {
        outer: area,
        ..Panel::default()
    };
    let text = inset(area, 4, 1);
    let mut lines = vec![
        Line::from(strong(format!("No features in {repo} yet."), th)),
        Line::raw(""),
    ];
    lines.extend(
        wrap(
            "A feature is one piece of work you want to see through: what it is, the tasks under it, and the decisions made along the way.",
            text.width.min(72),
        )
        .into_iter()
        .map(|line| Line::from(span(line, th.muted))),
    );
    lines.push(Line::raw(""));
    lines.push(row(
        spread(
            vec![span("a  ", th.accent), strong("Start a feature", th)],
            vec![span("write it as one note ", th.muted)],
            text.width.min(72).saturating_sub(2),
        ),
        true,
        text.width.min(72),
        th,
    ));
    lines.push(Line::raw(""));
    lines.push(Line::from(span("WHAT IT HOLDS", th.muted)));
    for (mark, color, name, says) in [
        ("○", th.dim, "tasks", "a task and its subtasks, checked off as they land"),
        ("◆", th.accent, "decisions", "what was decided and why, with sub-decisions under it"),
        ("?", th.warn, "open", "questions waiting on you; they hold the tasks beneath them"),
    ] {
        lines.push(Line::from(vec![
            span(format!("{mark} "), color),
            span(format!("{name:<11}"), th.text),
            span(says, th.muted),
        ]));
    }

    let agent = app
        .current_checkout()
        .and_then(|checkout| {
            let pane = checkout
                .panes
                .iter()
                .find(|pane| pane.kind == PaneKind::Agent)?;
            Some((checkout.name.clone(), pane.id, pane.title.clone()))
        });
    if let Some((branch, pane, title)) = agent {
        lines.push(Line::raw(""));
        lines.push(Line::from(span("OR HAND IT TO A PANE", th.muted)));
        lines.push(Line::from(vec![
            span("■ ", th.dim),
            span(format!("{repo} · {branch} #{} ", pane.0), th.text),
            span(title, th.muted),
        ]));
        lines.push(Line::from(span(
            "  an agent there can open one with argus-hook feature open",
            th.dim,
        )));
    }
    paint(f, &lines, text, 0);
}
