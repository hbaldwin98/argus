//! The Flows lens of an open feature: its sequence diagrams, the one under
//! the cursor read as steps, the step under the cursor with what binds it
//! and what carries it out, and the diagram drawn beside them.
//!
//! A step is bound in its flow's source
//! (`argus_protocol::board::diagrams::flow`) and what carries a decision
//! out is the outline's to say, as the terminal client's Flows lens reads
//! them; this only turns them into what the page draws, `sequence` drawing
//! the diagram.

use argus_protocol::board::diagrams::flow::{self, FlowRow, Step};
use argus_protocol::board::outline::Outline;
use argus_protocol::{DiagramList, SequenceDiagram};

use crate::connection::page::{FlowDetail, FlowItem, FlowsLens, Line, StepDetail, StepRow};
use crate::sequence;

/// Where the lens's cursor is: a flow by id, and a step of it by its place
/// among the steps, counted from zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FlowCursor {
    pub diagram: Option<i64>,
    pub step: usize,
}

/// The flow the cursor is on: its own while it is listed, else the first.
pub fn on(list: Option<&DiagramList>, cursor: FlowCursor) -> Option<&SequenceDiagram> {
    let list = list?;
    cursor
        .diagram
        .and_then(|id| list.diagrams.iter().find(|d| d.id == id))
        .or_else(|| list.diagrams.first())
}

/// A flow's steps in order, its branch lines left out.
pub fn steps(rows: &[FlowRow]) -> Vec<&Step> {
    rows.iter()
        .filter_map(|row| match row {
            FlowRow::Step(step) => Some(step),
            FlowRow::Branch { .. } => None,
        })
        .collect()
}

/// The flows with the cursor on one; a flow gone lands on the first, and a
/// step past the last on the last.
pub fn build(list: Option<&DiagramList>, outline: &Outline, cursor: FlowCursor) -> FlowsLens {
    let decisions = outline
        .decisions
        .iter()
        .filter(|row| row.decision.stands() || row.decision.open())
        .map(|row| Line { label: row.label(), text: row.decision.chose.clone(), aside: Some(format!("#{}", row.decision.id)) })
        .collect();
    let Some(list) = list else {
        return FlowsLens { loaded: false, flows: Vec::new(), flow: None, decisions };
    };
    let on = on(Some(list), cursor).map(|d| d.id);
    let flows = list
        .diagrams
        .iter()
        .map(|diagram| FlowItem {
            id: diagram.id,
            title: diagram.title.clone(),
            summary: flow::summary(&flow::rows(&diagram.body)),
            on: Some(diagram.id) == on,
        })
        .collect();
    let flow = list.diagrams.iter().find(|d| Some(d.id) == on).map(|diagram| {
        let rows = flow::rows(&diagram.body);
        let steps = steps(&rows);
        let lit = steps.get(cursor.step.min(steps.len().saturating_sub(1))).map(|step| step.number);
        let participants = flow::participants(&diagram.body);
        let label = |name: &str| participants.iter().find(|p| p.id == name).map_or(name.to_string(), |p| p.label.clone());
        FlowDetail {
            id: diagram.id,
            title: diagram.title.clone(),
            source: diagram.body.clone(),
            rows: rows
                .iter()
                .map(|row| match row {
                    FlowRow::Step(step) => StepRow {
                        number: Some(step.number),
                        route: step.route(),
                        text: step.text.clone(),
                        tags: tags(step, outline),
                        on: Some(step.number) == lit,
                    },
                    FlowRow::Branch { kind, when } => {
                        StepRow { number: None, route: kind.clone(), text: when.clone(), tags: Vec::new(), on: false }
                    }
                })
                .collect(),
            step: steps.iter().find(|step| Some(step.number) == lit).map(|step| StepDetail {
                number: step.number,
                route: step.route(),
                text: step.text.clone(),
                participants: std::iter::once(&step.from).chain(step.to.as_ref()).map(|name| label(name)).collect(),
                bound_by: step
                    .decisions
                    .iter()
                    .filter_map(|id| outline.decision(*id))
                    .map(|row| Line { label: row.label(), text: row.decision.chose.clone(), aside: None })
                    .collect(),
                carried_out_by: carried_out_by(step, outline),
            }),
            diagram: sequence::read(&diagram.body, lit),
        }
    });
    FlowsLens { loaded: true, flows, flow, decisions }
}

/// What a step's row names on its right: each decision as a reader calls
/// it, then each task its own comment names; an id the board no longer has
/// by its id.
fn tags(step: &Step, outline: &Outline) -> Vec<String> {
    let decisions = step.decisions.iter().map(|id| outline.decision(*id).map_or(format!("#{id}"), |row| row.label()));
    let tasks = step
        .tasks
        .iter()
        .map(|id| outline.task(*id).map_or(format!("task #{id}"), |row| format!("task {}", row.label())));
    decisions.chain(tasks).collect()
}

/// The tasks its decisions are carried out by, then those it names itself,
/// each once.
fn carried_out_by(step: &Step, outline: &Outline) -> Vec<Line> {
    let mut ids: Vec<i64> = Vec::new();
    let carried = step
        .decisions
        .iter()
        .flat_map(|id| outline.carried_out_by(*id))
        .map(|task| task.task.id)
        .chain(step.tasks.iter().copied());
    for id in carried {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids.iter()
        .filter_map(|id| outline.task(*id))
        .map(|row| Line { label: row.label(), text: row.task.title.clone(), aside: None })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use std::collections::HashSet;

    fn lens(list: Option<&DiagramList>, cursor: FlowCursor) -> FlowsLens {
        let board = fixtures::open_board();
        let tasks = fixtures::open_tasks();
        let scoped = board.scoped(Some("trim"));
        let outline = Outline::build(tasks.tree_rows(), scoped.tree_rows(), &HashSet::new());
        build(list, &outline, cursor)
    }

    #[test]
    fn the_first_flow_is_read_as_steps_with_its_branch_between_them() {
        let list = fixtures::diagrams("trim");
        let flows = lens(Some(&list), FlowCursor::default());
        assert!(flows.loaded);
        let listed: Vec<_> = flows.flows.iter().map(|f| (f.title.as_str(), f.summary.as_str(), f.on)).collect();
        assert_eq!(listed, [("Asking for the brief", "5 steps · 1 branch", true), ("Capping", "1 step", false)]);
        let flow = flows.flow.unwrap();
        let rows: Vec<_> = flow.rows.iter().map(|r| (r.number, r.route.as_str())).collect();
        assert_eq!(rows[4], (None, "else"));
        assert_eq!(rows[1], (Some(2), "hook → argusd"));
        assert_eq!(flow.rows[1].tags, ["D1"]);
        assert!(flow.rows[0].on, "the cursor starts on the first step");
    }

    #[test]
    fn the_step_under_the_cursor_names_its_participants_what_binds_it_and_what_carries_it_out() {
        let list = fixtures::diagrams("trim");
        let flow = lens(Some(&list), FlowCursor { diagram: Some(11), step: 1 }).flow.unwrap();
        let step = flow.step.unwrap();
        assert_eq!(step.participants, ["argus-hook", "argusd"]);
        assert_eq!(step.bound_by[0].label, "D1");
        let carried: Vec<_> = step.carried_out_by.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(carried, ["1.1", "1.2"], "what is under task 1, where D1 was raised");
        assert!(flow.diagram.items.iter().any(|item| matches!(item, crate::connection::page::SequenceItem::Message { step: 2, on: true, .. })));
    }

    #[test]
    fn a_flow_gone_hands_the_cursor_to_the_first_and_a_step_past_the_end_to_the_last() {
        let list = fixtures::diagrams("trim");
        let flows = lens(Some(&list), FlowCursor { diagram: Some(99), step: 40 });
        let flow = flows.flow.unwrap();
        assert_eq!(flow.id, 11);
        assert_eq!(flow.step.unwrap().number, 5);
    }

    #[test]
    fn flows_not_yet_arrived_say_so_and_still_offer_the_decisions_to_bind() {
        let flows = lens(None, FlowCursor::default());
        assert!(!flows.loaded && flows.flow.is_none());
        let offered: Vec<_> = flows.decisions.iter().map(|l| (l.label.as_str(), l.aside.as_deref())).collect();
        assert_eq!(offered, [("D1", Some("#1")), ("D2", Some("#2"))]);
    }
}
