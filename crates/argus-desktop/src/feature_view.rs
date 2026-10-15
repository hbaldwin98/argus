//! What a feature opened from the index shows: its tasks and the decisions
//! raised under them as one outline, and the task or decision under the
//! cursor in full.
//!
//! Where a decision sits among the tasks, what binds a task and what an
//! open question holds up are `argus_protocol::outline`'s to say, as they
//! are for the terminal client's Work lens; this only turns them into what
//! the page draws.

use std::collections::HashSet;

use argus_protocol::outline::{Outline, WorkRef};
use argus_protocol::standing::{self, FeatureRow};
use argus_protocol::{Decision, DecisionBoard, DecisionText, DecisionTreeRow, Feature, ProjectInfo, TaskList, TaskTreeRow};

use crate::features::{loudest_agent, word};
use crate::page::{Criterion, DecisionDetail, FeatureView, Line, TaskDetail, WorkAt, WorkRow};

pub struct Opened<'a> {
    pub feature: &'a Feature,
    pub row: &'a FeatureRow,
    pub board: &'a DecisionBoard,
    /// `None` until the feature's tasks arrive.
    pub tasks: Option<&'a TaskList>,
    pub tree: &'a [ProjectInfo],
    pub cursor: Option<WorkRef>,
}

pub fn build(opened: &Opened) -> FeatureView {
    let Opened { feature, row, board, tasks, tree, cursor } = *opened;
    let scoped = board.scoped(Some(&feature.slug));
    let empty = TaskList {
        project_name: String::new(),
        feature: Some(feature.slug.clone()),
        tasks: Vec::new(),
        repositories: Vec::new(),
    };
    let list = tasks.unwrap_or(&empty);
    let outline = Outline::build(list.tree_rows(), scoped.tree_rows(), &HashSet::new());
    // A cursor on a row that went away lands on the first.
    let cursor = cursor
        .filter(|at| outline.rows.iter().any(|row| row.at == *at))
        .or_else(|| outline.rows.first().map(|row| row.at));

    let rows = outline
        .rows
        .iter()
        .map(|work| {
            let on = Some(work.at) == cursor;
            match work.at {
                WorkRef::Task(_) => {
                    let task = &outline.tasks[work.index];
                    WorkRow {
                        at: at(work.at),
                        label: task.label(),
                        depth: work.depth,
                        text: task.task.title.clone(),
                        state: task.task.state.as_str(),
                        waits_on: work.waits_on.clone(),
                        on,
                    }
                }
                WorkRef::Decision(_) => {
                    let decision = &outline.decisions[work.index];
                    WorkRow {
                        at: at(work.at),
                        label: decision.label(),
                        depth: work.depth,
                        text: decision.decision.chose.clone(),
                        state: decision_state(decision.decision),
                        waits_on: None,
                        on,
                    }
                }
            }
        })
        .collect();

    let (task, decision) = match cursor {
        Some(WorkRef::Task(id)) => (outline.task(id).map(|row| task_detail(row, &outline)), None),
        Some(WorkRef::Decision(id)) => (None, outline.decision(id).map(|row| decision_detail(row, &outline))),
        None => (None, None),
    };
    FeatureView {
        slug: feature.slug.clone(),
        number: feature.number,
        title: feature.title.clone(),
        standing: word(row.standing),
        branch: row.branch.clone(),
        agent: loudest_agent(&standing::panes_on(feature, tree)),
        brief: feature.body.clone(),
        tasks_done: row.tasks.done,
        tasks_total: row.tasks.total(),
        decided: row.decided,
        open: row.open,
        loaded: tasks.is_some(),
        rows,
        task,
        decision,
    }
}

fn at(work: WorkRef) -> WorkAt {
    match work {
        WorkRef::Task(id) => WorkAt::Task { id },
        WorkRef::Decision(id) => WorkAt::Decision { id },
    }
}

pub fn work_ref(at: WorkAt) -> WorkRef {
    match at {
        WorkAt::Task { id } => WorkRef::Task(id),
        WorkAt::Decision { id } => WorkRef::Decision(id),
    }
}

fn decision_state(decision: &Decision) -> &'static str {
    if decision.withdrawn() {
        "withdrawn"
    } else if decision.superseded() {
        "superseded"
    } else {
        decision.state.as_str()
    }
}

fn task_detail(row: &TaskTreeRow, outline: &Outline) -> TaskDetail {
    let task = row.task;
    let waits_on = outline
        .rows
        .iter()
        .find(|work| work.at == WorkRef::Task(task.id))
        .and_then(|work| work.waits_on.clone());
    TaskDetail {
        id: task.id,
        label: row.label(),
        title: task.title.clone(),
        body: task.body.clone().unwrap_or_default(),
        state: task.state.as_str(),
        parent: task
            .parent
            .and_then(|parent| outline.task(parent))
            .map(|parent| format!("{} · {}", parent.label(), parent.task.title)),
        criteria: task.criteria.iter().map(|c| Criterion { text: c.text.clone(), done: c.done }).collect(),
        bound_by: outline
            .bound_by(task.id)
            .into_iter()
            .map(|row| Line { label: row.label(), text: row.decision.chose.clone(), aside: None })
            .collect(),
        notes: task
            .notes
            .iter()
            .map(|note| Line {
                label: if note.session.is_some() { "agent" } else { "you" }.into(),
                text: note.text.clone(),
                aside: None,
            })
            .collect(),
        waits_on,
    }
}

fn decision_detail(row: &DecisionTreeRow, outline: &Outline) -> DecisionDetail {
    let decision = row.decision;
    DecisionDetail {
        id: decision.id,
        label: row.label(),
        chose: decision.chose.clone(),
        state: decision_state(decision),
        withdrawn: decision.withdrawn(),
        raised_under: outline.raised_under(decision.id).map(|task| format!("task {}", task.label())),
        context: decision.context.clone(),
        detail: decision.detail.clone(),
        because: decision.because.clone(),
        over: decision.over.clone(),
        consequences: decision.consequences.iter().map(|c| format!("{} {}", c.sign(), c.text)).collect(),
        options: decision.options.clone(),
        carried_out_by: outline
            .carried_out_by(decision.id)
            .into_iter()
            .map(|row| Line { label: row.label(), text: row.task.title.clone(), aside: None })
            .collect(),
        document: DecisionText::from(decision).markdown(decision.open(), true).trim_end().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use argus_protocol::{Consequence, DecisionState, TaskNote};

    /// `trim` with its open question raised under task 4 and its made
    /// decision under task 1, one criterion checked off on task 3.
    fn board() -> DecisionBoard {
        let mut board = fixtures::board();
        for decision in &mut board.decisions {
            decision.task = Some(if decision.state == DecisionState::Open { 4 } else { 1 });
        }
        board.decisions[1].options = vec!["Every standing decision".into(), "Only the binding ones".into()];
        board.decisions[0].consequences = vec![Consequence { gain: false, text: "one more request".into() }];
        board
    }

    fn tasks() -> TaskList {
        let mut tasks = fixtures::tasks("trim");
        tasks.tasks[2].criteria = vec![
            argus_protocol::Criterion { text: "The route answers".into(), done: true },
            argus_protocol::Criterion { text: "A test covers it".into(), done: false },
        ];
        tasks.tasks[2].notes = vec![TaskNote { text: "wiring it now".into(), session: Some("s".into()), ..TaskNote::default() }];
        tasks
    }

    fn view(cursor: Option<WorkRef>, tasks: Option<&TaskList>) -> FeatureView {
        let board = board();
        let tree = fixtures::feature_tree();
        let rows = standing::feature_rows(&board, &tree);
        let row = rows.iter().find(|r| r.slug == "trim").unwrap();
        let feature = board.features.iter().find(|f| f.slug == "trim").unwrap();
        build(&Opened { feature, row, board: &board, tasks, tree: &tree, cursor })
    }

    #[test]
    fn the_outline_reads_tasks_and_the_decisions_raised_under_them_as_one() {
        let tasks = tasks();
        let open = view(None, Some(&tasks));
        let rows: Vec<_> = open.rows.iter().map(|r| (r.label.as_str(), r.depth, r.state)).collect();
        assert_eq!(
            rows,
            [
                ("1", 0, "doing"),
                ("1.1", 1, "done"),
                ("1.2", 1, "doing"),
                ("D1", 1, "accepted"),
                ("2", 0, "todo"),
                ("D2", 1, "open"),
            ]
        );
        assert!(open.rows[0].on, "the cursor starts on the first row");
        assert_eq!((open.agent.as_ref().map(|a| a.title.as_str()), open.loaded), (Some("claude"), true));
    }

    #[test]
    fn a_task_under_the_cursor_shows_its_criteria_what_binds_it_and_its_notes() {
        let tasks = tasks();
        let open = view(Some(WorkRef::Task(3)), Some(&tasks));
        let task = open.task.unwrap();
        assert_eq!((task.label.as_str(), task.parent.as_deref()), ("1.2", Some("1 · Serve the brief on demand")));
        assert_eq!(task.criteria.iter().map(|c| c.done).collect::<Vec<_>>(), [true, false]);
        assert_eq!(task.bound_by[0].label, "D1");
        assert_eq!((task.notes[0].label.as_str(), task.notes[0].text.as_str()), ("agent", "wiring it now"));
        assert!(open.decision.is_none());
    }

    #[test]
    fn an_open_decision_under_the_cursor_offers_its_options_and_its_record() {
        let tasks = tasks();
        let open = view(Some(WorkRef::Decision(2)), Some(&tasks));
        let decision = open.decision.unwrap();
        assert_eq!((decision.label.as_str(), decision.state), ("D2", "open"));
        assert_eq!(decision.options.len(), 2);
        assert_eq!(decision.raised_under.as_deref(), Some("task 2"));
        assert!(decision.document.starts_with("How much history?\n\n## Context"));
        assert!(decision.document.contains("## Options\n- Every standing decision"));
        let made = view(Some(WorkRef::Decision(1)), Some(&tasks)).decision.unwrap();
        assert_eq!(made.consequences, ["- one more request"]);
        assert_eq!(made.carried_out_by.len(), 2, "what is under task 1");
    }

    #[test]
    fn a_withdrawn_decision_says_so_and_a_cursor_on_a_row_gone_moves_to_the_first() {
        let tasks = tasks();
        let mut board = board();
        board.decisions[0].withdrawn_at = Some(5);
        let tree = fixtures::feature_tree();
        let rows = standing::feature_rows(&board, &tree);
        let row = rows.iter().find(|r| r.slug == "trim").unwrap();
        let feature = board.features.iter().find(|f| f.slug == "trim").unwrap();
        let open = build(&Opened { feature, row, board: &board, tasks: Some(&tasks), tree: &tree, cursor: Some(WorkRef::Decision(1)) });
        let decision = open.decision.unwrap();
        assert_eq!((decision.state, decision.withdrawn), ("withdrawn", true));

        let gone = view(Some(WorkRef::Task(99)), Some(&tasks));
        assert!(gone.rows[0].on && gone.task.is_some());
    }

    #[test]
    #[ignore]
    fn dump_open_feature() {
        let tasks = tasks();
        let open = view(Some(WorkRef::Decision(2)), Some(&tasks));
        println!("Features › #{}  {} [{}]", open.number, open.title, open.standing);
        println!("  {}", open.brief);
        println!("  {} of {} tasks · {} decided · {} open", open.tasks_done, open.tasks_total, open.decided, open.open);
        for row in &open.rows {
            let mark = if row.on { '▌' } else { ' ' };
            let waits = row.waits_on.as_ref().map(|w| format!("  waits on {w}")).unwrap_or_default();
            println!("{mark} {}{:<5} {:<9} {}{waits}", "  ".repeat(row.depth), row.label, row.state, row.text);
        }
        let d = open.decision.unwrap();
        println!("-- {} {} (raised under {})", d.label, d.state, d.raised_under.unwrap_or_default());
        println!("   {}", d.chose);
        for (i, option) in d.options.iter().enumerate() {
            println!("   ( ) option {}: {option}", i + 1);
        }
        println!("   [Take option N] [Answer in words…]   [Edit record] [Sub-decision] [Withdraw]");
    }

    #[test]
    fn tasks_not_yet_arrived_leave_the_outline_to_its_decisions() {
        let open = view(None, None);
        assert!(!open.loaded);
        assert!(open.rows.iter().all(|r| matches!(r.at, WorkAt::Decision { .. })));
    }
}
