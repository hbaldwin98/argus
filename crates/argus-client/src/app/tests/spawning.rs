//! Starting and closing shells and agents.

use super::*;
// --- spawning ----------------------------------------------------------

#[test]
fn s_spawns_a_shell_in_the_selected_checkout_and_focuses_it() {
    let mut h = Harness::new();
    h.checkouts_stage();
    h.key(KeyCode::Char('j')); // the linked worktree, which has no panes
    h.sent();
    h.key(KeyCode::Char('s'));
    let request = h.sent().remove(0);
    assert!(
        matches!(
            request,
            ClientMsg::SpawnShell {
                checkout: CheckoutId(11),
                ..
            }
        ),
        "spawns into the selected checkout"
    );

    // The daemon's next tree carries the new pane, and it says so.
    let mut t = tree();
    t[0].repositories[0].checkouts[1]
        .panes
        .push(pane(102, "shell"));
    h.app.on_server_msg(ServerMsg::Tree(t));
    h.answer(&request, argus_protocol::Created::Pane(PaneId(102)));
    assert_eq!(h.app.sel_pane, 0);
    assert_eq!(
        (h.app.view, h.app.focus),
        (View::Workspace, Focus::PaneContent),
        "drops you straight into it, on the stage that shows it"
    );
    assert_eq!(h.app.column_pane(), Some(PaneId(102)));
}

#[test]
fn the_pane_a_spawn_made_is_focused_whichever_arrives_first() {
    // The answer can beat the tree carrying the pane, or trail it.
    for answer_first in [true, false] {
        let mut h = Harness::new();
        h.key(KeyCode::Char('s'));
        let request = h.sent().remove(0);
        let mut t = tree();
        t[0].repositories[0].checkouts[0]
            .panes
            .push(pane(102, "shell"));
        if answer_first {
            h.answer(&request, argus_protocol::Created::Pane(PaneId(102)));
            assert_ne!(h.app.column_pane(), Some(PaneId(102)), "not in the tree yet");
            h.app.on_server_msg(ServerMsg::Tree(t));
        } else {
            h.app.on_server_msg(ServerMsg::Tree(t));
            h.answer(&request, argus_protocol::Created::Pane(PaneId(102)));
        }
        assert_eq!(h.app.column_pane(), Some(PaneId(102)), "answer first: {answer_first}");
        assert_eq!(h.app.focus, Focus::PaneContent);
    }
}

#[test]
fn a_tree_that_arrives_before_the_new_pane_does_not_take_the_keys() {
    // The bug this guards: the first tree after a spawn was taken to carry
    // the new pane, and its last pane — an agent already running — got
    // the keys. The daemon broadcasts trees for everything.
    let mut h = Harness::new();
    h.key(KeyCode::Char('s'));
    h.sent();
    let mut unrelated = tree();
    unrelated[0].repositories[0].checkouts[0].panes[1].status = PaneStatus::Waiting;
    h.app.on_server_msg(ServerMsg::Tree(unrelated));

    assert_ne!(h.app.focus, Focus::PaneContent, "nothing was made yet");
    assert_eq!(h.app.column_pane(), Some(PaneId(100)), "the selection stayed put");
}

#[test]
fn a_refused_spawn_leaves_nothing_waiting_for_a_later_pane() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('s'));
    let request = h.sent().remove(0);
    let ClientMsg::SpawnShell { request_id, .. } = request else {
        panic!("{request:?}");
    };
    h.app.on_server_msg(ServerMsg::Created {
        request_id,
        created: None,
    });
    // A pane someone else starts afterwards is not the one refused.
    let mut t = tree();
    t[0].repositories[0].checkouts[0]
        .panes
        .push(pane(102, "shell"));
    h.app.on_server_msg(ServerMsg::Tree(t));
    assert_ne!(h.app.focus, Focus::PaneContent);
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
        ClientMsg::SpawnAgent {
            checkout, template, ..
        } => {
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
