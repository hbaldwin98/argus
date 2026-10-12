//! The flows lens: the feature's sequence diagrams, the selected one's
//! steps as rows, and the diagram itself drawn beside them.

use super::*;
use crate::flow::{FlowRow, Step};

/// How wide the column naming who a step runs between is drawn.
const ROUTE_WIDTH: usize = 24;

/// The step a line of the steps list draws, if the line is a step's.
pub(super) fn step_at_line(app: &App, line: usize) -> Option<usize> {
    let rows = app.flow_rows();
    let steps_before = rows
        .iter()
        .take(line)
        .filter(|row| matches!(row, FlowRow::Step(_)))
        .count();
    matches!(rows.get(line)?, FlowRow::Step(_)).then_some(steps_before)
}

fn step_line(step: &Step, selected: bool, width: u16, th: Theme) -> Line<'static> {
    let (number, route, text) = match selected {
        true => (span(format!("{:<3}", step.number), th.accent), strong(format!("{:<ROUTE_WIDTH$}", step.route()), th), strong(step.text.clone(), th)),
        false => (
            span(format!("{:<3}", step.number), th.dim),
            span(format!("{:<ROUTE_WIDTH$}", step.route()), th.muted),
            span(step.text.clone(), th.text),
        ),
    };
    row(
        ellipsize_spans(vec![number, route, Span::raw(" "), text], usize::from(width.saturating_sub(3))),
        selected,
        width,
        th,
    )
}

pub(super) fn render(f: &mut Frame, app: &mut App, body: Rect, th: Theme) {
    let diagrams = app.feature_diagrams();
    if diagrams.is_empty() {
        let lines = vec![
            heading("FLOWS", true, "", body.width, th),
            Line::raw(""),
            Line::from(span(
                "  No flows yet. a adds a sequence diagram; an agent records one with argus-hook diagram.",
                th.muted,
            )),
        ];
        return paint(f, &lines, inset(body, 0, 1), 0);
    }
    let (list, picture) = columns(app, body, 40);
    divider(f, list, picture, th);
    let list = inset(list, 0, 1);
    let picture = inset(picture, 2, 1);

    let flow_lines: Vec<Line<'static>> = diagrams
        .iter()
        .enumerate()
        .map(|(index, diagram)| {
            let selected = index == app.diagram_sel;
            let title = match selected {
                true => strong(diagram.title.clone(), th),
                false => span(diagram.title.clone(), th.muted),
            };
            let summary = crate::flow::summary(&crate::flow::rows(&diagram.body));
            row(
                spread(vec![title], vec![span(format!("{summary} "), th.dim)], list.width.saturating_sub(3)),
                selected,
                list.width,
                th,
            )
        })
        .collect();
    let flows_height = (flow_lines.len() as u16).min(4);
    let flows_area = Rect {
        height: (flows_height + 2).min(list.height),
        ..list
    };
    let steps_area = Rect {
        y: list.y + (flows_height + 3).min(list.height),
        height: list.height.saturating_sub(flows_height + 3),
        ..list
    };
    let flows_head = heading("FLOWS", false, &format!("{} ", diagrams.len()), list.width, th);

    let rows = app.flow_rows();
    let mut step_index = 0;
    let mut cursor = None;
    let step_lines: Vec<Line<'static>> = rows
        .iter()
        .enumerate()
        .map(|(line, row)| match row {
            FlowRow::Step(step) => {
                let selected = step_index == app.step_sel;
                if selected {
                    cursor = Some(line);
                }
                step_index += 1;
                step_line(step, selected, list.width, th)
            }
            FlowRow::Branch { kind, when } => Line::from(vec![
                span(format!("  {kind} "), th.warn),
                span(when.clone(), th.muted),
            ]),
        })
        .collect();
    let steps_head = heading(
        "STEPS",
        true,
        "the diagram is drawn from these ",
        list.width,
        th,
    );

    let Some(diagram) = app.selected_diagram().cloned() else {
        return;
    };
    let was_flows = app.layout.feature_flows.first;
    let was_steps = app.layout.feature_list.first;
    app.layout.feature_flows = list_column(f, flows_area, flows_head, &flow_lines, Some(app.diagram_sel), was_flows);
    app.layout.feature_list = list_column(f, steps_area, steps_head, &step_lines, cursor, was_steps);

    if picture.width == 0 || picture.height < 3 {
        return;
    }
    let width = usize::from(picture.width);
    let current = app
        .flow_drawn
        .as_ref()
        .is_some_and(|(id, drawn)| *id == diagram.id && drawn.shows(&diagram));
    if !current {
        app.flow_drawn = Some((diagram.id, crate::diagram::DiagramView::open(&diagram, width)));
    }
    let Some((_, drawn)) = app.flow_drawn.as_mut() else {
        return;
    };
    drawn.resize(width);
    let mut lines = vec![
        Line::from(span(diagram.title.to_uppercase(), th.muted)),
        Line::raw(""),
    ];
    lines.extend(
        drawn
            .lines
            .iter()
            .map(|line| Line::from(span(drawn.line_window(line, width), th.dim))),
    );
    paint(
        f,
        &lines,
        Rect {
            height: picture.height - 1,
            ..picture
        },
        0,
    );
    paint(
        f,
        &[Line::from(vec![
            strong("enter", th),
            span(" opens it whole   ", th.muted),
            strong("h/l", th),
            span(" other flows", th.muted),
        ])],
        Rect {
            y: picture.bottom() - 1,
            height: 1,
            ..picture
        },
        0,
    );
}
