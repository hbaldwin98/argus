//! Switching the open workspace and making new ones.

use super::*;
#[test]
fn the_open_workspace_is_remembered_from_the_daemons_list() {
    let mut h = Harness::new();
    h.app
        .on_server_msg(ServerMsg::Workspaces(workspaces("work")));
    assert_eq!(h.app.open_workspace, "work");
    assert_eq!(h.app.workspaces.len(), 3);
}

#[test]
fn w_opens_a_picker_positioned_on_the_workspace_already_open() {
    // "Look at where I am, then move" is the reason to press it, so
    // starting at the top of the list would be the wrong default.
    let mut h = Harness::new();
    h.app
        .on_server_msg(ServerMsg::Workspaces(workspaces("work")));
    h.key(KeyCode::Char('w'));

    let picker = h.app.picker.as_ref().expect("w should open the picker");
    assert_eq!(picker.sel, 1, "starts on the open one");
    assert!(picker.items[1].starts_with("work"));
}

#[test]
fn choosing_a_workspace_asks_the_daemon_to_switch() {
    let mut h = Harness::new();
    h.app
        .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
    h.key(KeyCode::Char('w'));
    h.key(KeyCode::Down);
    h.key(KeyCode::Enter);

    match &h.sent()[0] {
        ClientMsg::OpenWorkspace { workspace } => {
            assert_eq!(*workspace, argus_protocol::WorkspaceId(2), "the 'work' row");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(h.app.picker.is_none());
}

#[test]
fn switching_workspace_resets_navigation_to_the_top() {
    // The incoming tree is a different set of projects; keeping an index
    // that meant something else would land the user somewhere arbitrary.
    let mut h = Harness::new();
    h.app
        .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
    h.keys("lllj"); // wander into the pane column
    h.sent();

    h.key(KeyCode::Char('w'));
    h.key(KeyCode::Down);
    h.key(KeyCode::Enter);

    assert_eq!(h.app.focus, Focus::Projects);
    assert_eq!(
        (h.app.sel_project, h.app.sel_checkout, h.app.sel_pane),
        (0, 0, 0)
    );
}

#[test]
fn escaping_the_workspace_picker_switches_nothing() {
    let mut h = Harness::new();
    h.app
        .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
    h.key(KeyCode::Char('w'));
    h.key(KeyCode::Down);
    h.key(KeyCode::Esc);
    assert!(h.app.picker.is_none());
    assert!(h.sent().is_empty());
}

#[test]
fn w_still_opens_on_a_lone_workspace_because_that_is_where_a_second_comes_from() {
    // The zero-config case, and the one that used to be a dead end:
    // with no way to name a workspace here, an install stayed at one
    // forever unless the user hand-edited projects.toml.
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Workspaces(only_default()));
    h.key(KeyCode::Char('w'));
    assert!(h.app.picker.is_some());
}

#[test]
fn a_query_naming_no_workspace_offers_to_create_it() {
    let mut h = Harness::new();
    h.app
        .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
    h.key(KeyCode::Char('w'));
    h.app.picker.as_mut().unwrap().type_query("weekday");

    let p = h.app.picker.as_ref().unwrap();
    assert_eq!(p.create.as_deref(), Some("weekday"));
}

#[test]
fn a_query_naming_a_workspace_that_exists_does_not_offer_to_create_it() {
    // Two ways to reach the same workspace, one of which would fail on
    // the daemon, is worse than one.
    let mut h = Harness::new();
    h.app
        .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
    h.key(KeyCode::Char('w'));
    h.app.picker.as_mut().unwrap().type_query("weekend");
    assert_eq!(h.app.picker.as_ref().unwrap().create, None);
}

#[test]
fn workspace_rows_are_matched_on_their_names_not_their_counts() {
    // The rows carry "2\u{25a3}"; typing a digit must not "find" a
    // workspace by how many panes it happens to be running.
    let mut h = Harness::new();
    h.app
        .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
    h.key(KeyCode::Char('w'));
    h.app.picker.as_mut().unwrap().type_query("2");
    let p = h.app.picker.as_ref().unwrap();
    assert!(p.shown.is_empty(), "no workspace is named 2: {:?}", p.shown);
    assert_eq!(
        p.create.as_deref(),
        Some("2"),
        "it is a name to make instead"
    );
}

#[test]
fn choosing_the_create_row_makes_the_workspace() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Workspaces(only_default()));
    h.key(KeyCode::Char('w'));
    h.keys("side");
    h.key(KeyCode::Down); // past the (now empty) matches, onto create
    h.key(KeyCode::Enter);

    match &h.sent()[0] {
        ClientMsg::CreateWorkspace { name } => assert_eq!(name, "side"),
        other => panic!("unexpected {other:?}"),
    }
    assert!(h.app.picker.is_none());
}

#[test]
fn a_created_workspace_arrives_empty_so_navigation_starts_over() {
    // The daemon opens what it creates, and it has no projects; leaving
    // the columns pointed into the old workspace would be a selection
    // into a tree that is gone.
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Workspaces(only_default()));
    h.keys("lllj");
    h.sent();

    h.key(KeyCode::Char('w'));
    h.keys("side");
    h.key(KeyCode::Down);
    h.key(KeyCode::Enter);

    assert_eq!(h.app.focus, Focus::Projects);
    assert_eq!(
        (h.app.sel_project, h.app.sel_checkout, h.app.sel_pane),
        (0, 0, 0)
    );
}

#[test]
fn the_top_row_still_switches_rather_than_creating() {
    // The create row sits below the matches; enter on a match is a
    // switch, exactly as it was before the row existed.
    let mut h = Harness::new();
    h.app
        .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
    h.key(KeyCode::Char('w'));
    h.keys("week");
    h.key(KeyCode::Enter);

    match &h.sent()[0] {
        ClientMsg::OpenWorkspace { workspace } => {
            assert_eq!(
                *workspace,
                argus_protocol::WorkspaceId(3),
                "the 'weekend' row"
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn the_picker_shows_how_much_is_running_in_each_workspace() {
    // The reason to surface counts at all: an agent working somewhere
    // you are not looking should still be visible.
    let mut h = Harness::new();
    h.app
        .on_server_msg(ServerMsg::Workspaces(workspaces("default")));
    h.key(KeyCode::Char('w'));
    let items = &h.app.picker.as_ref().unwrap().items;
    assert!(
        items[2].contains("2▣"),
        "weekend has two live panes: {items:?}"
    );
    assert!(
        !items[0].contains('▣'),
        "an idle workspace stays quiet: {items:?}"
    );
}

#[test]
fn the_agent_picker_still_spawns_after_the_picker_grew_a_second_use() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('a'));
    h.key(KeyCode::Enter);
    assert!(matches!(h.sent()[0], ClientMsg::SpawnAgent { .. }));
}
