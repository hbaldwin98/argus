//! A clash drawn in the text, and a connection lost and back with typing in the buffer
//! (EDITOR-CLASH-3, EDITOR-RECONN).

use std::ops::Range;
use std::time::Duration;

use argus_desktop_core::connection::page::{Connection, Status};
use argus_protocol::TextChange;

use super::*;

fn open_with(cx: &mut TestAppContext, text: &str) -> Opened {
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![opened_with("src/lib.rs", text)]);
    settle(cx, &opened);
    sent(&opened);
    opened
}

fn type_at(cx: &mut TestAppContext, opened: &Opened, at: usize, typed: &str) {
    let state = state(cx);
    cx.update_window(opened.handle, |_, window, cx| {
        state.update(cx, |state, cx| {
            state.set_selected_range(at..at, cx);
            state.insert(typed.to_owned(), window, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
}

fn disk(cx: &mut TestAppContext, opened: &Opened, from: u64, version: u64, changes: Vec<TextChange>) {
    heard_edit(cx, opened, EditEvent::Changed { checkout: 100, path: "src/lib.rs".into(), from, version, changes });
}

fn after(cx: &mut TestAppContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

/// The ranges the clash marks cover.
fn marked(cx: &mut TestAppContext) -> Vec<Range<usize>> {
    let editor = editor();
    cx.update(|cx| {
        let editor = editor.read(cx);
        let session = editor.sessions.get(editor.shown.as_ref().unwrap()).unwrap();
        session.clashes.as_ref().map(|marks| marks.get_ranges(cx)).unwrap_or_default()
    })
}

fn status(connection: Connection) -> Event {
    Event::Status(Status { connection, detail: None, alarm: None, chime: false, host: None })
}

/// A clash held on `let a = 1;`: the person typed `X` inside `let`, the agent rewrote it as `var`.
fn clashing(cx: &mut TestAppContext, agent: &str) -> Opened {
    clashing_after(cx, "", agent)
}

/// The same, with `before` ahead of the line.
fn clashing_after(cx: &mut TestAppContext, before: &str, agent: &str) -> Opened {
    let opened = open_with(cx, &format!("{before}let a = 1;\n"));
    let at = before.len();
    type_at(cx, &opened, at + 1, "X");
    disk(cx, &opened, 0, 1, vec![TextChange { from: at as u32, to: at as u32 + 3, insert: agent.into() }]);
    settle(cx, &opened);
    opened
}

/// EDITOR-CLASH-3: the person's clashing text is marked, and the agent's words stand at it.
#[gpui_kit::test]
fn a_held_clash_marks_the_persons_text_and_says_what_the_agent_wrote(cx: &mut TestAppContext) {
    let opened = clashing(cx, "var");
    assert_eq!(marked(cx), vec![0..4], "the three letters with the person's X in them");
    assert_eq!(label(cx, &opened, ("theirs", 0usize)).as_deref(), Some("Line 1, the agent wrote var"));
    assert!(!present(cx, &opened, "editor-theirs"), "the words are in the line, not in a strip under the bar");
}

/// Where the editor last drew the end of the clash, and where the agent's words are drawn.
fn places(cx: &mut TestAppContext, opened: &Opened) -> (gpui_kit::Bounds<gpui_kit::Pixels>, gpui_kit::Bounds<gpui_kit::Pixels>) {
    use gpui_kit::test::TestWindowExt as _;
    let editing = state(cx);
    let end = marked(cx)[0].end;
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let editor = cx.update(|cx| editing.read(cx).range_to_bounds(&(end..end))).expect("the clash is in view");
    let widget = cx.update_window(opened.handle, |_, window, _| window.find(("theirs", 0usize)).bounds()).unwrap();
    (editor, widget)
}

/// EDITOR-CLASH-3: the agent's words stand on the clash's own line, just after the clash.
#[gpui_kit::test]
fn the_agents_words_stand_on_the_clashs_line_after_it(cx: &mut TestAppContext) {
    let opened = clashing_after(cx, "a
", "var");
    let (clash, widget) = places(cx, &opened);
    assert_eq!(widget.origin.y, clash.origin.y, "on the same line");
    assert!(widget.origin.x > clash.origin.x, "after the clash, not before it");
    assert!(widget.size.height <= clash.size.height, "no taller than the line");
}

/// EDITOR-CLASH-3: the marks move with typing, and so do the words that stand at them: a line
/// typed above carries them down a line.
#[gpui_kit::test]
fn the_agents_words_move_with_the_text_they_stand_at(cx: &mut TestAppContext) {
    let opened = clashing_after(cx, "a
", "var");
    let (_, before) = places(cx, &opened);
    type_at(cx, &opened, 0, "new
");
    let (clash, after) = places(cx, &opened);
    assert_eq!(after.origin.y, clash.origin.y);
    assert!(after.origin.y > before.origin.y, "{:?} is not below {:?}", after.origin, before.origin);
}

/// EDITOR-CLASH-3: a clash whose text the person deleted has no range left for a mark to hold, and
/// still says what the agent wrote at its place, without handing its words to the clash after it.
#[gpui_kit::test]
fn a_clash_with_nothing_left_of_the_persons_text_keeps_its_words_apart_from_the_next(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc
xyz
");
    let editing = state(cx);
    cx.update_window(opened.handle, |_, window, cx| {
        editing.update(cx, |state, cx| {
            state.set_selected_range(0..2, cx);
            state.replace(String::new(), window, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
    type_at(cx, &opened, 3, "Q");
    disk(cx, &opened, 0, 1, vec![TextChange { from: 1, to: 1, insert: "B".into() }, TextChange { from: 4, to: 7, insert: "XYZ".into() }]);
    settle(cx, &opened);
    assert_eq!(label(cx, &opened, ("theirs", 0usize)).as_deref(), Some("Line 1, the agent wrote B"));
    assert_eq!(label(cx, &opened, ("theirs", 1usize)).as_deref(), Some("Line 2, the agent wrote XYZ"));
}

/// EDITOR-CLASH-3: the words follow the editor's scrolling by themselves: the editor's own redraw
/// is not asked for, only the state of the editor changes.
#[gpui_kit::test]
fn the_agents_words_follow_the_editor_when_it_scrolls(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    let opened = clashing_after(cx, &"a
".repeat(40), "var");
    let editing = state(cx);
    let scroll = |lines: f32, cx: &mut TestAppContext| {
        cx.update_window(opened.handle, |_, window, cx| {
            editing.update(cx, |state, cx| {
                let line = state.line_height().unwrap();
                state.set_scroll_offset(gpui_kit::point(gpui_kit::px(0.), -line * lines), cx);
            });
            window.render_frame(cx);
            // The offset is taken up as the editor paints; the editor then says it changed.
            editing.update(cx, |_, cx| cx.notify());
            window.render_frame(cx);
        })
        .unwrap();
        cx.update_window(opened.handle, |_, window, _| window.find(("theirs", 0usize)).bounds()).unwrap()
    };
    let before = scroll(25., cx);
    let after = scroll(26., cx);
    assert!(after.origin.y < before.origin.y, "{:?} is not above {:?}", after.origin, before.origin);
}

/// EDITOR-CLASH-3: the marks move with typing, as the page's decorations are mapped through each change.
#[gpui_kit::test]
fn the_marks_move_with_what_is_typed(cx: &mut TestAppContext) {
    let opened = clashing_after(cx, "a\n", "var");
    assert_eq!(marked(cx), vec![2..6]);
    type_at(cx, &opened, 0, "// ");
    assert_eq!(marked(cx), vec![5..9]);
    assert_eq!(bar(cx, &opened).as_deref(), Some("An agent changed what you were editing"));
}

/// EDITOR-CLASH-3: where the agent deleted, the words say `removed`.
#[gpui_kit::test]
fn where_the_agent_deleted_the_words_say_removed(cx: &mut TestAppContext) {
    let opened = clashing(cx, "");
    assert_eq!(label(cx, &opened, ("theirs", 0usize)).as_deref(), Some("Line 1, the agent wrote removed"));
}

/// EDITOR-CLASH-5: once the person says whose text stands, nothing is marked and the words are gone.
#[gpui_kit::test]
fn resolving_a_clash_takes_the_marks_and_the_words_away(cx: &mut TestAppContext) {
    let opened = clashing(cx, "var");
    assert!(present(cx, &opened, ("theirs", 0usize)));
    crate::checkouts::stage::tests::click_id(cx, &opened, "editor-keep-mine");
    assert_eq!(marked(cx), Vec::<Range<usize>>::new());
    assert!(!present(cx, &opened, ("theirs", 0usize)));
}

/// No clash, no marks and no words, and a change that does not touch the person's text is not one.
#[gpui_kit::test]
fn a_change_clear_of_the_typing_marks_nothing(cx: &mut TestAppContext) {
    let opened = open_with(cx, "let a = 1;\nlet b = 2;\n");
    type_at(cx, &opened, 1, "X");
    disk(cx, &opened, 0, 1, vec![TextChange { from: 15, to: 16, insert: "c".into() }]);
    settle(cx, &opened);
    assert_eq!(marked(cx), Vec::<Range<usize>>::new());
    assert!(!present(cx, &opened, ("theirs", 0usize)));
}

/// EDITOR-RECONN-1, -2: typing while the connection is down stays in the buffer and is not sent
/// into nothing; the bar keeps saying so.
#[gpui_kit::test]
fn typing_while_disconnected_is_kept_and_not_sent(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    heard_edit(cx, &opened, EditEvent::Lost);
    type_at(cx, &opened, 0, "x");
    after(cx, 1500);
    assert_eq!(sent(&opened), Vec::<String>::new(), "nothing is sent while the daemon is away");
    assert_eq!(text(cx).as_deref(), Some("xabc\n"));
    assert_eq!(bar(cx, &opened).as_deref(), Some("Disconnected; kept here until the daemon is back"));
}

/// EDITOR-RECONN-4, -11: once back, the file is opened again, what became of it meanwhile lands as
/// an agent's change, and what the person typed is written over it.
#[gpui_kit::test]
fn on_reconnecting_the_disk_lands_and_the_typing_is_written_over_it(cx: &mut TestAppContext) {
    let opened = open_with(cx, "one\ntwo\n");
    heard_edit(cx, &opened, EditEvent::Lost);
    type_at(cx, &opened, 0, "x");
    hear(cx, &opened, vec![status(Connection::Connecting)]);
    hear(cx, &opened, vec![status(Connection::Connected)]);
    assert_eq!(sent(&opened), ["Edit { gesture: Open { checkout: 100, path: \"src/lib.rs\" } }"]);
    heard_edit(cx, &opened, EditEvent::Opened { checkout: 100, path: "src/lib.rs".into(), version: 0, text: "one\ntwo\nthree\n".into() });
    settle(cx, &opened);
    assert_eq!(text(cx).as_deref(), Some("xone\ntwo\nthree\n"), "the agent's line lands and the person's typing is kept");
    let writes = sent(&opened);
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert!(writes[0].contains("insert: \"x\""), "the typing is written over the new text: {}", writes[0]);
}

/// EDITOR-RECONN-7: a file the daemon let go stays closed across a lost connection.
#[gpui_kit::test]
fn a_closed_file_stays_closed_when_the_connection_is_lost(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    heard_edit(cx, &opened, EditEvent::Closed { checkout: 100, path: "src/lib.rs".into(), reason: "it is binary".into() });
    heard_edit(cx, &opened, EditEvent::Lost);
    assert_eq!(bar(cx, &opened).as_deref(), Some("Closed: it is binary"));
    hear(cx, &opened, vec![status(Connection::Connecting)]);
    hear(cx, &opened, vec![status(Connection::Connected)]);
    assert_eq!(sent(&opened), Vec::<String>::new(), "it is not opened again");
}

/// EDITOR-OPEN-14: the sync is made from the version the daemon answers, so a first change from that
/// version lands without asking to open the file again.
#[gpui_kit::test]
fn a_file_opened_at_a_later_version_takes_changes_from_that_version(cx: &mut TestAppContext) {
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![Event::Edit(EditEvent::Opened { checkout: 100, path: "src/lib.rs".into(), version: 7, text: "abc\n".into() })]);
    settle(cx, &opened);
    sent(&opened);
    disk(cx, &opened, 7, 8, vec![TextChange { from: 3, to: 3, insert: "d".into() }]);
    assert_eq!(text(cx).as_deref(), Some("abcd\n"));
    assert_eq!(sent(&opened), Vec::<String>::new(), "no resync");
}
