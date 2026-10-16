//! What a feature opened from the index shows: its header — title, how it
//! stands, its branch and agent, brief, progress and tickets — and the lens
//! it is read through, each lens built by its own module off one outline.
//!
//! The Work lens is always built, since the outline is what every lens's
//! detail reads; Record and Flows only while they are the one read. The rows
//! each lens's cursor steps over are read off the same outline, so a step
//! moves exactly one row drawn.

use std::collections::HashSet;

use argus_protocol::outline::{Outline, WorkRef};
use argus_protocol::standing::{self, FeatureRow};
use argus_protocol::{Changes, Decision, DecisionBoard, DiagramList, Feature, ProjectInfo, TaskList};

use crate::features::{loudest_agent, word};
use crate::flows_lens::{self, FlowCursor};
use crate::page::{FeatureView, FeatureViewLink, Lens, Link};
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
    /// The views agents showed in this repository, each with the feature
    /// number it names; the ones naming this feature are offered.
    pub views: &'a [(i64, FeatureViewLink)],
    /// Seconds since the Unix epoch, for how long ago it was reconciled.
    pub now: u64,
}

/// The rows each lens's cursor steps over, in the order the lens lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorRows {
    pub work: Vec<WorkRef>,
    pub record: Vec<i64>,
}

pub fn cursor_rows(slug: &str, board: &DecisionBoard, tasks: Option<&TaskList>) -> CursorRows {
    with_outline(slug, board, tasks, |outline, _, _| CursorRows {
        work: outline.rows.iter().map(|row| row.at).collect(),
        record: outline.decisions.iter().map(|row| row.decision.id).collect(),
    })
}

/// Reads a feature's outline, as every lens does, with the board scoped to
/// it and the task list it was built from: tasks not yet arrived stand in
/// as none.
fn with_outline<R>(slug: &str, board: &DecisionBoard, tasks: Option<&TaskList>, read: impl FnOnce(&Outline, &DecisionBoard, &TaskList) -> R) -> R {
    let scoped = board.scoped(Some(slug));
    let empty = TaskList {
        project_name: String::new(),
        feature: Some(slug.to_string()),
        tasks: Vec::new(),
        repositories: Vec::new(),
    };
    let list = tasks.unwrap_or(&empty);
    let outline = Outline::build(list.tree_rows(), scoped.tree_rows(), &HashSet::new());
    read(&outline, &scoped, list)
}

pub fn build(opened: &Opened) -> FeatureView {
    with_outline(&opened.feature.slug, opened.board, opened.tasks, |outline, scoped, list| view(opened, outline, scoped, list))
}

fn view(opened: &Opened, outline: &Outline, scoped: &DecisionBoard, list: &TaskList) -> FeatureView {
    let Opened { feature, row, board: _, tasks, diagrams, tree, lens, work, record, flow, views, now } = *opened;
    // Counted only once the tasks are here: before, every task seen at the
    // last reconcile would read as gone.
    let filed: Vec<&Decision> = scoped.decisions.iter().collect();
    let changes = feature
        .reconciled
        .as_ref()
        .filter(|_| tasks.is_some())
        .map(|reconciled| Changes::since(reconciled, &list.tasks, &filed));
    let work = work_lens::build(outline, work, changes.as_ref());
    FeatureView {
        slug: feature.slug.clone(),
        number: feature.number,
        title: feature.title.clone(),
        standing: word(row.standing),
        branch: row.branch.clone(),
        agent: loudest_agent(&standing::panes_on(feature, tree)),
        brief: feature.body.clone(),
        links: feature.links.iter().map(Link::of).collect(),
        views: views.iter().filter(|(number, _)| *number == feature.number).map(|(_, link)| link.clone()).collect(),
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
        record: (lens == Lens::Record).then(|| record_lens::build(outline, record)),
        flows: (lens == Lens::Flows).then(|| flows_lens::build(diagrams, outline, flow)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn linked(number: i64, id: &str) -> (i64, FeatureViewLink) {
        (number, FeatureViewLink { pane: 1, id: id.into(), title: id.into() })
    }

    fn view(lens: Lens, tasks: Option<&TaskList>) -> FeatureView {
        view_with(lens, tasks, &[])
    }

    fn view_with(lens: Lens, tasks: Option<&TaskList>, views: &[(i64, FeatureViewLink)]) -> FeatureView {
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
            views,
            now: 2 * 3600,
        })
    }

    #[test]
    fn the_rows_a_cursor_steps_over_are_the_rows_each_lens_draws() {
        let board = fixtures::open_board();
        let tasks = fixtures::open_tasks();
        let open = view(Lens::Record, Some(&tasks));
        let rows = cursor_rows("trim", &board, Some(&tasks));
        let drawn: Vec<WorkRef> = open.rows.iter().map(|row| work_lens::work_ref(row.at)).collect();
        assert_eq!(rows.work, drawn);
        let record: Vec<i64> = open.record.unwrap().rows.iter().map(|row| row.id).collect();
        assert_eq!(rows.record, record);
        assert!(rows.work.len() > 1 && rows.record.len() > 1, "enough rows to step over");
        assert!(cursor_rows("trim", &board, None).work.iter().all(|at| matches!(at, WorkRef::Decision(_))), "no tasks until they arrive");
    }

    #[test]
    fn the_header_offers_only_the_views_naming_this_feature() {
        let views = [linked(7, "plan"), linked(8, "someone-elses"), linked(7, "estimate")];
        let open = view_with(Lens::Work, None, &views);
        assert_eq!(open.views.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["plan", "estimate"]);
        assert!(view(Lens::Work, None).views.is_empty());
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
                views: &[],
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
