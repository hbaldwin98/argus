//! The record lens: every decision of the feature as a tree in the order
//! it was made, and the one under the cursor read as an ADR.

use super::*;
use argus_protocol::Consequence;

/// What the detail column offers on a decision, wherever one is read.
pub(super) const KEYS: &[(&str, &str)] = &[
    ("e", "edit"),
    ("s", "sub-decision"),
    ("u", "supersede"),
    ("y", "copy as ADR markdown"),
];

/// The rails and elbow that lead to a row this deep in the tree.
fn guide(row: &DecisionTreeRow<'_>) -> String {
    if row.depth == 0 {
        return String::new();
    }
    let mut guide: String = row
        .ancestor_continuations
        .iter()
        .take(row.depth - 1)
        .map(|continues| if *continues { "│ " } else { "  " })
        .collect();
    guide.push_str(if row.has_next_sibling { "├ " } else { "└ " });
    guide
}

fn decision_line(decision: &DecisionTreeRow<'_>, selected: bool, width: u16, th: Theme) -> Line<'static> {
    let aside = set_aside(decision.decision);
    let label_color = match (aside, decision.decision.open()) {
        (true, _) => th.dim,
        (false, true) => th.warn,
        (false, false) => th.accent,
    };
    let title = match (selected, aside) {
        (true, false) => {
            let mut title = decision_title(decision.decision, th.text);
            title.style = title.style.add_modifier(Modifier::BOLD);
            title
        }
        (_, true) => decision_title(decision.decision, th.dim),
        (false, false) if decision.depth == 0 => decision_title(decision.decision, th.text),
        (false, false) => decision_title(decision.decision, th.muted),
    };
    let left = vec![
        span(guide(decision), th.dim),
        decision_mark(decision.decision, th),
        span(format!(" {:<5}", decision.label()), label_color),
        title,
    ];
    let status = vec![decision_status(decision.decision, th), Span::raw(" ")];
    row(spread(left, status, width.saturating_sub(3)), selected, width, th)
}

pub(super) fn render(f: &mut Frame, app: &mut App, body: Rect, th: Theme) {
    let outline = app.outline();
    let (list, detail) = columns(app, body, 50);
    divider(f, list, detail, th);
    let list = inset(list, 0, 1);
    let detail = inset(detail, 0, 1);

    let mut rows: Vec<Line<'static>> = outline
        .decisions
        .iter()
        .enumerate()
        .map(|(index, decision)| decision_line(decision, index == app.decision_sel, list.width, th))
        .collect();
    if rows.is_empty() {
        rows.push(Line::from(span(
            "  Nothing decided yet. d records a decision, made already or still open.",
            th.muted,
        )));
    } else {
        rows.push(Line::raw(""));
        rows.extend(prose(
            "Superseded and rejected decisions stay listed, struck through.",
            list.width,
            th.dim,
        ));
    }
    let head = heading(
        "DECISIONS",
        !app.in_detail,
        "in the order they were made ",
        list.width,
        th,
    );
    let (lines, cursor) = outline
        .decisions
        .get(app.decision_sel)
        .map(|decision| decision_detail(app, &outline, decision, detail.width, th))
        .unwrap_or_default();
    let selected = (!outline.decisions.is_empty()).then_some(app.decision_sel);
    drop(outline);
    let was = app.layout.feature_list.first;
    app.layout.feature_list = list_column(f, list, head, &rows, selected, was);
    app.layout.feature_detail = detail_column(f, detail, &lines, KEYS, cursor, th);
}

/// A decision read as a record: what made it necessary, what was decided
/// and why, what it costs and buys, what was turned down, and the tasks
/// that carry it out. An open one lists the answers it is choosing between
/// instead, with a cursor to take one.
pub(super) fn decision_detail(
    app: &App,
    outline: &Outline<'_>,
    row: &DecisionTreeRow<'_>,
    width: u16,
    th: Theme,
) -> (Vec<Line<'static>>, Option<usize>) {
    let decision = row.decision;
    let raised = outline
        .raised_under(decision.id)
        .map(|task| format!("raised under task {} · ", task.label()))
        .unwrap_or_default();
    let by = match (&decision.session, &decision.checkout) {
        (None, _) => "by you".to_string(),
        (Some(_), _) => "by an agent".to_string(),
    };
    let mut status = decision_status(decision, th);
    status.content = status.content.to_uppercase().into();
    let mut lines = vec![Line::from(spread(
        vec![span(format!("  {} ", row.label()), th.accent), status],
        vec![span(format!("{raised}{by} "), th.dim)],
        width,
    ))];
    lines.extend(
        wrap(&decision.chose, width.saturating_sub(3))
            .into_iter()
            .map(|line| Line::from(vec![Span::raw("  "), strong(line, th)])),
    );
    if let Some(successor) = decision
        .superseded_by
        .and_then(|id| outline.decision(id))
    {
        lines.push(Line::from(span(format!("  superseded by {}", successor.label()), th.dim)));
    }

    let section = |lines: &mut Vec<Line<'static>>, name: &str, lit: bool, body: Vec<Line<'static>>| {
        if body.is_empty() {
            return;
        }
        lines.push(Line::raw(""));
        lines.push(heading(name, lit, "", width, th));
        lines.extend(body);
    };
    let text = |field: &Option<String>, color: Color| {
        field
            .as_deref()
            .map(|text| prose(text, width, color))
            .unwrap_or_default()
    };

    section(&mut lines, "CONTEXT", false, text(&decision.context, th.muted));
    let mut cursor_line = None;
    if decision.open() {
        let options: Vec<Line<'static>> = decision
            .options
            .iter()
            .enumerate()
            .map(|(at, option)| {
                let selected = app.in_detail && at == app.detail_sel;
                super::row(
                    vec![span(format!("{} ", at + 1), th.warn), span(option.clone(), th.text)],
                    selected,
                    width,
                    th,
                )
            })
            .collect();
        let hint = match options.is_empty() {
            true => "  no options listed — o answers it in your own words",
            false => "  tab then enter takes one · o answers in your own words",
        };
        let at = app.detail_sel;
        section(&mut lines, "OPTIONS", app.in_detail, options);
        if app.in_detail && !decision.options.is_empty() {
            // The heading and the blank above it are the two lines before
            // the first option.
            cursor_line = Some(lines.len() - decision.options.len() + at);
        }
        lines.push(Line::from(span(hint, th.dim)));
    } else {
        section(&mut lines, "DECISION", false, text(&decision.detail, th.text));
        section(&mut lines, "WHY", false, text(&decision.because, th.muted));
        let consequences = decision
            .consequences
            .iter()
            .map(|Consequence { gain, text }| {
                let sign = match gain {
                    true => span("  + ", th.ok),
                    false => span("  − ", th.err),
                };
                Line::from(vec![sign, span(text.clone(), th.muted)])
            })
            .collect();
        section(&mut lines, "CONSEQUENCES", false, consequences);
    }

    let mut considered = text(&decision.over, th.dim);
    for child in outline.decisions.iter().filter(|child| {
        child.decision.parent == Some(decision.id)
            && child.decision.state == DecisionState::Rejected
    }) {
        considered.push(decision_ref(child, width, th));
        if let Some(why) = &child.decision.because {
            considered.extend(prose(&format!("  rejected: {why}"), width, th.dim));
        }
    }
    section(&mut lines, "CONSIDERED", false, considered);

    let carried = outline
        .carried_out_by(decision.id)
        .into_iter()
        .map(|task| task_ref(task, width, th))
        .collect();
    section(&mut lines, "CARRIED OUT BY", false, carried);
    (lines, cursor_line)
}
