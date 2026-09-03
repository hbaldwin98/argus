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
fn f_forwards_the_exact_line_to_the_only_live_agent() {
    let mut h = harness_with_a_note("# Plan\n  - [!] keep the spaces  \nlast");
    h.app.tree[0].repositories[0].checkouts[0].panes[1].kind = PaneKind::Agent;
    h.key(KeyCode::Char('j'));

    h.key(KeyCode::Char('f'));

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::ForwardNote { target, recipient: PaneId(101), body }]
            if *target == NoteTarget::Checkout(CheckoutId(10))
                && body == "  - [!] keep the spaces  "
    ));
    assert_eq!(h.app.status, "forwarding note…");
}

#[test]
fn uppercase_f_forwards_the_visible_whole_note() {
    let mut h = harness_with_a_note("# Plan\n\n- [ ] one");
    h.app.tree[0].repositories[0].checkouts[0].panes[1].kind = PaneKind::Agent;

    h.key(KeyCode::Char('F'));

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::ForwardNote { body, .. }] if body == "# Plan\n\n- [ ] one"
    ));
}

#[test]
fn forwarding_chooses_between_live_agents_in_the_note_scope() {
    let mut h = harness_with_a_note("route this");
    let checkout = &mut h.app.tree[0].repositories[0].checkouts[0];
    checkout.panes[0].kind = PaneKind::Agent;
    checkout.panes[0].template = Some("codex".to_string());
    checkout.panes[1].kind = PaneKind::Agent;
    checkout.panes[1].template = Some("claude".to_string());

    h.key(KeyCode::Char('F'));

    assert!(matches!(
        &h.app.picker.as_ref().unwrap().kind,
        PickerKind::NoteRecipient { panes, target, body }
            if panes == &[PaneId(100), PaneId(101)]
                && *target == NoteTarget::Checkout(CheckoutId(10))
                && body == "route this"
    ));
    assert!(h.sent().is_empty());
    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Enter);
    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::ForwardNote { recipient: PaneId(101), .. }]
    ));
}

#[test]
fn a_project_note_can_reach_an_agent_in_any_project_checkout() {
    let mut h = Harness::new();
    h.app.tree[0].repositories[0].checkouts[1]
        .panes
        .push(PaneInfo {
            id: PaneId(111),
            kind: PaneKind::Agent,
            title: "feature agent".to_string(),
            status: PaneStatus::Working,
            note: None,
            template: Some("claude".to_string()),
            children: Vec::new(),
        });
    h.key(KeyCode::Char('m'));
    h.app.on_server_msg(ServerMsg::Note(Box::new(argus_protocol::Note::new(
        NoteTarget::Project(ProjectId(1)),
        "project rule".to_string(),
    ))));
    h.sent();

    h.key(KeyCode::Char('F'));

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::ForwardNote {
            target: NoteTarget::Project(ProjectId(1)),
            recipient: PaneId(111),
            body,
        }] if body == "project rule"
    ));
}

#[test]
fn a_recipient_that_leaves_the_scope_while_the_picker_is_open_is_refused() {
    let mut h = harness_with_a_note("route this");
    for pane in &mut h.app.tree[0].repositories[0].checkouts[0].panes {
        pane.kind = PaneKind::Agent;
    }
    h.key(KeyCode::Char('F'));
    h.app.tree[0].repositories[0].checkouts[0].panes[0].status =
        PaneStatus::Exited { code: Some(0) };

    h.key(KeyCode::Enter);

    assert!(h.sent().is_empty());
    assert_eq!(h.app.status, "that agent is no longer in this note's scope");
}

#[test]
fn empty_note_text_and_missing_agents_are_reported_without_sending() {
    let mut h = harness_with_a_note("   \ncontent");
    h.key(KeyCode::Char('f'));
    assert_eq!(h.app.status, "line is empty");
    assert!(h.sent().is_empty());

    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Char('f'));
    assert_eq!(h.app.status, "no agent running in this note's scope");
    assert!(h.sent().is_empty());
}

#[test]
fn forwarding_keys_are_text_in_insert_mode_and_success_is_acknowledged() {
    let mut h = harness_with_a_note("");
    h.key(KeyCode::Char('i'));
    h.keys("fF");
    assert_eq!(h.app.notes.as_ref().unwrap().body(), "fF");
    assert!(h.sent().is_empty());

    h.app
        .on_server_msg(ServerMsg::NoteForwarded { recipient: PaneId(7) });
    assert_eq!(h.app.status, "note forwarded to agent #7");
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
