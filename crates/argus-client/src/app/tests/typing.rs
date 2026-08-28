//! Keys and pastes on their way into a pane.

use super::*;
// --- typing into a pane ------------------------------------------------

#[test]
fn keys_reach_the_child_when_inside_a_pane() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.keys("echo");
    h.key(KeyCode::Enter);

    let bytes: Vec<u8> = h
        .sent()
        .into_iter()
        .flat_map(|m| match m {
            ClientMsg::Input { pane, bytes } => {
                assert_eq!(pane, PaneId(100));
                bytes
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(bytes, b"echo\r");
}

#[test]
fn a_pointer_crossing_the_screen_is_not_a_reason_to_redraw() {
    let h = Harness::new();
    let moved = MouseEvent {
        kind: MouseEventKind::Moved,
        column: 4,
        row: 4,
        modifiers: KeyModifiers::NONE,
    };

    assert!(h.app.mouse_is_idle(&moved));
    assert!(!h.app.mouse_is_idle(&MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        ..moved
    }));
}

#[test]
fn ctrl_v_pastes_from_inside_a_pane_rather_than_reaching_the_child() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.clipboard = || Some("one\ntwo".to_string());

    h.app
        .on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));

    assert!(
        matches!(
            h.sent().as_slice(),
            [ClientMsg::Paste { pane: PaneId(100), text }] if text == "one\ntwo"
        ),
        "ctrl-v must not go to the child as a keystroke"
    );
}

#[test]
fn ctrl_shift_v_pastes_too() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.clipboard = || Some("x".to_string());

    h.app.on_key(KeyEvent::new(
        KeyCode::Char('V'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));

    assert!(matches!(h.sent().as_slice(), [ClientMsg::Paste { .. }]));
}

#[test]
fn the_paste_key_sends_the_clipboard_as_one_message() {
    // The point of an explicit key: no inference, and the newlines
    // stay newlines instead of arriving as a run of Enters.
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.clipboard = || {
        Some(
            "first
second
"
            .to_string(),
        )
    };

    h.app
        .on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::Paste { pane: PaneId(100), text }] if text == "first
second
"
    ));
    assert!(h.app.status.contains("2 lines"), "{}", h.app.status);
}

#[test]
fn the_paste_key_says_so_rather_than_failing_silently() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.clipboard = || None;

    h.app
        .on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));

    assert!(h.sent().is_empty(), "nothing to paste, nothing sent");
    assert!(
        h.app.status_alert,
        "a clipboard that cannot be read is worth saying"
    );
}

#[test]
fn a_paste_reaches_the_child_as_one_message() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();

    h.app.on_paste("first\nsecond".to_string());

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::Paste { pane: PaneId(100), text }] if text == "first\nsecond"
    ));
}

#[test]
fn navigation_keys_are_typed_not_interpreted_inside_a_pane() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.keys("hjkq");
    assert_eq!(h.app.focus, Focus::PaneContent, "still typing");
    assert!(!h.app.should_quit, "q must not detach from inside a pane");
    assert_eq!(h.sent().len(), 4, "all four went to the child");
}

#[test]
fn leader_then_esc_leaves_the_pane_without_typing_anything() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.leader();
    assert!(h.app.leader_pending);
    assert!(h.sent().is_empty(), "the leader itself is never forwarded");
    h.app.pane_fullscreen = true;

    h.key(KeyCode::Esc);
    assert_eq!(h.app.focus, Focus::Panes);
    assert!(!h.app.leader_pending);
    assert!(
        !h.app.pane_fullscreen,
        "leaving restores the navigation columns"
    );
    assert!(h.sent().is_empty());
}

#[test]
fn leader_then_f_toggles_pane_fullscreen_without_typing() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();

    h.leader();
    h.key(KeyCode::Char('f'));
    assert!(h.app.pane_fullscreen);
    assert!(
        h.sent().is_empty(),
        "the fullscreen chord never reaches the child"
    );

    h.leader();
    h.key(KeyCode::Char('f'));
    assert!(!h.app.pane_fullscreen);
    assert!(h.sent().is_empty());
}

#[test]
fn leader_then_x_closes_the_pane() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.pane_fullscreen = true;
    h.leader();
    h.key(KeyCode::Char('x'));
    assert!(matches!(h.sent()[0], ClientMsg::Kill { pane: PaneId(100) }));
    assert_eq!(
        h.app.focus,
        Focus::Panes,
        "land back in the list, not on another pane"
    );
    assert!(
        !h.app.pane_fullscreen,
        "closing restores the navigation columns"
    );
}

#[test]
fn an_unbound_leader_chord_is_swallowed_not_typed() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.leader();
    h.key(KeyCode::Char('Q'));
    assert!(h.sent().is_empty());
    assert!(!h.app.leader_pending, "chord consumed");
}
