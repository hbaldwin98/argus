//! The floating window, and getting back out of it.

use super::*;
// --- floating windows ---------------------------------------------------

#[test]
fn a_floating_pane_streams_alongside_the_column_not_instead_of_it() {
    // Opening a file must not cost you sight of the agent behind it.
    let mut h = Harness::new();
    h.keys("lll"); // watching the column's pane
    h.sent();
    let column = h.app.column_pane().unwrap();

    h.app
        .open_overlay_pane(PaneId(700), "vim".to_string(), false);

    assert_eq!(h.app.overlay_pane(), Some(PaneId(700)));
    assert_eq!(h.app.column_pane(), Some(column), "the column is untouched");
    assert!(h.app.grids.contains_key(&column), "and still streaming");
    assert_eq!(h.app.focus, Focus::Overlay);
    assert!(matches!(
        h.sent().last(),
        Some(ClientMsg::Subscribe { pane: PaneId(700) })
    ));
}

#[test]
fn a_floating_pane_restores_the_columns_before_taking_focus() {
    let mut h = Harness::new();
    h.keys("llll");
    h.leader();
    h.key(KeyCode::Char('f'));

    h.app
        .open_overlay_pane(PaneId(700), "vim".to_string(), false);

    assert_eq!(h.app.focus, Focus::Overlay);
    assert!(!h.app.pane_fullscreen);
}

#[test]
fn typing_in_a_floating_pane_reaches_its_child() {
    let mut h = Harness::new();
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);
    h.sent();

    h.keys("iabc");

    let typed: Vec<u8> = h
        .sent()
        .into_iter()
        .filter_map(|m| match m {
            ClientMsg::Input {
                pane: PaneId(101),
                bytes,
            } => Some(bytes),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(typed, b"iabc");
}

#[test]
fn nav_keys_do_not_leak_out_of_a_floating_pane() {
    // Every key belongs to the editor while it is up — `q` especially.
    let mut h = Harness::new();
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);
    h.keys("q");
    assert!(!h.app.should_quit, "q is the editor's, not ours");
    assert!(h.app.overlay.is_some());
}

#[test]
fn the_leader_closes_a_floating_pane_and_leaves_it_running() {
    let mut h = Harness::new();
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);
    h.sent();

    h.leader();
    h.key(KeyCode::Esc);

    assert!(h.app.overlay.is_none());
    assert!(
        !h.sent().iter().any(|m| matches!(m, ClientMsg::Kill { .. })),
        "closing the window must not kill the editor"
    );
}

#[test]
fn the_leader_can_also_kill_the_pane_in_a_floating_window() {
    let mut h = Harness::new();
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);
    h.sent();

    h.leader();
    h.key(KeyCode::Char('x'));

    assert!(h
        .sent()
        .iter()
        .any(|m| matches!(m, ClientMsg::Kill { pane } if *pane == PaneId(101))));
    assert!(h.app.overlay.is_none());
}

#[test]
fn closing_a_floating_pane_puts_the_live_view_back_on_the_column() {
    let mut h = Harness::new();
    h.keys("lll");
    h.sent();
    let was = h.app.column_pane();

    h.app
        .open_overlay_pane(PaneId(999), "vim".to_string(), false);
    h.leader();
    h.key(KeyCode::Esc);

    assert_eq!(h.app.column_pane(), was, "back to what the columns show");
    assert!(
        !h.app.grids.contains_key(&PaneId(999)),
        "and the editor is dropped"
    );
}

#[test]
fn a_floating_pane_and_the_column_are_sized_separately() {
    let mut h = Harness::new();
    laid_out(&mut h);
    assert_eq!(h.app.live_panes()[0].1, h.app.layout.content.inner);

    h.app
        .open_overlay_pane(PaneId(700), "vim".to_string(), false);
    h.app.layout.overlay = Panel {
        outer: Rect::new(2, 1, 40, 20),
        inner: Rect::new(3, 2, 38, 18),
        first: 0,
    };
    let live = h.app.live_panes();
    assert_eq!(live.len(), 2);
    assert_eq!(live[1].1, h.app.layout.overlay.inner);
}

// --- getting out of a floating window -----------------------------------

#[test]
fn f12_closes_a_floating_window_from_anywhere() {
    // The leader depends on the terminal delivering Ctrl-Space, and a
    // floating pane swallows every other key on purpose. When both fail
    // there has to be something left to press.
    let mut h = Harness::new();
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);
    h.sent();

    h.key(KeyCode::F(12));

    assert!(h.app.overlay.is_none());
    assert!(
        !h.sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Input { .. })),
        "and it is never forwarded to the child"
    );
}

#[test]
fn f12_is_harmless_when_no_window_is_open() {
    let mut h = Harness::new();
    h.key(KeyCode::F(12));
    assert!(!h.app.should_quit);
    assert!(h.app.overlay.is_none());
}

#[test]
fn clicking_outside_a_floating_window_dismisses_it() {
    let mut h = Harness::new();
    laid_out(&mut h);
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);
    h.app.layout.overlay = Panel {
        outer: Rect::new(10, 4, 20, 10),
        inner: Rect::new(11, 5, 18, 8),
        first: 0,
    };

    h.app.on_mouse(click(1, 1)); // out on the projects column

    assert!(h.app.overlay.is_none());
}

#[test]
fn a_click_inside_the_window_belongs_to_its_pane() {
    let mut h = Harness::new();
    laid_out(&mut h);
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);
    h.app.layout.overlay = Panel {
        outer: Rect::new(10, 4, 20, 10),
        inner: Rect::new(11, 5, 18, 8),
        first: 0,
    };
    wants_mouse(&mut h, PaneId(101));
    h.sent();

    h.app.on_mouse(click(15, 7));

    assert!(h.app.overlay.is_some(), "still open");
    assert!(h
        .sent()
        .iter()
        .any(|m| matches!(m, ClientMsg::Input { .. })));
}

#[test]
fn a_click_under_a_floating_window_never_reaches_the_columns() {
    // The bug this exists for: focus moved to a column while the keys
    // still went to the overlay, leaving no way in and no way out.
    let mut h = Harness::new();
    laid_out(&mut h);
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);
    h.app.layout.overlay = Panel {
        outer: Rect::new(10, 4, 20, 10),
        inner: Rect::new(11, 5, 18, 8),
        first: 0,
    };
    let before = h.app.sel_project;

    h.app.on_mouse(click(1, 3)); // a project row, underneath

    assert_eq!(
        h.app.sel_project, before,
        "the click dismissed, it did not select"
    );
}

#[test]
fn a_window_whose_pane_exits_closes_itself() {
    // Otherwise it sits there showing a dead grid — the shape of a hung
    // editor, with no sign anything is wrong.
    let mut h = Harness::new();
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);

    h.app.on_server_msg(ServerMsg::PaneClosed {
        pane: PaneId(101),
        code: Some(0),
    });

    assert!(h.app.overlay.is_none());
}

#[test]
fn another_panes_exit_leaves_the_window_alone() {
    let mut h = Harness::new();
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);
    h.app.on_server_msg(ServerMsg::PaneClosed {
        pane: PaneId(100),
        code: Some(0),
    });
    assert!(h.app.overlay.is_some());
}

#[test]
fn a_window_whose_pane_vanishes_from_the_tree_closes_itself() {
    // Killed from another client, or reaped while we were not looking.
    let mut h = Harness::new();
    h.app
        .open_overlay_pane(PaneId(101), "vim".to_string(), false);

    let mut t = tree();
    t[0].repositories[0].checkouts[0]
        .panes
        .retain(|p| p.id != PaneId(101));
    h.app.on_server_msg(ServerMsg::Tree(t));

    assert!(h.app.overlay.is_none());
}
