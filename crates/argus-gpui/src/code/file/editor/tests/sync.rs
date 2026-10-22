//! What is typed is written, what an agent writes is shown, and the bar says each
//! (EDITOR-SAVE, EDITOR-DISK, EDITOR-CLASH, EDITOR-STATE).

use std::time::Duration;

use argus_protocol::TextChange;

use super::*;

/// A file open with `text`, the calls so far cleared.
fn open_with(cx: &mut TestAppContext, text: &str) -> Opened {
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![opened_with("src/lib.rs", text)]);
    settle(cx, &opened);
    sent(&opened);
    opened
}

/// The person types `typed` at the cursor.
fn type_at_cursor(cx: &mut TestAppContext, opened: &Opened, typed: &str) {
    let state = state(cx);
    cx.update_window(opened.handle, |_, window, cx| state.update(cx, |state, cx| state.insert(typed.to_owned(), window, cx))).unwrap();
    cx.run_until_parked();
}

fn after(cx: &mut TestAppContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn change(from: u32, to: u32, insert: &str) -> TextChange {
    TextChange { from, to, insert: insert.into() }
}

fn disk(cx: &mut TestAppContext, opened: &Opened, from: u64, version: u64, changes: Vec<TextChange>) {
    heard_edit(cx, opened, EditEvent::Changed { checkout: 100, path: "src/lib.rs".into(), from, version, changes });
}

/// EDITOR-SAVE-1, -2: typing is written once it pauses, as one write on the version it was made
/// on, and the bar goes Unsaved, Saving, Saved.
#[gpui_kit::test]
fn typing_is_written_once_it_pauses_and_the_bar_follows(cx: &mut TestAppContext) {
    let opened = open_with(cx, "let a = 1;\n");
    type_at_cursor(cx, &opened, "x");
    type_at_cursor(cx, &opened, "y");
    assert_eq!(bar(cx, &opened).as_deref(), Some("Unsaved"));
    assert_eq!(sent(&opened), Vec::<String>::new(), "nothing is written while typing goes on");
    after(cx, 250);
    let writes = sent(&opened);
    assert_eq!(writes.len(), 1, "a burst of typing is one write: {writes:?}");
    assert!(writes[0].starts_with("Edit { gesture: Change { checkout: 100, path: \"src/lib.rs\", base: 0, changes: [TextChange { from: 0, to: 0, insert: \"xy\" }] } }"), "{}", writes[0]);
    hear(cx, &opened, vec![]);
    assert_eq!(bar(cx, &opened).as_deref(), Some("Saving\u{2026}"));
    heard_edit(cx, &opened, EditEvent::Saved { checkout: 100, path: "src/lib.rs".into(), base: 0, version: 1 });
    assert_eq!(bar(cx, &opened).as_deref(), Some("Saved"));
}

/// EDITOR-SAVE-3, EDITOR-GUTTER-3: a write that settles has the file read again, once, 400 ms
/// after the last answer.
#[gpui_kit::test]
fn a_write_that_settles_has_the_file_read_again_once(cx: &mut TestAppContext) {
    let opened = open_with(cx, "a\n");
    type_at_cursor(cx, &opened, "x");
    after(cx, 250);
    sent(&opened);
    heard_edit(cx, &opened, EditEvent::Saved { checkout: 100, path: "src/lib.rs".into(), base: 0, version: 1 });
    assert_eq!(sent(&opened), Vec::<String>::new(), "not at once");
    after(cx, 450);
    assert_eq!(sent(&opened), ["Code { gesture: Written }"]);
}

/// EDITOR-SAVE-4: a refusal with a reason says so and is tried again after a second, the change
/// written again with anything new.
#[gpui_kit::test]
fn a_refused_write_says_why_and_is_tried_again_after_a_second(cx: &mut TestAppContext) {
    let opened = open_with(cx, "a\n");
    type_at_cursor(cx, &opened, "x");
    after(cx, 250);
    sent(&opened);
    heard_edit(cx, &opened, EditEvent::Refused { checkout: 100, path: "src/lib.rs".into(), base: 0, reason: Some("disk full".into()) });
    assert_eq!(bar(cx, &opened).as_deref(), Some("Not saved: disk full"));
    assert_eq!(sent(&opened), Vec::<String>::new());
    after(cx, 1050);
    let again = sent(&opened);
    assert_eq!(again.len(), 1);
    assert!(again[0].contains("insert: \"x\""), "{}", again[0]);
}

/// EDITOR-DISK-1, -2: what an agent writes lands at once, the person's cursor stays on the text it
/// was on, and the bar does not call it unsaved.
#[gpui_kit::test]
fn an_agents_write_lands_without_moving_the_cursor_or_marking_the_file_unsaved(cx: &mut TestAppContext) {
    let opened = open_with(cx, "one\ntwo\nthree\n");
    // The cursor is on "three".
    let state = state(cx);
    cx.update_window(opened.handle, |_, _, cx| state.update(cx, |state, cx| state.set_selected_range(8..8, cx))).unwrap();
    disk(cx, &opened, 0, 1, vec![change(0, 0, "zero\n")]);
    settle(cx, &opened);
    assert_eq!(text(cx).as_deref(), Some("zero\none\ntwo\nthree\n"));
    let cursor = cx.update(|cx| state.read(cx).cursor());
    assert_eq!(cursor, 13, "still at the start of three, which moved down a line");
    assert_eq!(bar(cx, &opened).as_deref(), Some("Saved"), "the person typed nothing");
    assert_eq!(sent(&opened), Vec::<String>::new(), "nothing of it is written back");
}

/// EDITOR-DISK-3, EDITOR-GUTTER-3: the file is read again for its marks after an agent's write.
#[gpui_kit::test]
fn an_agents_write_has_the_file_read_again(cx: &mut TestAppContext) {
    let opened = open_with(cx, "one\n");
    disk(cx, &opened, 0, 1, vec![change(3, 3, "!")]);
    after(cx, 450);
    assert_eq!(sent(&opened), ["Code { gesture: Written }"]);
}

/// EDITOR-SYNC-12: a change made on a version the editor does not hold means the two have parted:
/// the file is opened again and nothing is shown.
#[gpui_kit::test]
fn a_change_on_another_version_has_the_file_opened_again(cx: &mut TestAppContext) {
    let opened = open_with(cx, "one\n");
    disk(cx, &opened, 5, 6, vec![change(0, 0, "x")]);
    assert_eq!(sent(&opened), ["Edit { gesture: Open { checkout: 100, path: \"src/lib.rs\" } }"]);
    assert_eq!(text(cx).as_deref(), Some("one\n"));
}

/// EDITOR-CLASH-2 to -5: a write that touches what the person typed is held: the bar says so, the
/// buffer keeps the person's text and nothing is written; "Take the agent's" shows the agent's
/// text and writes at once.
#[gpui_kit::test]
fn a_clash_is_held_until_the_person_says_whose_text_stands(cx: &mut TestAppContext) {
    let opened = open_with(cx, "let a = 1;\n");
    // The person types inside the three letters the agent rewrites.
    let typing = state(cx);
    cx.update(|cx| typing.update(cx, |state, cx| state.set_selected_range(1..1, cx)));
    type_at_cursor(cx, &opened, "X");
    disk(cx, &opened, 0, 1, vec![change(0, 3, "var")]);
    settle(cx, &opened);
    assert_eq!(bar(cx, &opened).as_deref(), Some("An agent changed what you were editing"));
    assert!(present(cx, &opened, "editor-keep-mine") && present(cx, &opened, "editor-take-theirs"));
    assert_eq!(text(cx).as_deref(), Some("lXet a = 1;\n"), "the buffer keeps what the person typed");
    after(cx, 1500);
    assert!(sent(&opened).iter().all(|call| !call.starts_with("Edit")), "nothing is written while a clash waits");
    crate::checkouts::stage::tests::click_id(cx, &opened, "editor-take-theirs");
    assert_eq!(text(cx).as_deref(), Some("var a = 1;\n"), "the agent's change lands and the clashing typing is given up");
    assert_eq!(bar(cx, &opened).as_deref(), Some("Saved"));
}

/// EDITOR-CLASH-5: "Keep mine" leaves the buffer as it is and writes the person's text over the
/// disk's change at once.
#[gpui_kit::test]
fn keeping_mine_writes_the_persons_text_over_the_agents(cx: &mut TestAppContext) {
    let opened = open_with(cx, "let a = 1;\n");
    let typing = state(cx);
    cx.update(|cx| typing.update(cx, |state, cx| state.set_selected_range(1..1, cx)));
    type_at_cursor(cx, &opened, "X");
    disk(cx, &opened, 0, 1, vec![change(0, 3, "var")]);
    settle(cx, &opened);
    crate::checkouts::stage::tests::click_id(cx, &opened, "editor-keep-mine");
    assert_eq!(text(cx).as_deref(), Some("lXet a = 1;\n"));
    let writes = sent(&opened);
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert!(writes[0].contains("base: 1"), "written on the agent's version: {}", writes[0]);
}
