//! Keys and pastes on their way into a pane, and a child's copies on
//! their way out of one.

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
fn alt_v_pastes_too_for_a_terminal_that_keeps_ctrl_v() {
    // Windows Terminal never passes Ctrl-V on; Alt-V is the key that gets
    // through, and the one Claude Code taught Windows users already.
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.clipboard = || Some("x".to_string());

    h.app.on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT));

    assert!(matches!(h.sent().as_slice(), [ClientMsg::Paste { .. }]));
}

#[test]
fn an_image_on_the_clipboard_crosses_as_a_file_not_as_its_text() {
    // A screenshot copied from a browser comes with its URL as text; the
    // picture is what was meant, and it goes as bytes because the pane's
    // host may be another machine with no clipboard of this one's.
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.greeted(Some(&argus_protocol::Hello::this_build()));
    h.app.clipboard = || Some("https://example.com/shot".to_string());
    h.app.clipboard_image = || Some(b"\x89PNG...".to_vec());

    h.app
        .on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));

    assert!(
        matches!(
            h.sent().as_slice(),
            [ClientMsg::PasteFile { pane: PaneId(100), name, bytes }]
                if name == "clipboard.png" && bytes == b"\x89PNG..."
        ),
        "{}",
        h.app.status
    );
    assert!(h.app.status.contains("image"), "{}", h.app.status);
}

#[test]
fn an_image_for_a_daemon_that_takes_none_is_said_not_sent() {
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.greeted(None);
    h.app.clipboard_image = || Some(vec![1, 2, 3]);

    h.app
        .on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));

    assert!(h.sent().is_empty(), "a message the daemon cannot read is never sent");
    assert!(h.app.status_alert, "{}", h.app.status);
    assert!(h.app.status.contains("argus server restart"), "{}", h.app.status);
}

#[test]
fn an_image_is_not_pasted_where_only_text_goes() {
    // A prompt over the pane takes typed text; an image on the clipboard
    // must neither reach the pane behind it nor stop the text arriving.
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.greeted(Some(&argus_protocol::Hello::this_build()));
    h.app.prompt = Some(Prompt::EditorCommand { input: String::new() });
    h.app.clipboard = || Some("typed".to_string());
    h.app.clipboard_image = || Some(vec![1, 2, 3]);

    h.app
        .on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL));

    assert!(h.sent().is_empty(), "nothing crosses for a prompt");
    match &h.app.prompt {
        Some(Prompt::EditorCommand { input }) => assert_eq!(input, "typed"),
        _ => panic!("the prompt is gone"),
    }
}

#[test]
fn an_image_dropped_on_the_terminal_crosses_as_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let shot = dir.path().join("shot.png");
    std::fs::write(&shot, b"picture").unwrap();
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.greeted(Some(&argus_protocol::Hello::this_build()));

    h.app.on_paste(format!("'{}'", shot.display()));

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::PasteFile { pane: PaneId(100), name, bytes }]
            if name == "shot.png" && bytes == b"picture"
    ));
}

#[test]
fn an_image_dropped_from_another_machine_is_pasted_as_typed_and_the_fix_named() {
    // The client is on a box ssh'd into from a desktop; the desktop's
    // screenshot was dropped on that terminal. Nothing here can read it,
    // and the way to make it cross is to run the client on the desktop.
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.greeted(Some(&argus_protocol::Hello::this_build()));

    h.app.on_paste("C:\\Users\\me\\Pictures\\shot.png".to_string());

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::Paste { pane: PaneId(100), text }] if text == "C:\\Users\\me\\Pictures\\shot.png"
    ));
    assert!(h.app.status_alert, "{}", h.app.status);
    assert!(h.app.status.contains("shot.png"), "{}", h.app.status);
    assert!(h.app.status.contains("--host"), "{}", h.app.status);
}

#[test]
fn a_dropped_path_an_old_daemon_cannot_take_as_a_file_pastes_as_the_path_and_says_so() {
    // The path still names the file to a daemon on this machine, which is
    // what every drop did before files crossed; the alert is for the one
    // on another machine, where it names nothing.
    let dir = tempfile::tempdir().unwrap();
    let shot = dir.path().join("shot.png");
    std::fs::write(&shot, b"picture").unwrap();
    let typed = shot.display().to_string();
    let mut h = Harness::new();
    h.keys("llll");
    h.sent();
    h.app.greeted(None);

    h.app.on_paste(typed.clone());

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::Paste { pane: PaneId(100), text }] if *text == typed
    ));
    assert!(h.app.status_alert, "{}", h.app.status);
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

thread_local! {
    static COPIED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn record_copy(text: &str) -> bool {
    COPIED.with(|copied| copied.borrow_mut().push(text.to_string()));
    true
}

#[test]
fn a_childs_copy_reaches_the_clipboard() {
    let mut h = Harness::new();
    h.app.clipboard_write = record_copy;

    h.app.on_server_msg(ServerMsg::Clipboard {
        pane: PaneId(100),
        text: "from the agent".into(),
    });

    COPIED.with(|copied| assert_eq!(*copied.borrow(), vec!["from the agent".to_string()]));
    assert!(!h.app.status_alert);
}

// --- messages queued from a phone -----------------------------------------

#[test]
fn u_on_the_panes_stage_takes_back_the_newest_message_queued_for_that_agent() {
    let mut h = Harness::new();
    let mut t = tree();
    for (pane, base) in t[0].repositories[0].checkouts[0].panes.iter_mut().zip([10, 20]) {
        pane.queued = vec![
            argus_protocol::QueuedMessage { id: base, text: "first".into() },
            argus_protocol::QueuedMessage { id: base + 1, text: "second".into() },
        ];
    }
    h.app.on_server_msg(ServerMsg::Tree(t));
    h.keys("3");
    h.sent();
    let selected = h.app.current_pane().unwrap();
    let (pane, newest) = (selected.id, selected.queued[1].id);

    h.key(KeyCode::Char('u'));

    let sent = h.sent();
    assert!(
        sent.iter().any(|m| matches!(m, ClientMsg::CancelQueued { pane: p, id } if *p == pane && *id == newest)),
        "{sent:?}"
    );
    // Nothing is taken off the card until the daemon says it is gone: it
    // may have been typed already.
    assert_eq!(h.app.current_pane().unwrap().queued.len(), 2);
}
