//! Where a file opens, and what an editor pane is not.

use super::*;
#[test]
fn an_editor_opens_in_a_floating_window_by_default() {
    let mut h = Harness::new();
    open_editor_from_review(&mut h);
    assert!(matches!(h.app.overlay, Some(Overlay::Pane { pane, .. }) if pane == PaneId(700)));
}

#[test]
fn the_column_setting_keeps_the_editor_in_the_column() {
    let mut h = Harness::new();
    h.app.settings.editor = crate::settings::EditorMode::Column;
    open_editor_from_review(&mut h);

    assert!(h.app.overlay.is_none());
    assert_eq!(h.app.focus, Focus::PaneContent);
}

#[test]
fn an_external_editor_asks_the_daemon_not_to_make_a_pane() {
    let mut h = Harness::new();
    h.app.settings.editor = crate::settings::EditorMode::External;
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));
    h.key(KeyCode::Char('e'));

    match &h.sent()[0] {
        ClientMsg::OpenInEditor { external, .. } => assert!(*external),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn an_external_editor_does_not_steal_focus_when_the_tree_changes() {
    // It has no pane to focus, and grabbing the newest one would land
    // the user somewhere arbitrary.
    let mut h = Harness::new();
    h.app.settings.editor = crate::settings::EditorMode::External;
    open_editor_from_review(&mut h);

    assert!(h.app.overlay.is_none());
    assert_ne!(h.app.focus, Focus::PaneContent);
}

#[test]
fn the_editor_command_is_typed_rather_than_cycled() {
    let mut h = Harness::new();
    settings_row(&mut h, crate::app::Setting::EditorCmd);
    h.key(KeyCode::Char('l'));

    assert!(
        matches!(h.app.prompt, Some(Prompt::EditorCommand { .. })),
        "free text needs a field, not a carousel"
    );
}

#[test]
fn typing_a_command_stores_it() {
    let mut h = Harness::new();
    settings_row(&mut h, crate::app::Setting::EditorCmd);
    h.key(KeyCode::Enter);
    h.keys("nvim -p");
    h.key(KeyCode::Enter);

    assert_eq!(h.app.settings.editor_cmd, "nvim -p");
    assert!(h.app.prompt.is_none());
}

#[test]
fn the_prompt_starts_from_the_command_already_set() {
    // Retyping a long path to change one flag would be miserable.
    let mut h = Harness::new();
    h.app.settings.editor_cmd = "code -w".to_string();
    settings_row(&mut h, crate::app::Setting::EditorCmd);
    h.key(KeyCode::Enter);

    match &h.app.prompt {
        Some(Prompt::EditorCommand { input }) => assert_eq!(input, "code -w"),
        other => panic!("unexpected {other:?}", other = other.is_some()),
    }
}

#[test]
fn clearing_the_command_goes_back_to_the_environment() {
    let mut h = Harness::new();
    h.app.settings.editor_cmd = "nvim".to_string();
    settings_row(&mut h, crate::app::Setting::EditorCmd);
    h.key(KeyCode::Enter);
    for _ in 0..4 {
        h.key(KeyCode::Backspace);
    }
    h.key(KeyCode::Enter);

    assert!(h.app.settings.editor_cmd.is_empty());
}

#[test]
fn escaping_the_command_prompt_changes_nothing() {
    let mut h = Harness::new();
    h.app.settings.editor_cmd = "nvim".to_string();
    settings_row(&mut h, crate::app::Setting::EditorCmd);
    h.key(KeyCode::Enter);
    h.keys("zzz");
    h.key(KeyCode::Esc);

    assert_eq!(h.app.settings.editor_cmd, "nvim");
}

#[test]
fn the_chosen_command_is_sent_with_the_request() {
    let mut h = Harness::new();
    h.app.settings.editor_cmd = "hx".to_string();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));
    h.key(KeyCode::Char('e'));

    match &h.sent()[0] {
        ClientMsg::OpenInEditor { command, .. } => {
            assert_eq!(command.as_deref(), Some("hx"))
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn no_command_leaves_the_choice_to_the_daemon() {
    // The daemon can see $VISUAL and what is installed; the client
    // guessing would only be worse.
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));
    h.key(KeyCode::Char('e'));

    match &h.sent()[0] {
        ClientMsg::OpenInEditor { command, .. } => assert_eq!(*command, None),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn opening_an_editor_does_not_move_the_pane_selection() {
    // The agent you were watching stays selected and stays on screen;
    // the editor is a window over the top, not a replacement for it.
    let mut h = Harness::new();
    h.keys("lll"); // sitting on the agent in the panes column
    h.sent();
    let watching = h.app.column_pane();
    let where_ = h.app.sel_pane;

    editor_arrives(&mut h);

    assert_eq!(h.app.sel_pane, where_, "selection untouched");
    assert_eq!(h.app.column_pane(), watching, "column still on the agent");
    assert_eq!(
        h.app.overlay_pane(),
        Some(PaneId(700)),
        "editor is the window"
    );
    assert!(
        h.app.grids.contains_key(&watching.unwrap()),
        "and the agent is still streaming"
    );
}

#[test]
fn closing_the_editor_leaves_you_back_on_the_agent() {
    let mut h = Harness::new();
    h.keys("lll");
    h.sent();
    let watching = h.app.column_pane();

    editor_arrives(&mut h);
    h.key(KeyCode::F(12));

    assert_eq!(h.app.column_pane(), watching);
    assert!(h.app.overlay.is_none());
}

#[test]
fn an_editor_is_not_listed_among_the_panes() {
    // It is a way of looking at a file, not something running here that
    // you would come back to.
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Tree(tree_with_editor()));

    let listed: Vec<PaneId> = h.app.tree[0].repositories[0].checkouts[0]
        .listed_panes()
        .map(|p| p.id)
        .collect();
    assert!(!listed.contains(&PaneId(700)), "{listed:?}");
    assert_eq!(listed.len(), 2, "the shell and the agent remain");
}

#[test]
fn navigation_skips_over_editors() {
    // Otherwise j/k walks onto a row nothing draws.
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Tree(tree_with_editor()));
    h.keys("lll");
    for _ in 0..5 {
        h.key(KeyCode::Char('j'));
    }
    assert_eq!(h.app.sel_pane, 1, "clamped to the last listed pane");
    assert_ne!(h.app.column_pane(), Some(PaneId(700)));
}

#[test]
fn an_editor_does_not_inflate_the_pane_counts() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Tree(tree_with_editor()));
    assert_eq!(
        h.app.tree[0].repositories[0].checkouts[0]
            .listed_panes()
            .count(),
        2
    );
}

#[test]
fn closing_an_editors_window_ends_the_editor() {
    // Nothing lists it afterwards, so a survivor would be a process
    // with no window and no way back to it.
    let mut h = Harness::new();
    h.app
        .open_overlay_pane(PaneId(700), "a.rs".to_string(), true);
    h.sent();

    h.key(KeyCode::F(12));

    assert!(h
        .sent()
        .iter()
        .any(|m| matches!(m, ClientMsg::Kill { pane } if *pane == PaneId(700))));
}

#[test]
fn closing_a_window_over_a_listed_pane_leaves_it_running() {
    // A shell or agent shown floating is still in the panes column, so
    // closing the window is only ever "stop looking at it".
    let mut h = Harness::new();
    h.app
        .open_overlay_pane(PaneId(101), "shell".to_string(), false);
    h.sent();

    h.key(KeyCode::F(12));

    assert!(!h.sent().iter().any(|m| matches!(m, ClientMsg::Kill { .. })));
}

#[test]
fn a_diff_opens_in_a_window_and_leaves_the_column_alone() {
    // Reading a diff should not cost you sight of the agent that
    // produced it.
    let mut h = Harness::new();
    h.keys("lll");
    h.sent();
    let watching = h.app.column_pane();

    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    h.app.review_for_test(checkout);
    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));

    assert!(matches!(h.app.overlay, Some(Overlay::Review)));
    assert_eq!(h.app.column_pane(), watching, "column untouched");
    assert!(
        h.app.grids.contains_key(&watching.unwrap()),
        "still streaming"
    );
}

#[test]
fn closing_a_diff_puts_you_back_on_the_checkout() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    h.app.review_for_test(checkout);
    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));

    h.key(KeyCode::Esc);

    assert!(h.app.overlay.is_none());
    assert!(h.app.review.is_none());
    assert_eq!(h.app.focus, Focus::Checkouts);
}

#[test]
fn f12_also_gets_you_out_of_a_diff() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    h.app.review_for_test(checkout);
    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));

    h.key(KeyCode::F(12));

    assert!(h.app.overlay.is_none());
    assert!(h.app.review.is_none(), "and the diff goes with it");
}

#[test]
fn s_flips_the_open_diff_between_split_and_unified() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    h.app.review_for_test(checkout);
    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
    assert!(!h.app.review_split, "unified to begin with");

    h.key(KeyCode::Char('s'));
    assert!(h.app.review.as_ref().unwrap().split, "the open view flips");
    assert!(h.app.settings.review_split, "and is remembered");
    assert!(h.app.status.contains("split"), "{}", h.app.status);

    h.key(KeyCode::Char('s'));
    assert!(!h.app.review.as_ref().unwrap().split);
    assert!(!h.app.settings.review_split);
}

#[test]
fn the_next_diff_opens_the_way_the_last_one_was_left() {
    // The same standing rule as the side toggle: a view is a setting,
    // not a per-visit choice.
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    h.app.review_for_test(checkout);
    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
    h.key(KeyCode::Char('s'));
    h.key(KeyCode::Esc);

    h.app.review_for_test(checkout);
    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
    assert!(h.app.review.as_ref().unwrap().split);
}
