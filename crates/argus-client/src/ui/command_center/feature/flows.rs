//! The flows lens: the feature's sequence diagrams, the selected one's
//! steps as rows, and the diagram itself drawn beside them.

use super::*;
use argus_protocol::flow::{FlowRow, Step};

/// The widest the column naming who a step runs between is drawn; a flow
/// whose routes are all shorter draws it as wide as its longest.
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

/// What a step is bound by, as the row's right edge names it: each
/// decision by what a reader calls it, then each task its own comment
/// names. An id the board no longer has is named by its id.
fn step_tags(step: &Step, outline: &Outline<'_>, th: Theme) -> Vec<Span<'static>> {
    let decisions = step.decisions.iter().map(|id| match outline.decision(*id) {
        Some(row) => span(row.label(), th.accent),
        None => span(format!("#{id}"), th.dim),
    });
    let tasks = step.tasks.iter().map(|id| match outline.task(*id) {
        Some(row) => span(format!("task {}", row.label()), th.dim),
        None => span(format!("task #{id}"), th.dim),
    });
    let mut tags = Vec::new();
    for tag in decisions.chain(tasks) {
        tags.push(tag);
        tags.push(Span::raw(" "));
    }
    tags
}

fn step_line(step: &Step, selected: bool, route_width: usize, outline: &Outline<'_>, width: u16, th: Theme) -> Line<'static> {
    let (number, route, text) = match selected {
        true => (span(format!("{:<3}", step.number), th.accent), strong(format!("{:<route_width$}", step.route()), th), strong(step.text.clone(), th)),
        false => (
            span(format!("{:<3}", step.number), th.dim),
            span(format!("{:<route_width$}", step.route()), th.muted),
            span(step.text.clone(), th.text),
        ),
    };
    row(
        spread(vec![number, route, Span::raw(" "), text], step_tags(step, outline, th), width.saturating_sub(2)),
        selected,
        width,
        th,
    )
}

/// What stands behind the selected step: the decisions that bind it, and
/// the tasks that carry it out — those its decisions are carried out by
/// and those it names itself — or, when nothing does, the key that binds
/// one.
fn bindings(step: &Step, outline: &Outline<'_>, width: u16, th: Theme) -> Vec<Line<'static>> {
    let decisions: Vec<_> = step.decisions.iter().filter_map(|id| outline.decision(*id)).collect();
    let mut tasks: Vec<i64> = Vec::new();
    let carried = decisions
        .iter()
        .flat_map(|decision| outline.carried_out_by(decision.decision.id))
        .map(|task| task.task.id)
        .chain(step.tasks.iter().copied());
    for id in carried {
        if !tasks.contains(&id) {
            tasks.push(id);
        }
    }
    let tasks: Vec<_> = tasks.iter().filter_map(|id| outline.task(*id)).collect();

    let mut lines = Vec::new();
    if decisions.is_empty() && tasks.is_empty() {
        lines.push(Line::from(span(
            format!("  nothing binds step {} — d names the decision that does", step.number),
            th.dim,
        )));
        return lines;
    }
    if !decisions.is_empty() {
        lines.push(heading(&format!("STEP {} IS BOUND BY", step.number), false, "", width, th));
        lines.extend(decisions.iter().map(|decision| decision_ref(decision, width, th)));
    }
    if !tasks.is_empty() {
        if !lines.is_empty() {
            lines.push(Line::raw(""));
        }
        lines.push(heading("CARRIED OUT BY", false, "", width, th));
        lines.extend(tasks.iter().map(|task| task_ref(task, width, th)));
    }
    lines
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
            let summary = argus_protocol::flow::summary(&argus_protocol::flow::rows(&diagram.body));
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
    let below_flows = list.height.saturating_sub(flows_height + 3);
    let outline = app.outline();
    let bound = app
        .flow_steps()
        .get(app.step_sel)
        .map(|step| bindings(step, &outline, list.width, th))
        .unwrap_or_default();
    // The steps keep at least a few rows of their own: they are what the
    // cursor is on, and the bindings only say more about one of them.
    let bound_height = match bound.len() {
        0 => 0,
        len => (len as u16 + 1).min(below_flows.saturating_sub(6)),
    };
    let flows_head = heading("FLOWS", false, &format!("{} ", diagrams.len()), list.width, th);

    let rows = app.flow_rows();
    let route_width = rows
        .iter()
        .filter_map(|row| match row {
            FlowRow::Step(step) => Some(step.route().chars().count() + 1),
            FlowRow::Branch { .. } => None,
        })
        .max()
        .unwrap_or(0)
        .min(ROUTE_WIDTH);
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
                step_line(step, selected, route_width, &outline, list.width, th)
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

    drop(outline);
    // The steps take what they need and the bindings sit under them,
    // where the eye already is; a long flow scrolls instead.
    let steps_room = below_flows - bound_height;
    let steps_area = Rect {
        y: list.y + (flows_height + 3).min(list.height),
        height: (step_lines.len() as u16 + 2).min(steps_room),
        ..list
    };
    let bound_area = Rect {
        y: steps_area.bottom() + 1.min(bound_height),
        height: bound_height.saturating_sub(1),
        ..list
    };
    paint(f, &bound, bound_area, 0);

    let Some(diagram) = app.selected_diagram().cloned() else {
        return;
    };
    let steps = app.flow_steps();
    let texts: Vec<&str> = steps.iter().map(|step| step.text.as_str()).collect();
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
    // The selected step's label and the arrow drawn under it.
    let lit = drawn.step_line(&texts, app.step_sel);
    let lit_line = |at: usize| lit.is_some_and(|line| at == line || at == line + 1);
    let drawing: Vec<Line<'static>> = drawn
        .lines
        .iter()
        .enumerate()
        .map(|(at, line)| {
            let color = if lit_line(at) { th.accent } else { th.dim };
            Line::from(span(drawn.line_window(line, width), color))
        })
        .collect();
    paint(f, &[Line::from(span(diagram.title.to_uppercase(), th.muted))], Rect { height: 1, ..picture }, 0);
    let drawing_area = Rect {
        y: picture.y + 2,
        height: picture.height.saturating_sub(3),
        ..picture
    };
    let visible = usize::from(drawing_area.height);
    drawn.scroll = scrolled_to_show(drawn.scroll, lit.map(|line| line + 1), visible, drawing.len());
    paint(f, &drawing, drawing_area, drawn.scroll);
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
