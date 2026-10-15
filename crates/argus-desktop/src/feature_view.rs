//! What a feature opened from the index shows: its header — title, how it
//! stands, its branch and agent, brief, progress and tickets — and the lens
//! it is read through, each lens built by its own module off one outline.
//!
//! The Work lens is always built, since the outline is what every lens's
//! detail reads; Record and Flows only while they are the one read.

use std::collections::HashSet;

use argus_protocol::outline::{Outline, WorkRef};
use argus_protocol::standing::{self, FeatureRow};
use argus_protocol::{Changes, Decision, DecisionBoard, DiagramList, Feature, ProjectInfo, TaskList};

use crate::features::{loudest_agent, word};
use crate::flows_lens::{self, FlowCursor};
use crate::page::{FeatureView, Lens, Link};
use crate::{record_lens, work_lens};

pub struct Opened<'a> {
    pub feature: &'a Feature,
    pub row: &'a FeatureRow,
    pub board: &'a DecisionBoard,
    /// `None` until the feature's tasks arrive.
    pub tasks: Option<&'a TaskList>,
    /// `None` until the feature's flows arrive.
    pub diagrams: Option<&'a DiagramList>,
    pub tree: &'a [ProjectInfo],
    pub lens: Lens,
    /// Each lens's cursor.
    pub work: Option<WorkRef>,
    pub record: Option<i64>,
    pub flow: FlowCursor,
    /// Seconds since the Unix epoch, for how long ago it was reconciled.
    pub now: u64,
}

pub fn build(opened: &Opened) -> FeatureView {
    let Opened { feature, row, board, tasks, diagrams, tree, lens, work, record, flow, now } = *opened;
    let scoped = board.scoped(Some(&feature.slug));
    let empty = TaskList {
        project_name: String::new(),
        feature: Some(feature.slug.clone()),
        tasks: Vec::new(),
        repositories: Vec::new(),
    };
    let list = tasks.unwrap_or(&empty);
    let outline = Outline::build(list.tree_rows(), scoped.tree_rows(), &HashSet::new());
    // Counted only once the tasks are here: before, every task seen at the
    // last reconcile would read as gone.
    let filed: Vec<&Decision> = scoped.decisions.iter().collect();
    let changes = feature
        .reconciled
        .as_ref()
        .filter(|_| tasks.is_some())
        .map(|reconciled| Changes::since(reconciled, &list.tasks, &filed));
    let work = work_lens::build(&outline, work, changes.as_ref());
    FeatureView {
        slug: feature.slug.clone(),
        number: feature.number,
        title: feature.title.clone(),
        standing: word(row.standing),
        branch: row.branch.clone(),
        agent: loudest_agent(&standing::panes_on(feature, tree)),
        brief: feature.body.clone(),
        links: feature.links.iter().map(Link::of).collect(),
        reconciled: feature.reconciled.as_ref().map(|r| {
            format!("reconciled {} ago by {}", argus_protocol::ago(Some(r.at.max(0) as u64), now), r.by)
        }),
        changes: changes.as_ref().map(Changes::count),
        reconcile: feature.reconcile.as_ref().map(|r| r.stage().as_str()),
        tasks_done: row.tasks.done,
        tasks_total: row.tasks.total(),
        decided: row.decided,
        open: row.open,
        lens,
        loaded: tasks.is_some(),
        rows: work.rows,
        task: work.task,
        decision: work.decision,
        record: (lens == Lens::Record).then(|| record_lens::build(&outline, record)),
        flows: (lens == Lens::Flows).then(|| flows_lens::build(diagrams, &outline, flow)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn view(lens: Lens, tasks: Option<&TaskList>) -> FeatureView {
        let board = fixtures::open_board();
        let tree = fixtures::feature_tree();
        let rows = standing::feature_rows(&board, &tree);
        let row = rows.iter().find(|r| r.slug == "trim").unwrap();
        let feature = board.features.iter().find(|f| f.slug == "trim").unwrap();
        let diagrams = fixtures::diagrams("trim");
        build(&Opened {
            feature,
            row,
            board: &board,
            tasks,
            diagrams: Some(&diagrams),
            tree: &tree,
            lens,
            work: None,
            record: None,
            flow: FlowCursor::default(),
            now: 2 * 3600,
        })
    }

    #[test]
    fn the_header_names_the_agent_the_tickets_and_the_lens() {
        let tasks = fixtures::open_tasks();
        let open = view(Lens::Work, Some(&tasks));
        assert_eq!((open.agent.as_ref().map(|a| a.title.as_str()), open.loaded), (Some("claude"), true));
        assert_eq!(open.links[0].key, "ENG-88");
        assert_eq!(open.lens, Lens::Work);
        assert_eq!((open.number, open.rows.len()), (7, 6));
    }

    #[test]
    fn only_the_lens_read_is_built_beside_the_outline() {
        let tasks = fixtures::open_tasks();
        let work = view(Lens::Work, Some(&tasks));
        assert!(work.record.is_none() && work.flows.is_none());
        let record = view(Lens::Record, Some(&tasks));
        assert!(record.record.is_some() && record.flows.is_none());
        assert!(!record.rows.is_empty(), "the outline is always sent");
        let flows = view(Lens::Flows, Some(&tasks));
        assert_eq!(flows.flows.unwrap().flows.len(), 2);
    }

    #[test]
    fn tasks_not_yet_arrived_leave_the_outline_to_its_decisions() {
        let open = view(Lens::Work, None);
        assert!(!open.loaded);
        assert!(open.rows.iter().all(|r| matches!(r.at, crate::page::WorkAt::Decision { .. })));
    }

    /// The header says when it was last reconciled and how much moved
    /// since, and the outline marks each row that moved.
    #[test]
    fn the_header_counts_the_changes_since_the_last_reconcile_and_the_outline_marks_them() {
        let mut board = fixtures::open_board();
        let mut tasks = fixtures::open_tasks();
        let filed: Vec<&Decision> = board.decisions.iter().filter(|d| d.feature.as_deref() == Some("trim")).collect();
        let seen = argus_protocol::reconcile::seen(&tasks.tasks, &filed);
        let trim = board.features.iter_mut().find(|f| f.slug == "trim").unwrap();
        trim.reconciled = Some(argus_protocol::Reconciled { at: 0, by: "pane #32".into(), seen });
        tasks.tasks[1].title = "renamed since".into();
        let tree = fixtures::feature_tree();
        let rows = standing::feature_rows(&board, &tree);
        let open = |tasks: Option<&TaskList>| {
            build(&Opened {
                feature: board.features.iter().find(|f| f.slug == "trim").unwrap(),
                row: rows.iter().find(|r| r.slug == "trim").unwrap(),
                board: &board,
                tasks,
                diagrams: None,
                tree: &tree,
                lens: Lens::Work,
                work: None,
                record: None,
                flow: FlowCursor::default(),
                now: 2 * 3600,
            })
        };
        let view = open(Some(&tasks));
        assert_eq!(view.reconciled.as_deref(), Some("reconciled 2h ago by pane #32"));
        assert_eq!(view.changes, Some(1));
        let changed: Vec<_> = view.rows.iter().filter(|r| r.changed).map(|r| r.label.as_str()).collect();
        assert_eq!(changed.len(), 1, "only the renamed task: {changed:?}");
        assert_eq!(open(None).changes, None, "nothing is counted before the tasks arrive");
    }

    #[test]
    #[ignore]
    fn dump_open_feature() {
        let tasks = fixtures::open_tasks();
        for lens in [Lens::Work, Lens::Record, Lens::Flows] {
            let open = view(lens, Some(&tasks));
            println!("Features › #{} › {lens:?}   {} [{}]", open.number, open.title, open.standing);
            let links: Vec<_> = open.links.iter().map(|l| format!("{} {} ({})", l.tracker, l.key, l.role)).collect();
            println!("  {}   tickets: {}", open.brief, links.join(", "));
            let changes = open.changes.map(|n| format!(" · {n} changes since")).unwrap_or_default();
            println!("  {}{changes}   reconcile: {}", open.reconciled.as_deref().unwrap_or("never reconciled"), open.reconcile.unwrap_or("none"));
            println!("  {} of {} tasks · {} decided · {} open", open.tasks_done, open.tasks_total, open.decided, open.open);
            match lens {
                Lens::Work => {
                    for row in &open.rows {
                        let mark = if row.on { '▌' } else if row.changed { '•' } else { ' ' };
                        let waits = row.waits_on.as_ref().map(|w| format!("  waits on {w}")).unwrap_or_default();
                        println!("{mark} {}{:<5} {:<9} {}{waits}", "  ".repeat(row.depth), row.label, row.state, row.text);
                    }
                }
                Lens::Record => {
                    let record = open.record.unwrap();
                    for row in &record.rows {
                        println!("{} {}{:<5} {:<9} {}", if row.on { '▌' } else { ' ' }, "  ".repeat(row.depth), row.label, row.state, row.text);
                    }
                    println!("{}", record.decision.unwrap().adr);
                }
                Lens::Flows => {
                    let flows = open.flows.unwrap();
                    for flow in &flows.flows {
                        println!("{} {}  {}", if flow.on { '▌' } else { ' ' }, flow.title, flow.summary);
                    }
                    let flow = flows.flow.unwrap();
                    for row in &flow.rows {
                        let n = row.number.map_or("  ".into(), |n| format!("{n:<2}"));
                        println!("{} {n} {:<18} {}  {}", if row.on { '▌' } else { ' ' }, row.route, row.text, row.tags.join(" "));
                    }
                    let names: Vec<_> = flow.diagram.lifelines.iter().map(|l| l.label.as_str()).collect();
                    println!("  lifelines: {}", names.join(" | "));
                    for item in &flow.diagram.items {
                        println!("    {item:?}");
                    }
                }
            }
            println!();
        }
    }
}
