use super::*;
use crate::fixtures::{checkout, pane, project, repository};
use argus_protocol::{PaneKind, PaneStatus, WorkspaceId, WorkspaceInfo};

fn shell(id: u64) -> argus_protocol::PaneInfo {
    pane(id, PaneKind::Shell, "zsh", PaneStatus::Idle)
}

fn repo(id: u64, name: &str, panes: Vec<argus_protocol::PaneInfo>) -> argus_protocol::RepositoryInfo {
    repository(id, name, None, vec![checkout(id * 10, "main", true, panes)])
}

/// "src" (panes 1 and 2) and "work" (pane 3), as the default workspace holds them.
fn tree() -> Vec<ProjectInfo> {
    vec![project(1, "src", vec![repo(10, "argus", vec![shell(1), shell(2)])]), project(2, "work", vec![repo(20, "api", vec![shell(3)])])]
}

fn workspaces(open: &str, projects: usize) -> ServerMsg {
    let listed = |id, name: &str| WorkspaceInfo { id: WorkspaceId(id), name: name.into(), projects, panes: 0, open: name == open };
    ServerMsg::Workspaces(vec![listed(1, "default"), listed(2, "other")])
}

fn connect(tree: Vec<ProjectInfo>, open: &str) -> Session {
    let mut s = Session::default();
    let count = tree.len();
    s.connected(Some(Hello::this_build()), None, vec![ServerMsg::Tree(tree), workspaces(open, count)]);
    s
}

#[test]
fn returns_to_the_pane_a_project_was_left_on() {
    let mut s = connect(tree(), "default");
    s.select(PaneId(2));
    s.select_project(ProjectId(2));
    assert_eq!(s.shown.pane(), None, "a project not visited shows nothing, as before");
    s.select_project(ProjectId(1));
    assert_eq!(s.shown.pane(), Some(PaneId(2)));
}

#[test]
fn returns_to_the_split_grid_a_project_was_left_with() {
    let mut s = connect(tree(), "default");
    s.resize(Side::Workspace, 30, 100);
    s.select(PaneId(1));
    s.open_beside(PaneId(2));
    s.select_project(ProjectId(2));
    assert!(!s.shown.grid().holds(PaneId(1)) && !s.shown.grid().holds(PaneId(2)), "none of it stays behind");
    s.select_project(ProjectId(1));
    assert_eq!((s.shown.on(Side::Workspace), s.shown.pane()), (Some(PaneId(1)), Some(PaneId(2))));
    assert!(s.shown.grid().holds(PaneId(2)));
}

#[test]
fn returns_to_the_stage_and_repository_a_project_was_left_on() {
    let mut t = tree();
    t[0].repositories.push(repo(11, "docs", vec![]));
    let mut s = connect(t, "default");
    s.choose_repository(RepositoryId(11));
    s.go(Stage::Feature);
    s.select_project(ProjectId(2));
    s.go(Stage::Workspace);
    s.select_project(ProjectId(1));
    assert_eq!((s.stages.now(), s.repository), (Stage::Feature, Some(RepositoryId(11))));
}

#[test]
fn a_closed_pane_falls_back_to_its_checkout() {
    let mut s = connect(tree(), "default");
    s.select(PaneId(2));
    s.select_project(ProjectId(2));
    let mut grown = tree();
    grown[0].repositories[0].checkouts[0].panes = vec![shell(7)];
    s.receive(ServerMsg::Tree(grown));
    let sent = s.select_project(ProjectId(1));
    assert_eq!(s.shown.pane(), Some(PaneId(7)), "the checkout's next pane");
    assert!(!sent.iter().any(|m| matches!(m, ClientMsg::Subscribe { pane } if *pane == PaneId(2))), "nothing asked for the closed pane");
}

#[test]
fn a_project_whose_checkout_has_no_pane_left_opens_empty() {
    let mut s = connect(tree(), "default");
    s.select(PaneId(2));
    s.select_project(ProjectId(2));
    let mut grown = tree();
    grown[0].repositories[0].checkouts[0].panes.clear();
    s.receive(ServerMsg::Tree(grown));
    s.select_project(ProjectId(1));
    assert_eq!(s.shown.pane(), None);
}

#[test]
fn switching_workspace_opens_its_last_project() {
    let mut s = connect(tree(), "default");
    s.select_project(ProjectId(2));
    let other = vec![project(5, "alpha", vec![]), project(6, "beta", vec![repo(60, "web", vec![shell(9)])])];
    s.receive(ServerMsg::Tree(other.clone()));
    s.receive(workspaces("other", 2));
    assert_eq!(s.project, Some(ProjectId(5)), "a workspace not visited opens its first project");
    s.select_project(ProjectId(6));
    s.select(PaneId(9));

    s.receive(ServerMsg::Tree(tree()));
    s.receive(workspaces("default", 2));
    assert_eq!(s.project, Some(ProjectId(2)), "not the first project");

    s.receive(ServerMsg::Tree(other));
    s.receive(workspaces("other", 2));
    assert_eq!((s.project, s.shown.pane()), (Some(ProjectId(6)), Some(PaneId(9))));
}

#[test]
fn a_tree_that_arrives_before_its_workspace_is_listed_is_not_noted_under_the_old_one() {
    let mut s = connect(tree(), "default");
    s.select_project(ProjectId(2));
    s.receive(ServerMsg::Tree(vec![project(5, "alpha", vec![])]));
    s.receive(workspaces("other", 1));
    assert_eq!(s.places.project("", "default"), Some("work"));
    assert_eq!(s.places.project("", "other"), Some("alpha"));
}

#[test]
fn host_switch_back_restores_project() {
    let mut s = connect(tree(), "default");
    s.select_project(ProjectId(2));
    s.select(PaneId(3));
    s.switch_host(Some("box".into()));
    assert!(s.tree.is_empty());
    s.receive(ServerMsg::Tree(vec![project(8, "remote", vec![repo(80, "r", vec![shell(30)])])]));
    s.receive(workspaces("default", 1));
    assert_eq!(s.project, Some(ProjectId(8)));

    s.switch_host(None);
    s.receive(ServerMsg::Tree(tree()));
    s.receive(workspaces("default", 2));
    assert_eq!((s.project, s.shown.pane()), (Some(ProjectId(2)), Some(PaneId(3))));
}

#[test]
fn a_host_opens_the_workspace_it_was_last_in() {
    let mut s = connect(tree(), "other");
    s.receive(ServerMsg::Tree(vec![project(5, "alpha", vec![])]));
    s.receive(workspaces("other", 1));
    // The next run starts in the daemon's default again.
    let mut next = Session::new(s.take_settings().unwrap());
    let sent = next.connected(Some(Hello::this_build()), None, vec![ServerMsg::Tree(tree()), workspaces("default", 2)]);
    assert!(sent.iter().any(|m| matches!(m, ClientMsg::OpenWorkspace { workspace: WorkspaceId(2) })), "{sent:?}");
}

#[test]
fn the_places_survive_the_window_restarting() {
    let mut s = connect(tree(), "default");
    s.select(PaneId(2));
    s.go(Stage::Checkouts);
    s.select_project(ProjectId(2));
    let saved = serde_json::to_string(&s.take_settings().expect("a move is worth saving")).unwrap();
    let mut next = Session::new(serde_json::from_str(&saved).unwrap());
    next.connected(Some(Hello::this_build()), None, vec![ServerMsg::Tree(tree()), workspaces("default", 2)]);
    assert_eq!(next.project, Some(ProjectId(2)));
    next.select_project(ProjectId(1));
    assert_eq!((next.shown.pane(), next.stages.now()), (Some(PaneId(2)), Stage::Checkouts));
}
