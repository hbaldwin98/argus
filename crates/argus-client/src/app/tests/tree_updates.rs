//! What a tree arriving from the daemon does to the selection.

use super::*;
// --- tree updates -------------------------------------------------------

#[test]
fn a_shrinking_tree_clamps_the_selection_instead_of_dangling() {
    let mut h = Harness::new();
    h.keys("lllj"); // second pane of the first checkout
    h.sent();
    let mut t = tree();
    t[0].repositories[0].checkouts[0].panes.pop();
    h.app.on_server_msg(ServerMsg::Tree(t));
    assert_eq!(h.app.sel_pane, 0);
    assert_eq!(h.app.column_pane(), Some(PaneId(100)));
}

#[test]
fn a_fullscreen_pane_that_vanishes_restores_the_pane_list() {
    let mut h = Harness::new();
    h.keys("llll");
    h.leader();
    h.key(KeyCode::Char('f'));
    let mut t = tree();
    t[0].repositories[0].checkouts[0].panes.remove(0);

    h.app.on_server_msg(ServerMsg::Tree(t));

    assert_eq!(h.app.focus, Focus::Panes);
    assert!(!h.app.pane_fullscreen);
    assert_eq!(h.app.column_pane(), Some(PaneId(101)));
}

#[test]
fn an_empty_tree_leaves_nothing_selected_and_nothing_subscribed() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Tree(Vec::new()));
    assert!(h.app.current_project().is_none());
    assert_eq!(h.app.sel_project, 0);
    assert_eq!(h.app.column_pane(), None);
}

#[test]
fn templates_arrive_out_of_band() {
    let (tx, _rx) = unbounded_channel();
    let mut app = App::new(tx);
    assert!(app.templates.is_empty());
    app.on_server_msg(ServerMsg::Templates(vec!["claude".to_string()]));
    assert_eq!(app.templates, vec!["claude"]);
}

#[test]
fn a_picker_will_not_open_with_no_templates() {
    let (tx, _rx) = unbounded_channel();
    let mut app = App::new(tx);
    app.on_server_msg(ServerMsg::Tree(tree()));
    app.on_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(app.picker.is_none());
}

#[test]
fn a_pane_exit_is_reported_in_the_status_line() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::PaneClosed {
        pane: PaneId(100),
        code: Some(1),
    });
    assert_eq!(
        h.app.status, "pane exited with code 1",
        "the bar is prose, not a Debug dump"
    );
    assert!(
        h.app.status_alert,
        "a failed exit is the thing you have to read"
    );
}

#[test]
fn a_fullscreen_pane_exit_restores_the_pane_list() {
    let mut h = Harness::new();
    h.keys("llll");
    h.leader();
    h.key(KeyCode::Char('f'));

    h.app.on_server_msg(ServerMsg::PaneClosed {
        pane: PaneId(100),
        code: Some(0),
    });

    assert_eq!(h.app.focus, Focus::Panes);
    assert!(!h.app.pane_fullscreen);
}

#[test]
fn a_clean_exit_is_news_but_a_kill_is_an_alarm() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::PaneClosed {
        pane: PaneId(100),
        code: Some(0),
    });
    assert_eq!(h.app.status, "pane exited");
    assert!(
        !h.app.status_alert,
        "a clean exit is no louder here than the ✓ on its row"
    );

    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::PaneClosed {
        pane: PaneId(100),
        code: None,
    });
    assert_eq!(h.app.status, "pane was killed");
    assert!(h.app.status_alert);
}

#[test]
fn a_daemon_error_is_surfaced_not_swallowed() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Error {
        message: "git worktree add failed".to_string(),
    });
    assert!(h.app.status.contains("git worktree add failed"));
}

#[test]
fn a_message_gives_the_bar_back_on_the_next_keypress() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Error {
        message: "git worktree add failed".to_string(),
    });
    h.key(KeyCode::Char('j'));
    assert!(
        h.app.status.is_empty(),
        "an unread error would hide the breadcrumb forever: {}",
        h.app.status
    );
}

#[test]
fn a_click_acknowledges_a_message_but_a_mouse_move_does_not() {
    let mut h = Harness::new();
    laid_out(&mut h);
    h.app.on_server_msg(ServerMsg::PaneClosed {
        pane: PaneId(100),
        code: Some(1),
    });
    h.app.on_mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: 2,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });
    assert!(
        !h.app.status.is_empty(),
        "drifting across the terminal is not reading it"
    );

    h.app.on_mouse(click(2, 3));
    assert!(h.app.status.is_empty(), "{}", h.app.status);
}

#[test]
fn git_status_rides_along_on_checkout_rows() {
    let mut h = Harness::new();
    let mut t = tree();
    t[0].repositories[0].checkouts[0].git = Some(GitStatus {
        branch: Some("master".to_string()),
        dirty: true,
        changed_files: 2,
        ahead: 1,
        behind: 0,
    });
    h.app.on_server_msg(ServerMsg::Tree(t));
    let g = h.app.current_checkout().unwrap().git.as_ref().unwrap();
    assert_eq!(g.branch.as_deref(), Some("master"));
    assert_eq!(g.changed_files, 2);
}

#[test]
fn q_detaches_from_the_nav_columns() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('q'));
    assert!(h.app.should_quit);
}
