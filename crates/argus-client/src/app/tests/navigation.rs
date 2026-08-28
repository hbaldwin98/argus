//! Moving between the columns, and what the selection drags along
//! with it.

use super::*;
// --- Miller-column navigation -----------------------------------------

#[test]
fn starts_focused_on_projects() {
    let h = Harness::new();
    assert_eq!(h.app.focus, Focus::Projects);
    assert_eq!(h.app.current_project().unwrap().name, "argus");
}

#[test]
fn l_descends_and_h_ascends_through_every_column() {
    let mut h = Harness::new();
    for expected in [
        Focus::Repositories,
        Focus::Checkouts,
        Focus::Panes,
        Focus::PaneContent,
    ] {
        h.key(KeyCode::Char('l'));
        assert_eq!(h.app.focus, expected);
    }
    // Leaving the innermost column needs the leader chord: a bare `h`
    // there is a character typed at the child, not a navigation key.
    h.leader();
    h.key(KeyCode::Esc);
    assert_eq!(h.app.focus, Focus::Panes);
    for expected in [Focus::Checkouts, Focus::Repositories, Focus::Projects] {
        h.key(KeyCode::Char('h'));
        assert_eq!(h.app.focus, expected);
    }
}

#[test]
fn ascending_past_projects_is_a_no_op() {
    let mut h = Harness::new();
    h.keys("hhh");
    assert_eq!(h.app.focus, Focus::Projects, "must not fall off the left");
}

#[test]
fn cannot_descend_into_a_checkout_with_no_panes() {
    let mut h = Harness::new();
    h.keys("llj"); // checkouts column, select the linked worktree
    assert_eq!(h.app.current_checkout().unwrap().name, "feat");
    h.keys("lll");
    assert_eq!(h.app.focus, Focus::Panes, "no pane to descend into");
}

#[test]
fn j_and_k_move_within_the_focused_column_only() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('j'));
    assert_eq!(h.app.sel_project, 1);
    assert_eq!(h.app.sel_checkout, 0, "other columns untouched");
    h.key(KeyCode::Char('k'));
    assert_eq!(h.app.sel_project, 0);
}

#[test]
fn selection_does_not_run_off_either_end() {
    let mut h = Harness::new();
    h.keys("kkk");
    assert_eq!(h.app.sel_project, 0);
    h.keys("jjjjj");
    assert_eq!(h.app.sel_project, 1, "clamped to the last project");
}

#[test]
fn descending_resets_the_child_columns_selection() {
    let mut h = Harness::new();
    h.keys("lllj"); // into panes, select the second pane
    assert_eq!(h.app.sel_pane, 1);
    h.keys("hhh"); // back to projects
    h.keys("lll"); // descend again
    assert_eq!(h.app.sel_pane, 0, "re-entering a column starts at the top");
}

#[test]
fn flat_view_moves_through_panes_across_checkout_and_project_boundaries() {
    let mut h = Harness::new();
    h.app.tree[0].repositories[0].checkouts[1]
        .panes
        .push(pane(102, "feature agent"));
    h.app.tree[1].repositories[0].checkouts[0]
        .panes
        .push(pane(200, "other agent"));
    h.keys("lllv");

    assert_eq!(h.app.settings.pane_view, crate::settings::PaneView::Flat);
    assert_eq!(h.app.column_pane(), Some(PaneId(100)));

    h.keys("jj");
    assert_eq!(h.app.column_pane(), Some(PaneId(102)));
    assert_eq!(
        h.app.current_checkout().map(|checkout| checkout.id),
        Some(CheckoutId(11))
    );

    h.key(KeyCode::Char('j'));
    assert_eq!(h.app.column_pane(), Some(PaneId(200)));
    assert_eq!(
        h.app.current_project().map(|project| project.id),
        Some(ProjectId(2))
    );

    h.key(KeyCode::Char('v'));
    assert_eq!(
        h.app.settings.pane_view,
        crate::settings::PaneView::Checkout
    );
    assert_eq!(h.app.pane_column_locations().len(), 1);
}

#[test]
fn moving_to_a_project_with_fewer_checkouts_clamps_the_selection() {
    let mut h = Harness::new();
    h.keys("llj"); // checkouts, index 1
    assert_eq!(h.app.sel_checkout, 1);
    h.app.sel_project = 1; // "other" has only one checkout
    h.key(KeyCode::Char('j'));
    assert_eq!(h.app.sel_checkout, 0, "clamped into range");
}

/// The bug this guards: a poll that has not cached the primary
/// checkout's branch yet leaves `master` looking like a branch nobody
/// is on, which pins a row above every checkout. With a bare index the
/// cursor slid up a row on each such tree until it sat on the pinned
/// main row, and the worktree could not be worked in at all.
#[test]
fn a_branch_row_appearing_above_the_selection_does_not_drag_it_off_the_worktree() {
    let mut h = Harness::new();
    h.keys("llj"); // checkouts column, the "feat" worktree
    assert_eq!(h.app.current_checkout().map(|c| c.id), Some(CheckoutId(11)));

    let mut t = tree();
    let r = &mut t[0].repositories[0];
    r.default_branch = Some("master".to_string());
    // The status sweep has not landed, so master reads as unoccupied.
    r.branches = vec!["master".to_string()];
    h.app.on_server_msg(ServerMsg::Tree(t));

    assert_eq!(
        h.app.current_checkout().map(|c| c.id),
        Some(CheckoutId(11)),
        "the cursor must stay on the worktree it was on"
    );
}

/// The bug this guards: `sel_checkout` is a row in the drawn column,
/// but following the watched pane set it from the checkout's position
/// in `checkouts`. Those agree only while a checkout is sitting on the
/// main branch; once none is, the main branch takes a pinned row of its
/// own and every checkout is a row lower. Watching an agent then threw
/// the cursor onto that pinned row, where there is no checkout and so
/// no agent — and every later tree kept it there.
#[test]
fn watching_an_agent_off_the_main_branch_stays_on_its_checkout() {
    let mut h = Harness::new();
    h.keys("lll");
    assert_eq!(h.app.current_pane().map(|p| p.id), Some(PaneId(100)));

    let mut t = tree();
    let r = &mut t[0].repositories[0];
    // Both checkouts have been switched off the main branch, so it is a
    // branch nobody is on and is pinned above them.
    r.default_branch = Some("dev".to_string());
    r.branches = vec!["dev".to_string()];
    h.app.on_server_msg(ServerMsg::Tree(t));

    assert_eq!(h.app.sel_checkout, 1, "the pinned branch row sits above it");
    assert_eq!(
        h.app.current_pane().map(|p| p.id),
        Some(PaneId(100)),
        "the agent being watched must still be the selected pane"
    );
}

#[test]
fn n_lands_on_the_checkout_row_of_the_pane_that_needs_attention() {
    let mut h = Harness::new();
    let mut t = tree();
    let r = &mut t[0].repositories[0];
    r.default_branch = Some("dev".to_string());
    r.branches = vec!["dev".to_string()];
    r.checkouts[0].panes[1].status = PaneStatus::Waiting;
    h.app.on_server_msg(ServerMsg::Tree(t));

    h.key(KeyCode::Char('N'));

    assert_eq!(h.app.column_pane(), Some(PaneId(101)));
    assert_eq!(h.app.current_checkout().map(|c| c.id), Some(CheckoutId(10)));
}

#[test]
fn a_new_worktree_is_selected_by_its_row_not_its_index() {
    let mut h = Harness::new();
    let mut t = tree();
    let r = &mut t[0].repositories[0];
    r.default_branch = Some("dev".to_string());
    r.branches = vec!["dev".to_string()];
    r.checkouts.push(checkout(12, "spike", false, vec![]));
    h.app.pending_focus_new_checkout = Some(RepositoryId(5));
    h.app.on_server_msg(ServerMsg::Tree(t));

    assert_eq!(
        h.app.current_checkout().map(|c| c.id),
        Some(CheckoutId(12)),
        "the worktree just created is the row to land on"
    );
}

#[test]
fn the_checkout_selection_survives_a_checkout_added_above_it() {
    let mut h = Harness::new();
    h.keys("llj");
    assert_eq!(h.app.current_checkout().map(|c| c.id), Some(CheckoutId(11)));

    let mut t = tree();
    t[0].repositories[0]
        .checkouts
        .insert(0, checkout(12, "hotfix", false, vec![]));
    h.app.on_server_msg(ServerMsg::Tree(t));

    assert_eq!(
        h.app.current_checkout().map(|c| c.id),
        Some(CheckoutId(11)),
        "a new row above the cursor must not move it"
    );
}

/// A branch row is the offer of a checkout, so when one appears the
/// user is still on the same thing and should end up inside it.
#[test]
fn a_selected_branch_row_is_followed_into_the_checkout_that_takes_it() {
    let mut h = Harness::new();
    h.app.show_branches = true;
    let mut t = tree();
    t[0].repositories[0].branches = vec!["spike".to_string()];
    h.app.on_server_msg(ServerMsg::Tree(t));
    h.keys("ll");
    // Rows: master, feat, then the free "spike" branch.
    h.app.sel_checkout = 2;
    assert_eq!(h.app.current_branch_row(), Some("spike"));

    let mut t = tree();
    let mut c = checkout(13, "spike", false, vec![]);
    c.git = Some(argus_protocol::GitStatus {
        branch: Some("spike".to_string()),
        dirty: false,
        changed_files: 0,
        ahead: 0,
        behind: 0,
    });
    t[0].repositories[0].checkouts.push(c);
    h.app.on_server_msg(ServerMsg::Tree(t));

    assert_eq!(
        h.app.current_checkout().map(|c| c.id),
        Some(CheckoutId(13)),
        "the branch row became a checkout; follow it in"
    );
}

// --- live-view subscription -------------------------------------------

#[test]
fn the_live_view_subscribes_to_the_selected_pane_without_descending() {
    // The rightmost column always shows a pane; it never has to take
    // over the screen for content to be visible.
    let mut h = Harness::new();
    assert_eq!(
        h.app.column_pane(),
        Some(PaneId(100)),
        "first pane, from Projects focus"
    );
    assert!(h.app.grids.contains_key(&PaneId(100)));
    assert!(h.sent().is_empty());
}

#[test]
fn changing_pane_selection_unsubscribes_the_old_and_subscribes_the_new() {
    let mut h = Harness::new();
    h.keys("lllj");
    let msgs = h.sent();
    assert!(
        matches!(msgs[0], ClientMsg::Unsubscribe { pane: PaneId(100) }),
        "{msgs:?}"
    );
    assert!(
        matches!(msgs[1], ClientMsg::Subscribe { pane: PaneId(101) }),
        "{msgs:?}"
    );
    assert_eq!(h.app.column_pane(), Some(PaneId(101)));
    assert!(
        !h.app.grids.contains_key(&PaneId(100)),
        "the old grid is dropped"
    );
}

#[test]
fn selecting_a_paneless_checkout_unsubscribes_and_clears_the_grid() {
    let mut h = Harness::new();
    h.keys("llj");
    assert_eq!(h.app.column_pane(), None);
    assert!(h.app.grids.is_empty(), "stale content must not linger");
    assert!(matches!(h.sent()[0], ClientMsg::Unsubscribe { .. }));
}

#[test]
fn ascending_out_of_a_pane_keeps_it_subscribed() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.leader();
    h.key(KeyCode::Esc);
    assert_eq!(h.app.focus, Focus::Panes);
    assert_eq!(
        h.app.column_pane(),
        Some(PaneId(100)),
        "live view keeps showing it"
    );
    assert!(h.sent().is_empty(), "no resubscribe churn");
}

#[test]
fn damage_for_an_unsubscribed_pane_is_ignored() {
    let mut h = Harness::new();
    h.app.grids.insert(
        PaneId(100),
        crate::grid::Grid::new(vec![vec![Cell::default()]]),
    );
    h.app.on_server_msg(ServerMsg::Damage {
        mouse: Default::default(),
        alternate_screen: false,
        pane: PaneId(999),
        cursor: Default::default(),
        spans: vec![CellSpan {
            row: 0,
            col: 0,
            cells: vec![Cell {
                ch: "X".into(),
                ..Default::default()
            }],
        }],
    });
    assert_eq!(h.app.grids[&PaneId(100)].cells[0][0].ch, " ");
}

#[test]
fn a_snapshot_for_the_subscribed_pane_installs_the_grid() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::PaneSnapshot {
        mouse: Default::default(),
        alternate_screen: false,
        pane: PaneId(100),
        rows: 1,
        cols: 1,
        cells: vec![vec![Cell::default()]],
        cursor: argus_protocol::Cursor {
            row: 0,
            col: 0,
            visible: true,
            ..Default::default()
        },
    });
    assert!(h.app.grids.contains_key(&PaneId(100)));
}

#[test]
fn damage_carries_the_childs_alternate_screen() {
    let mut h = Harness::new();
    h.app.grids.insert(
        PaneId(100),
        crate::grid::Grid::new(vec![vec![Cell::default()]]),
    );
    h.app.on_server_msg(ServerMsg::Damage {
        mouse: Default::default(),
        alternate_screen: true,
        pane: PaneId(100),
        cursor: Default::default(),
        spans: vec![],
    });
    assert!(h.app.grids[&PaneId(100)].alternate_screen);
}
