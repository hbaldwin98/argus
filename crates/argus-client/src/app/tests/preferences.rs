//! The settings panel.

use super::*;
// --- settings -----------------------------------------------------------

#[test]
fn shift_s_opens_the_settings_panel() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('S'));
    assert!(matches!(h.app.overlay, Some(Overlay::Settings { .. })));
    assert_eq!(h.app.focus, Focus::Overlay);
}

#[test]
fn h_and_l_change_the_setting_under_the_cursor() {
    let mut h = Harness::new();
    h.app.open_settings();
    let before = h.app.settings.editor;

    h.key(KeyCode::Char('l'));
    assert_eq!(h.app.settings.editor, before.next());

    h.key(KeyCode::Char('h'));
    assert_eq!(h.app.settings.editor, before, "and back again");
}

#[test]
fn changing_the_theme_applies_it_at_once() {
    // There is no save button, so there is nothing to forget to press.
    let mut h = Harness::new();
    h.app.open_settings();
    let theme_row = Setting::ALL
        .iter()
        .position(|setting| *setting == Setting::Theme)
        .unwrap();
    for _ in 0..theme_row {
        h.key(KeyCode::Char('j'));
    }
    h.key(KeyCode::Char('l'));

    assert_eq!(
        h.app.theme,
        crate::theme::Theme::by_name(&h.app.settings.theme)
    );
    assert_ne!(h.app.settings.theme, "mocha");
}

#[test]
fn the_initial_tree_is_a_quiet_baseline() {
    let mut h = Harness::new();

    assert!(h.app.next_flash_deadline().is_none());
    assert!(!h.app.take_bell());
    assert!(h.app.status.is_empty());
}

#[test]
fn every_state_has_an_accurate_notification_word() {
    let cases = [
        (PaneStatus::Idle, "idle"),
        (PaneStatus::Working, "working"),
        (PaneStatus::Waiting, "needs attention"),
        (PaneStatus::NeedsReview, "needs review"),
        (PaneStatus::Done, "done"),
        (PaneStatus::Failed, "failed"),
        (PaneStatus::Exited { code: Some(0) }, "exited"),
        (
            PaneStatus::Exited { code: Some(1) },
            "exited unsuccessfully",
        ),
        (PaneStatus::Exited { code: None }, "exited unsuccessfully"),
    ];

    for (status, word) in cases {
        assert_eq!(state_word(status), word, "for {status:?}");
    }
}

#[test]
fn an_actionable_transition_flashes_and_explains_the_pane() {
    let mut h = Harness::new();
    let mut next = tree();
    let pane = &mut next[0].repositories[0].checkouts[0].panes[1];
    pane.status = PaneStatus::Waiting;
    pane.note = Some("needs the staging password".to_string());

    h.app.on_server_msg(ServerMsg::Tree(next));

    assert!(h.app.pane_is_flashing(PaneId(101)));
    assert!(h.app.status.contains("claude: needs the staging password"));
    assert!(h.app.status_alert);
    let deadline = h.app.next_flash_deadline().unwrap();
    h.app.expire_state_flashes(deadline);
    assert!(!h.app.pane_is_flashing(PaneId(101)));
}

#[test]
fn the_bell_is_opt_in_and_only_consumed_once() {
    let mut h = Harness::new();
    h.app.settings.notifications = crate::settings::NotificationMode::Bell;
    let mut next = tree();
    next[0].repositories[0].checkouts[0].panes[1].status = PaneStatus::NeedsReview;

    h.app.on_server_msg(ServerMsg::Tree(next));

    assert!(h.app.take_bell());
    assert!(!h.app.take_bell());
}

#[test]
fn a_child_transition_flashes_and_names_its_parent() {
    let mut h = Harness::new();
    let mut working = tree();
    let pane = &mut working[0].repositories[0].checkouts[0].panes[1];
    pane.status = PaneStatus::Working;
    pane.children.push(argus_protocol::ChildAgentInfo {
        label: "test runner".to_string(),
        status: PaneStatus::Working,
        note: None,
    });
    h.app.on_server_msg(ServerMsg::Tree(working.clone()));
    h.app
        .expire_state_flashes(std::time::Instant::now() + STATE_FLASH);
    working[0].repositories[0].checkouts[0].panes[1].children[0].status = PaneStatus::Failed;
    working[0].repositories[0].checkouts[0].panes[1].children[0].note =
        Some("unit tests failed".to_string());

    h.app.on_server_msg(ServerMsg::Tree(working));

    assert!(h.app.pane_is_flashing(PaneId(101)));
    assert!(h
        .app
        .status
        .contains("claude / test runner: unit tests failed"));
}

#[test]
fn the_settings_cursor_stops_at_the_ends() {
    let mut h = Harness::new();
    h.app.open_settings();
    for _ in 0..10 {
        h.key(KeyCode::Char('j'));
    }
    let Some(Overlay::Settings { sel }) = h.app.overlay else {
        panic!("no settings panel")
    };
    assert_eq!(sel, crate::app::Setting::ALL.len() - 1);
}

#[test]
fn esc_closes_the_settings_panel() {
    let mut h = Harness::new();
    h.app.open_settings();
    h.key(KeyCode::Esc);
    assert!(h.app.overlay.is_none());
}

#[test]
fn settings_keys_never_reach_a_pane() {
    // The panel shares the overlay slot with a live editor; leaking a
    // keystroke into a child would be silent and destructive.
    let mut h = Harness::new();
    h.keys("lll");
    h.sent();
    h.app.open_settings();

    h.keys("jklh");

    assert!(!h
        .sent()
        .iter()
        .any(|m| matches!(m, ClientMsg::Input { .. })));
}
