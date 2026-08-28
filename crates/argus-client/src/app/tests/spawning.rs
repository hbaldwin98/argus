//! Starting and closing shells and agents.

use super::*;
// --- spawning ----------------------------------------------------------

#[test]
fn s_spawns_a_shell_in_the_selected_checkout_and_focuses_it() {
    let mut h = Harness::new();
    h.keys("llj"); // the linked worktree, which has no panes
    h.sent();
    h.key(KeyCode::Char('s'));
    assert!(
        matches!(
            h.sent()[0],
            ClientMsg::SpawnShell {
                checkout: CheckoutId(11)
            }
        ),
        "spawns into the selected checkout"
    );

    // The daemon's next tree carries the new pane.
    let mut t = tree();
    t[0].repositories[0].checkouts[1]
        .panes
        .push(pane(102, "shell"));
    h.app.on_server_msg(ServerMsg::Tree(t));
    assert_eq!(h.app.sel_pane, 0);
    assert_eq!(
        h.app.focus,
        Focus::PaneContent,
        "drops you straight into it"
    );
    assert_eq!(h.app.column_pane(), Some(PaneId(102)));
}

#[test]
fn a_spawn_focuses_the_newest_pane_not_the_first() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('s'));
    h.sent();
    let mut t = tree();
    t[0].repositories[0].checkouts[0]
        .panes
        .push(pane(102, "shell"));
    h.app.on_server_msg(ServerMsg::Tree(t));
    assert_eq!(h.app.column_pane(), Some(PaneId(102)));
}

#[test]
fn a_selected_pane_is_followed_when_it_moves_to_another_checkout() {
    let mut h = Harness::new();
    h.app.focus = Focus::PaneContent;
    h.app.sel_pane = 1;
    assert_eq!(h.app.column_pane(), Some(PaneId(101)));

    let mut moved = tree();
    let pane = moved[0].repositories[0].checkouts[0].panes.remove(1);
    moved[0].repositories[0].checkouts[1].panes.push(pane);
    h.app.on_server_msg(ServerMsg::Tree(moved));

    assert_eq!(h.app.sel_checkout, 1);
    assert_eq!(h.app.column_pane(), Some(PaneId(101)));
    assert_eq!(h.app.focus, Focus::PaneContent);
}

#[test]
fn a_selected_pane_is_followed_when_it_moves_to_another_repository() {
    let mut h = Harness::new();
    h.app.focus = Focus::PaneContent;
    h.app.sel_pane = 1;

    let mut moved = tree();
    let pane = moved[0].repositories[0].checkouts[0].panes.remove(1);
    moved[0].repositories.push(repository(
        7,
        "satellite",
        vec![checkout(30, "main", true, vec![pane])],
    ));
    h.app.on_server_msg(ServerMsg::Tree(moved));

    assert_eq!(h.app.sel_repository, 1);
    assert_eq!(h.app.current_repository().unwrap().name, "satellite");
    assert_eq!(h.app.column_pane(), Some(PaneId(101)));
    assert_eq!(h.app.focus, Focus::PaneContent);
}

#[test]
fn a_background_pane_move_does_not_hijack_project_navigation() {
    let mut h = Harness::new();
    h.app.focus = Focus::Projects;

    let mut moved = tree();
    let pane = moved[0].repositories[0].checkouts[0].panes.remove(0);
    moved[0].repositories[0].checkouts[1].panes.push(pane);
    h.app.on_server_msg(ServerMsg::Tree(moved));

    assert_eq!(h.app.sel_checkout, 0);
    assert_eq!(h.app.focus, Focus::Projects);
}

#[test]
fn a_picks_an_agent_template_and_spawns_it() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('a'));
    assert!(h.app.picker.is_some());
    h.key(KeyCode::Char('j'));
    assert_eq!(h.app.picker.as_ref().unwrap().sel, 1);
    h.key(KeyCode::Enter);
    assert!(h.app.picker.is_none());
    match &h.sent()[0] {
        ClientMsg::SpawnAgent { checkout, template } => {
            assert_eq!(*checkout, CheckoutId(10));
            assert_eq!(template, "codex");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn esc_cancels_the_agent_picker_without_spawning() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('a'));
    h.key(KeyCode::Esc);
    assert!(h.app.picker.is_none());
    assert!(h.sent().is_empty());
}

#[test]
fn the_picker_selection_does_not_run_past_the_ends() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('a'));
    h.keys("jjj");
    assert_eq!(h.app.picker.as_ref().unwrap().sel, 1, "two templates");
    h.keys("kkk");
    assert_eq!(h.app.picker.as_ref().unwrap().sel, 0);
}

#[test]
fn the_picker_swallows_navigation_keys() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('a'));
    h.keys("ll");
    assert_eq!(
        h.app.focus,
        Focus::Projects,
        "column focus must not move behind the modal"
    );
}

// --- closing panes ------------------------------------------------------

#[test]
fn x_closes_the_selected_pane_from_the_panes_column() {
    let mut h = Harness::new();
    h.keys("lll");
    h.sent();
    h.key(KeyCode::Char('x'));
    assert!(matches!(h.sent()[0], ClientMsg::Kill { pane: PaneId(100) }));
}

#[test]
fn x_does_nothing_from_the_other_columns() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('x'));
    h.key(KeyCode::Char('l'));
    h.sent();
    h.key(KeyCode::Char('x'));
    assert!(h.sent().is_empty(), "x is not a global delete");
}
