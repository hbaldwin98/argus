use super::*;
use crate::fixtures::{checkout, pane, project, repository};
use argus_protocol::{PaneKind, PaneStatus};

/// A project holding one repository per `(repository name, folder)`.
fn holding(id: u64, name: &str, repositories: &[(&str, &str)]) -> ProjectInfo {
    let repositories = repositories
        .iter()
        .enumerate()
        .map(|(n, (repo, folder))| {
            let mut main = checkout(id * 100 + n as u64, "main", true, Vec::new());
            main.path = (*folder).into();
            repository(id * 100 + n as u64, repo, None, vec![main])
        })
        .collect();
    project(id, name, repositories)
}

fn tree() -> Vec<ProjectInfo> {
    let mut running = holding(2, "work", &[("web", "/src/web")]);
    let mut main = checkout(21, "main", true, vec![pane(1, PaneKind::Agent, "claude", PaneStatus::Working), pane(2, PaneKind::Shell, "sh", PaneStatus::Idle)]);
    main.path = "/src/web".into();
    running.repositories[0].checkouts = vec![main];
    let mut dots = holding(3, "dots", &[("web", "/src/web"), ("dots", "/home/me/dots")]);
    dots.root = Some("/home/me".into());
    vec![holding(1, "argus", &[("argus", "/src/argus")]), running, dots]
}

fn refusal(request_id: u64, reason: &str) -> RequestRefusal {
    RequestRefusal { request_id, reason: reason.into(), read: false }
}

#[test]
fn the_ask_names_the_project_folded_into_and_the_ticked_ones_in_the_trees_order() {
    let mut dialog = Combine::new(ProjectId(1), Some(ProjectId(3)));
    dialog.tick(ProjectId(2));
    let Some(ClientMsg::CombineProjects { into, from, request_id }) = dialog.combine(&tree(), || 7) else { panic!("asked") };
    assert_eq!((into, from, request_id), (ProjectId(1), vec![ProjectId(2), ProjectId(3)], 7));
    assert!(dialog.page(&tree()).unwrap().combining);
}

#[test]
fn nothing_is_asked_with_nothing_ticked_or_while_an_ask_is_with_the_daemon() {
    let mut dialog = Combine::new(ProjectId(1), None);
    assert!(dialog.combine(&tree(), || 1).is_none(), "nothing ticked");
    dialog.tick(ProjectId(2));
    assert!(dialog.combine(&tree(), || 2).is_some());
    assert!(dialog.combine(&tree(), || 3).is_none(), "one at a time");
    dialog.tick(ProjectId(3));
    assert_eq!(dialog.page(&tree()).unwrap().rows.iter().filter(|r| r.ticked).count(), 1, "the ticks wait too");
}

#[test]
fn the_project_folded_into_is_never_one_folded_in() {
    let mut dialog = Combine::new(ProjectId(1), Some(ProjectId(2)));
    dialog.tick(ProjectId(1));
    dialog.set_into(ProjectId(2));
    let page = dialog.page(&tree()).unwrap();
    assert_eq!(page.into.map(|p| p.id), Some(2));
    assert_eq!(page.rows.iter().map(|r| (r.id, r.ticked)).collect::<Vec<_>>(), [(1, false), (3, false)]);
    assert_eq!(page.choices.len(), 3, "the Into control offers every project");
}

#[test]
fn a_refusal_keeps_the_dialog_and_the_ticks_and_can_be_tried_again() {
    let mut dialog = Combine::new(ProjectId(1), Some(ProjectId(2)));
    dialog.combine(&tree(), || 9);
    assert!(!dialog.refused(&refusal(8, "another request")));
    assert!(dialog.refused(&refusal(9, "a pane is busy")));
    let page = dialog.page(&tree()).unwrap();
    assert_eq!((page.combining, page.refusal.as_deref()), (false, Some("a pane is busy")));
    assert!(page.rows[0].ticked, "what was ticked stays");
    assert!(!dialog.refused(&refusal(9, "again")), "nothing waits on it now");
    assert!(dialog.combine(&tree(), || 10).is_some());
    assert_eq!(dialog.page(&tree()).unwrap().refusal, None, "a new ask clears the old reason");
}

#[test]
fn a_new_connection_lets_go_of_an_ask_nothing_will_answer() {
    let mut dialog = Combine::new(ProjectId(1), Some(ProjectId(2)));
    dialog.combine(&tree(), || 9);
    dialog.reconnected();
    assert!(!dialog.page(&tree()).unwrap().combining);
}

#[test]
fn the_page_reads_what_the_ticks_would_hold_from_the_tree() {
    let mut dialog = Combine::new(ProjectId(1), None);
    let none = dialog.page(&tree()).unwrap();
    assert_eq!((none.repositories, none.holds.as_slice(), none.agents), (1, ["argus".to_owned()].as_slice(), 0));
    dialog.tick(ProjectId(2));
    dialog.tick(ProjectId(3));
    let page = dialog.page(&tree()).unwrap();
    assert_eq!(page.holds, ["argus", "web", "dots"], "web is held by both and listed once");
    assert_eq!(page.repositories, 3);
    assert_eq!(page.agents, 1, "the shell is not an agent");
    assert_eq!(page.rows.iter().map(|r| (r.id, r.repositories, r.scans_folder)).collect::<Vec<_>>(), [(2, 1, false), (3, 2, true)]);
    assert_eq!(page.rows[0].working, 1);
}

#[test]
fn the_dialog_goes_when_the_project_folded_into_leaves_the_tree() {
    let mut dialog = Combining::default();
    dialog.open(Combine::new(ProjectId(1), None));
    assert!(matches!(dialog.take(&tree()), Some(Event::Combine(page)) if page.open));
    assert!(dialog.take(&tree()).is_none(), "heard once");
    assert!(matches!(dialog.take(&tree()[1..]), Some(Event::Combine(page)) if !page.open));
    assert!(!dialog.is_open());
}
