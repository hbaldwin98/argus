//! Writing a row's note, and toggling the boxes in it.

use super::*;
#[test]
fn m_opens_the_note_for_the_selected_checkout_and_asks_for_it() {
    let mut h = Harness::new();
    h.keys("ll");

    h.key(KeyCode::Char('m'));

    assert!(matches!(h.app.overlay, Some(Overlay::Notes)));
    let target = NoteTarget::Checkout(CheckoutId(10));
    assert_eq!(h.app.notes.as_ref().unwrap().target, target);
    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::GetNote { target: t }] if *t == target
    ));
}

#[test]
fn m_in_the_projects_column_takes_the_projects_note_instead() {
    let mut h = Harness::new();

    h.key(KeyCode::Char('m'));

    let target = NoteTarget::Project(ProjectId(1));
    assert_eq!(h.app.notes.as_ref().unwrap().target, target);
    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::GetNote { target: t }] if *t == target
    ));
}

#[test]
fn the_note_that_comes_back_fills_the_window() {
    let h = harness_with_a_note("- [ ] one
- [x] two");
    let view = h.app.notes.as_ref().unwrap();
    assert_eq!(view.lines, ["- [ ] one", "- [x] two"]);
    assert_eq!(view.counts(), NoteCounts { open: 1, done: 1, pinned: 0 });
}

#[test]
fn a_note_for_a_window_that_has_moved_on_is_ignored() {
    let mut h = harness_with_a_note("mine");
    h.app
        .on_server_msg(ServerMsg::Note(Box::new(argus_protocol::Note::new(
            NoteTarget::Checkout(CheckoutId(11)),
            "someone else".to_string(),
        ))));
    assert_eq!(h.app.notes.as_ref().unwrap().body(), "mine");
}

#[test]
fn typing_a_note_saves_it_on_leaving_insert_mode() {
    let mut h = harness_with_a_note("");
    h.key(KeyCode::Char('i'));
    h.keys("hello");
    assert!(h.sent().is_empty(), "nothing goes out mid-word");

    h.key(KeyCode::Esc);

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::SetNote { target, body }]
            if *target == NoteTarget::Checkout(CheckoutId(10)) && body == "hello"
    ));
    assert_eq!(h.app.notes.as_ref().unwrap().mode, NoteMode::View);
    assert!(matches!(h.app.overlay, Some(Overlay::Notes)), "still open");
}

#[test]
fn an_unchanged_note_is_not_written_back() {
    let mut h = harness_with_a_note("- [ ] one");
    h.key(KeyCode::Char('i'));
    h.key(KeyCode::Esc);
    assert!(h.sent().is_empty());
}

#[test]
fn space_ticks_the_box_under_the_cursor_by_line_and_state() {
    let mut h = harness_with_a_note("# Plan
- [ ] one
- [x] two");
    h.key(KeyCode::Char('j'));

    h.key(KeyCode::Char(' '));

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::SetTodo { target, line: 1, state: TodoState::Done }]
            if *target == NoteTarget::Checkout(CheckoutId(10))
    ));
}

#[test]
fn ticking_an_edited_note_sends_the_text_before_the_line_number() {
    // The daemon toggles by line against what it holds, so a body it
    // has not seen would make that line number mean something else.
    let mut h = harness_with_a_note("- [ ] one");
    // An edit that has not gone out: the daemon still holds the one
    // line, so a toggle of line 1 would land on nothing.
    let view = h.app.notes.as_mut().unwrap();
    view.end_of_line();
    view.newline();
    for c in "- [ ] two".chars() {
        view.insert_char(c);
    }
    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Char(' '));

    assert!(
        matches!(
            h.sent().as_slice(),
            [
                ClientMsg::SetNote { .. },
                ClientMsg::SetTodo { line: 1, state: TodoState::Done, .. }
            ]
        ),
        "the body goes first, then the line it is toggling"
    );
}

#[test]
fn space_on_a_line_with_no_box_says_so_and_sends_nothing() {
    let mut h = harness_with_a_note("# Plan
- [ ] one");

    h.key(KeyCode::Char(' '));

    assert!(h.sent().is_empty());
    assert_eq!(h.app.status, "no checkbox on this line");
}

#[test]
fn q_saves_and_closes_the_window() {
    let mut h = harness_with_a_note("start");
    h.key(KeyCode::Char('$'));
    h.key(KeyCode::Char('i'));
    h.keys("!");
    h.key(KeyCode::Esc);
    h.sent();
    h.key(KeyCode::Char('$'));
    h.key(KeyCode::Char('i'));
    h.keys("?");

    h.key(KeyCode::Esc);
    h.key(KeyCode::Char('q'));

    assert!(h.app.overlay.is_none());
    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::SetNote { body, .. }] if body == "start!?"
    ));
}

#[test]
fn in_insert_mode_navigation_keys_are_just_letters() {
    let mut h = harness_with_a_note("");
    h.key(KeyCode::Char('i'));
    h.keys("jkq");
    assert_eq!(h.app.notes.as_ref().unwrap().body(), "jkq");
    assert!(matches!(h.app.overlay, Some(Overlay::Notes)));
}

#[test]
fn a_refused_write_is_shown_on_the_note_rather_than_only_in_the_bar() {
    let mut h = harness_with_a_note("x");
    h.app.on_server_msg(ServerMsg::NoteFailed {
        target: NoteTarget::Checkout(CheckoutId(10)),
        message: "note exceeds 65536 bytes".to_string(),
    });
    assert_eq!(
        h.app.notes.as_ref().unwrap().error.as_deref(),
        Some("note exceeds 65536 bytes")
    );
    assert!(h.app.status_alert);
}

#[test]
fn there_is_nothing_to_take_notes_on_without_a_project() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::Tree(Vec::new()));
    h.sent();

    h.key(KeyCode::Char('m'));

    assert!(h.app.overlay.is_none());
    assert_eq!(h.app.status, "nothing to take notes on");
    assert!(h.sent().is_empty());
}
