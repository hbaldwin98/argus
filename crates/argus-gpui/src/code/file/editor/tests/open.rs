//! A file opening into the editor, and another replacing it without a blank frame
//! (EDITOR-OPEN, EDITOR-SWITCH).

use gpui_kit::EntityInputHandler as _;

use super::*;

/// EDITOR-OPEN-4, EDITOR-STATE-1: a file shown for the first time is asked for, the bar says so,
/// and the text is the daemon's once it answers.
#[gpui_kit::test]
fn a_file_is_asked_for_and_the_bar_says_opening_until_the_daemon_answers(cx: &mut TestAppContext) {
    let opened = asking(cx, editing(3, 1));
    settle(cx, &opened);
    assert_eq!(sent(&opened), ["Edit { gesture: Open { checkout: 100, path: \"src/lib.rs\" } }"]);
    assert_eq!(bar(cx, &opened).as_deref(), Some("Opening\u{2026}"));
    assert_eq!(text(cx), None, "no text, so no view, before the daemon has answered");
    hear(cx, &opened, vec![opened_with("src/lib.rs", "let a = 1;\nlet b = 2;\nlet c = 3;\n")]);
    settle(cx, &opened);
    assert_eq!(text(cx).as_deref(), Some("let a = 1;\nlet b = 2;\nlet c = 3;\n"));
    assert_eq!(bar(cx, &opened).as_deref(), Some("Saved"));
}

/// EDITOR-OPEN-6, EDITOR-SWITCH-1: a page that says the same file again does not ask again.
#[gpui_kit::test]
fn the_same_file_is_not_asked_for_twice(cx: &mut TestAppContext) {
    let opened = asking(cx, editing(3, 1));
    assert_eq!(sent(&opened).len(), 1);
    says(cx, &opened, editing(3, 2));
    assert_eq!(sent(&opened), Vec::<String>::new());
}

/// EDITOR-SWITCH-2: choosing another file keeps the previous file's editor on screen until the
/// new file's text arrives, and the old file is let go once the new one is drawn (SWITCH-4).
#[gpui_kit::test]
fn another_file_leaves_the_previous_one_on_screen_until_its_text_arrives(cx: &mut TestAppContext) {
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![opened_with("src/lib.rs", "first\n")]);
    sent(&opened);
    says(cx, &opened, other("src/other.rs"));
    settle(cx, &opened);
    assert_eq!(sent(&opened), ["Edit { gesture: Open { checkout: 100, path: \"src/other.rs\" } }"]);
    assert_eq!(text(cx).as_deref(), Some("first\n"), "the file before stays up while the new one opens");
    assert_eq!(bar(cx, &opened).as_deref(), Some("Saved"), "and so does its bar");
    hear(cx, &opened, vec![opened_with("src/other.rs", "second\n")]);
    settle(cx, &opened);
    assert_eq!(text(cx).as_deref(), Some("second\n"));
    assert_eq!(sent(&opened), ["Edit { gesture: Close { checkout: 100, path: \"src/lib.rs\" } }"], "the file left is closed once nothing of it is unwritten");
}

/// EDITOR-SWITCH-5: coming back to a file that was not yet let go shows its buffer, with nothing
/// asked of the daemon again.
#[gpui_kit::test]
fn a_file_come_back_to_before_it_was_let_go_is_not_asked_for_again(cx: &mut TestAppContext) {
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![opened_with("src/lib.rs", "first\n")]);
    says(cx, &opened, other("src/other.rs"));
    sent(&opened);
    says(cx, &opened, editing(3, 1));
    assert_eq!(sent(&opened), Vec::<String>::new());
    assert_eq!(text(cx).as_deref(), Some("first\n"));
}

/// EDITOR-STATE-6: a file the daemon lets go is read-only, says why, and takes nothing typed.
#[gpui_kit::test]
fn a_closed_file_says_why_and_is_read_only(cx: &mut TestAppContext) {
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![opened_with("src/lib.rs", "text\n")]);
    heard_edit(cx, &opened, EditEvent::Closed { checkout: 100, path: "src/lib.rs".into(), reason: "it is binary".into() });
    assert_eq!(bar(cx, &opened).as_deref(), Some("Closed: it is binary"));
    let state = state(cx);
    cx.update_window(opened.handle, |_, window, cx| state.update(cx, |state, cx| state.replace_text_in_range(None, "x", window, cx))).unwrap();
    assert_eq!(text(cx).as_deref(), Some("text\n"), "a closed file takes no typing");
}

/// EDITOR-SWITCH-3: a file refused before it opens replaces the one held on screen, so the bar
/// says why (`ts:code/editor.ts` skips the hold-over for a closed file).
#[gpui_kit::test]
fn a_file_refused_while_the_previous_is_held_says_why(cx: &mut TestAppContext) {
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![opened_with("src/lib.rs", "first\n")]);
    says(cx, &opened, other("src/other.rs"));
    settle(cx, &opened);
    assert_eq!(bar(cx, &opened).as_deref(), Some("Saved"));
    heard_edit(cx, &opened, EditEvent::Closed { checkout: 100, path: "src/other.rs".into(), reason: "it is binary".into() });
    settle(cx, &opened);
    assert_eq!(bar(cx, &opened).as_deref(), Some("Closed: it is binary"));
}

/// EDITOR-RECONN-1, -11: a lost connection says so and keeps the buffer; the file is opened
/// again when the connection says it is back, not when the stage next draws.
#[gpui_kit::test]
fn a_lost_connection_keeps_the_buffer_and_the_file_is_opened_again_when_it_is_back(cx: &mut TestAppContext) {
    use argus_desktop_core::connection::page::{Connection, Status};
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![opened_with("src/lib.rs", "text\n")]);
    sent(&opened);
    heard_edit(cx, &opened, EditEvent::Lost);
    assert_eq!(bar(cx, &opened).as_deref(), Some("Disconnected; kept here until the daemon is back"));
    assert_eq!(text(cx).as_deref(), Some("text\n"));
    let status = |connection| Event::Status(Status { connection, detail: None, alarm: None, chime: false, host: None });
    hear(cx, &opened, vec![status(Connection::Connecting)]);
    assert_eq!(sent(&opened), Vec::<String>::new(), "not while it is still trying");
    hear(cx, &opened, vec![status(Connection::Connected)]);
    assert_eq!(sent(&opened), ["Edit { gesture: Open { checkout: 100, path: \"src/lib.rs\" } }"]);
    assert_eq!(bar(cx, &opened).as_deref(), Some("Opening\u{2026}"));
}
