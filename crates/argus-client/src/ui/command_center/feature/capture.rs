//! A feature being written as one note: the note on the left as it is
//! typed, and on the right what it reads as, which is what saving creates.

use super::*;
use argus_protocol::{Capture, CapturedDecision, CapturedTask};

/// The grammar, as the foot of the note says it.
const LEGEND: [(&str, &str); 5] = [
    ("first line", "title"),
    ("paragraph", "description"),
    ("-", "task · indent it for a subtask"),
    ("?", "open decision · indented lines are its options"),
    ("!", "decision already made · indented line is the why"),
];

pub(super) fn render(f: &mut Frame, app: &mut App, area: Rect, repo: &str, th: Theme) {
    let Some(capture) = app.captured() else {
        return;
    };
    let new = app.capture_is_new();
    let name = match (new, app.current_feature_row()) {
        (false, Some(feature)) => format!("More of #{} {}", feature.number, feature.title),
        _ => format!("New feature in {repo}"),
    };
    let says = match new {
        true => "write it as one note — it becomes the feature on save ",
        false => "tasks and decisions from one note — the first line is not a title here ",
    };
    let text = inset(area, 2, 0);
    paint(
        f,
        &[
            Line::raw(""),
            Line::from(spread(vec![strong(name, th)], vec![span(says, th.dim)], text.width)),
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
    let (note_area, reads) = match body.width < TWO_COLUMNS_FROM {
        true => (body, Rect { width: 0, ..body }),
        false => {
            let split = body.width * 55 / 100;
            (
                Rect { width: split, ..body },
                Rect {
                    x: body.x + split + 1,
                    width: body.width - split - 1,
                    ..body
                },
            )
        }
    };
    divider(f, note_area, reads, th);
    let note_area = inset(note_area, 0, 1);
    let reads = inset(reads, 0, 1);

    let legend_height = (LEGEND.len() as u16 + 1).min(note_area.height / 2);
    let editor = Rect {
        y: note_area.y + 2.min(note_area.height),
        height: note_area.height.saturating_sub(2 + legend_height),
        ..note_area
    };
    paint(
        f,
        &[heading("NOTE", true, "", note_area.width, th)],
        Rect { height: 1, ..note_area },
        0,
    );
    let Some(note) = app.capture.as_mut() else {
        return;
    };
    note.follow_cursor(usize::from(editor.height));
    let lines: Vec<Line<'static>> = note
        .lines
        .iter()
        .enumerate()
        .map(|(at, line)| note_line(at, line, (at == note.line).then_some(note.column), &capture, editor.width, th))
        .collect();
    paint(f, &lines, editor, note.scroll);

    let mut legend = vec![Line::from(span("─".repeat(usize::from(note_area.width)), th.edge))];
    legend.extend(LEGEND.iter().map(|(mark, reads_as)| {
        let color = match *mark {
            "-" => th.accent,
            "?" => th.warn,
            "!" => th.ok,
            _ => th.dim,
        };
        Line::from(vec![
            span(format!("  {mark:<12}"), color),
            span(*reads_as, th.muted),
        ])
    }));
    paint(
        f,
        &legend,
        Rect {
            y: editor.bottom(),
            height: legend_height,
            ..note_area
        },
        0,
    );

    if reads.width > 0 {
        paint(f, &reads_as(&capture, new, reads.width, th), reads, 0);
    }
}

/// One line of the note: its number, its text coloured by what its mark
/// makes it, and the cursor where the next character goes.
fn note_line(
    at: usize,
    line: &str,
    cursor: Option<usize>,
    capture: &Capture,
    width: u16,
    th: Theme,
) -> Line<'static> {
    let trimmed = line.trim_start();
    let color = match trimmed.chars().next() {
        _ if capture.unread.contains(&(at + 1)) => th.err,
        Some('-') => th.text,
        Some('?') => th.warn,
        Some('!') => th.ok,
        _ => th.text,
    };
    let mut spans = vec![span(format!("{:>4}  ", at + 1), th.dim)];
    let Some(column) = cursor else {
        spans.push(span(line.to_string(), color));
        return Line::from(spans);
    };
    let before: String = line.chars().take(column).collect();
    let under = line.chars().nth(column).unwrap_or(' ');
    let after: String = line.chars().skip(column + 1).collect();
    spans.extend([
        span(before, color),
        Span::styled(under.to_string(), Style::default().fg(th.bg).bg(th.accent)),
        span(after, color),
    ]);
    let pad = usize::from(width).saturating_sub(width_of(&spans));
    spans.push(Span::raw(" ".repeat(pad)));
    Line::from(spans).style(Style::default().bg(th.surface))
}

/// What the note reads as, drawn the way the feature will be.
fn reads_as(capture: &Capture, new: bool, width: u16, th: Theme) -> Vec<Line<'static>> {
    let verdict = match capture.clone().checked(new) {
        Ok(_) => span("ready to create ", th.ok),
        Err(why) => span(format!("{why} "), th.warn),
    };
    let mut lines = vec![
        Line::from(spread(vec![span("  READS AS", th.muted)], vec![verdict], width)),
        Line::raw(""),
    ];
    if new {
        match capture.title.is_empty() {
            true => lines.push(Line::from(span("  the first line is the title", th.dim))),
            false => lines.push(Line::from(vec![Span::raw("  "), strong(capture.title.clone(), th)])),
        }
        if let Some(body) = &capture.body {
            lines.extend(prose(body, width, th.muted));
        }
        lines.push(Line::raw(""));
    }

    lines.push(heading("TASKS", false, &format!("0/{} ", capture.task_count()), width, th));
    for (at, task) in capture.tasks.iter().enumerate() {
        task_lines(task, &[at + 1], &mut lines, th);
    }
    let decisions = capture.all_decisions();
    if decisions.is_empty() {
        return lines;
    }
    let open = decisions.iter().filter(|decision| decision.open).count();
    let about = match open {
        0 => format!("{} decided ", decisions.len()),
        open => format!("{open} open "),
    };
    lines.push(Line::raw(""));
    lines.push(heading("DECISIONS", false, &about, width, th));
    for (at, decision) in decisions.into_iter().enumerate() {
        decision_lines(decision, at + 1, &mut lines, th);
    }
    lines
}

fn task_lines(task: &CapturedTask, number: &[usize], lines: &mut Vec<Line<'static>>, th: Theme) {
    let depth = number.len() - 1;
    lines.push(Line::from(vec![
        Span::raw("  ".repeat(depth + 1)),
        span("○ ", th.dim),
        span(format!("{:<5}", argus_protocol::outline_number(number)), th.dim),
        span(task.title.clone(), if depth == 0 { th.text } else { th.muted }),
    ]));
    for (at, subtask) in task.subtasks.iter().enumerate() {
        let mut number = number.to_vec();
        number.push(at + 1);
        task_lines(subtask, &number, lines, th);
    }
}

fn decision_lines(decision: &CapturedDecision, number: usize, lines: &mut Vec<Line<'static>>, th: Theme) {
    let (mark, color) = match decision.open {
        true => ("?", th.warn),
        false => ("✓", th.ok),
    };
    lines.push(Line::from(vec![
        span(format!("  {mark} "), color),
        span(format!("{:<5}", format!("D{number}")), color),
        span(decision.chose.clone(), th.text),
    ]));
    for (at, option) in decision.options.iter().enumerate() {
        lines.push(Line::from(vec![
            span(format!("      {} ", at + 1), th.warn),
            span(option.clone(), th.muted),
        ]));
    }
    if let Some(because) = &decision.because {
        lines.push(Line::from(span(format!("      because {because}"), th.dim)));
    }
}
