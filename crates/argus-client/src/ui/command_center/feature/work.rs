//! The work lens: the feature's tasks and the decisions they raised as one
//! outline, with whatever is under the cursor read whole beside it.

use super::*;

/// The outline's indent for a row this deep.
fn indent(depth: usize) -> Span<'static> {
    Span::raw("  ".repeat(depth))
}

fn task_line(
    app: &App,
    outline: &Outline<'_>,
    at: &argus_protocol::outline::WorkRow,
    selected: bool,
    width: u16,
    th: Theme,
) -> Line<'static> {
    let task = &outline.tasks[at.index];
    let id = task.task.id;
    let holds_rows = task.has_children || outline.raised_count(id) > 0;
    let fold = match (holds_rows, app.folded_tasks().contains(&id)) {
        (false, _) => "  ",
        (true, true) => "▸ ",
        (true, false) => "▾ ",
    };
    let done = task.task.state == TaskState::Done;
    let title = match (selected, done) {
        (true, _) => strong(task.task.title.clone(), th),
        (false, true) => span(task.task.title.clone(), th.muted),
        (false, false) => span(task.task.title.clone(), th.text),
    };
    let left = vec![
        indent(at.depth),
        span(fold, th.dim),
        task_mark(task.task, at.waits_on.is_some(), th),
        span(format!(" {:<4}", task.label()), th.dim),
        title,
    ];

    let mut right = Vec::new();
    if let Some(decision) = &at.waits_on {
        if !done {
            right.push(span(format!("waits on {decision} "), th.err));
        }
    }
    if let Some(pane) = task.task.claimed_pane.filter(|_| !done) {
        right.push(span(format!("{} ", pane_label(pane)), th.warn));
    }
    if app.folded_tasks().contains(&id) && outline.raised_count(id) > 0 {
        right.push(span(
            format!("{} ", plural(outline.raised_count(id), "decision")),
            th.accent,
        ));
    }
    let (finished, total) = outline.progress(id);
    if total > 0 {
        right.push(span(format!("{finished}/{total} "), th.dim));
    }
    row(spread(left, right, width.saturating_sub(3)), selected, width, th)
}

fn decision_line(
    outline: &Outline<'_>,
    at: &argus_protocol::outline::WorkRow,
    selected: bool,
    width: u16,
    th: Theme,
) -> Line<'static> {
    let decision = &outline.decisions[at.index];
    let open = decision.decision.open();
    let mark = match (set_aside(decision.decision), open) {
        (true, _) => decision_mark(decision.decision, th),
        (false, true) => span("?", th.warn),
        (false, false) => span("◆", th.accent),
    };
    let label_color = match (set_aside(decision.decision), open) {
        (true, _) => th.dim,
        (false, true) => th.warn,
        (false, false) => th.accent,
    };
    let title_color = match (selected, open) {
        (true, _) | (_, true) => th.text,
        _ => th.muted,
    };
    let left = vec![
        indent(at.depth),
        Span::raw("  "),
        mark,
        span(format!(" {:<5}", decision.label()), label_color),
        decision_title(decision.decision, title_color),
    ];
    let right = match (open, set_aside(decision.decision)) {
        (true, _) => vec![span("NEEDS YOU ", th.warn)],
        (_, true) => vec![decision_status(decision.decision, th), Span::raw(" ")],
        _ => Vec::new(),
    };
    row(spread(left, right, width.saturating_sub(3)), selected, width, th)
}

pub(super) fn render(f: &mut Frame, app: &mut App, body: Rect, th: Theme) {
    let outline = app.outline();
    if outline.rows.is_empty() {
        drop(outline);
        return render_empty(f, body, th);
    }
    let (list, detail) = columns(app, body, 58);
    divider(f, list, detail, th);
    let list = inset(list, 0, 1);
    let detail = inset(detail, 0, 1);

    let rows: Vec<Line<'static>> = outline
        .rows
        .iter()
        .enumerate()
        .map(|(index, at)| {
            let selected = index == app.work_sel;
            match at.at {
                WorkRef::Task(_) => task_line(app, &outline, at, selected, list.width, th),
                WorkRef::Decision(_) => decision_line(&outline, at, selected, list.width, th),
            }
        })
        .collect();
    let head = heading(
        "OUTLINE",
        !app.in_detail,
        "tasks and the decisions they raised ",
        list.width,
        th,
    );
    let (lines, cursor, keys): (_, _, &[(&str, &str)]) = match app.selected_work() {
        Some(WorkRef::Task(id)) => match outline.task(id) {
            Some(task) => {
                let (lines, cursor) = task_detail(app, &outline, task, detail.width, th);
                (
                    lines,
                    cursor,
                    &[
                        ("e", "edit"),
                        ("c", "criterion"),
                        ("n", "note"),
                        ("d", "raise a decision"),
                        ("g", "go to pane"),
                    ],
                )
            }
            None => (Vec::new(), None, &[]),
        },
        Some(WorkRef::Decision(id)) => match outline.decision(id) {
            Some(decision) => {
                let (lines, cursor) = record::decision_detail(app, &outline, decision, detail.width, th);
                (lines, cursor, record::KEYS)
            }
            None => (Vec::new(), None, &[]),
        },
        None => (Vec::new(), None, &[]),
    };
    drop(outline);
    let was = app.layout.feature_list.first;
    app.layout.feature_list = list_column(f, list, head, &rows, Some(app.work_sel), was);
    app.layout.feature_detail = detail_column(f, detail, &lines, keys, cursor, th);
}

/// A feature with nothing in it: the keys that put the first thing there.
fn render_empty(f: &mut Frame, body: Rect, th: Theme) {
    let text = inset(body, 0, 1);
    let width = text.width.min(72);
    let offer = |key: &str, does: &str, about: &str, first: bool| {
        let name = match first {
            true => strong(does.to_string(), th),
            false => span(does.to_string(), th.text),
        };
        row(
            spread(
                vec![span(format!("{key}  "), if first { th.accent } else { th.text }), name],
                vec![span(format!("{about} "), th.dim)],
                width.saturating_sub(3),
            ),
            first,
            width,
            th,
        )
    };
    let lines = vec![
        heading("OUTLINE", true, "", text.width, th),
        Line::raw(""),
        Line::from(span(
            "  Nothing here yet. The first line you add becomes task 1.",
            th.muted,
        )),
        Line::raw(""),
        offer("a", "Add the first task", "", true),
        offer("d", "Record a decision", "made already, or still open", false),
        offer("e", "Write the description", "", false),
        offer("i", "Write it all as one note", "tasks and decisions from one buffer", false),
        Line::raw(""),
        Line::from(span(
            "  The detail column appears once there is something to select.",
            th.dim,
        )),
    ];
    paint(f, &lines, text, 0);
}
