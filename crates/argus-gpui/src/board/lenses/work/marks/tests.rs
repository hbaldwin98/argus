use super::*;
use crate::board::snapshots;

fn row(at: WorkAt, state: &'static str) -> WorkRow {
    WorkRow { at, label: "1".into(), depth: 0, text: "t".into(), state, waits_on: None, key: None, changed: false, on: false, subtasks: None }
}

fn task(state: &'static str) -> WorkRow {
    row(WorkAt::Task { id: 1 }, state)
}

fn decision(state: &'static str) -> WorkRow {
    row(WorkAt::Decision { id: 1 }, state)
}

// BOARD-WORK-4: the marks and the words of their tooltips.
#[test]
fn each_state_has_its_mark_and_its_tone() {
    assert_eq!(state_mark("todo"), ("\u{25cb}", Tone::Faint));
    assert_eq!(state_mark("doing"), ("\u{25d0}", Tone::Attention));
    assert_eq!(state_mark("done"), ("\u{2713}", Tone::Ok));
    assert_eq!(state_mark("open"), ("?", Tone::Attention));
    assert_eq!(state_mark("accepted"), ("\u{25c6}", Tone::Ref));
    assert_eq!(state_mark("rejected"), ("\u{2715}", Tone::Faint));
    assert_eq!(state_mark("withdrawn"), ("\u{2298}", Tone::Faint));
    assert_eq!(state_mark("superseded"), ("\u{2298}", Tone::Faint));
    assert_eq!(state_mark("something else"), ("\u{b7}", Tone::Faint));
}

#[test]
fn the_tooltips_say_the_states_in_words_and_an_unlisted_state_says_itself() {
    let said: Vec<_> = ["todo", "doing", "done", "accepted", "open", "rejected", "withdrawn"].into_iter().map(mark_word).collect();
    assert_eq!(said, ["to do", "in progress", "done", "decided", "open", "rejected", "withdrawn"]);
}

// BOARD-WORK-5: the first that applies wins.
#[test]
fn what_stands_at_a_rows_end_is_the_first_of_waiting_waits_on_the_state_and_the_subtasks() {
    assert_eq!(aside(&decision("open")), Aside::Waiting);
    let mut held = task("todo");
    held.waits_on = Some("D2".into());
    held.subtasks = Some((1, 2));
    assert_eq!(aside(&held), Aside::WaitsOn("D2".into()), "waiting on a decision beats the subtask count");
    assert_eq!(aside(&decision("withdrawn")), Aside::State("withdrawn".into()));
    assert_eq!(aside(&decision("rejected")), Aside::State("rejected".into()));
    let mut parent = task("doing");
    parent.subtasks = Some((1, 3));
    assert_eq!(aside(&parent), Aside::Subtasks(1, 3));
    assert_eq!(aside(&task("done")), Aside::Nothing);
    assert_eq!(aside(&decision("accepted")), Aside::Nothing);
    let mut open_and_waiting = decision("open");
    open_and_waiting.waits_on = Some("D1".into());
    assert_eq!(aside(&open_and_waiting), Aside::Waiting, "an open decision says waiting before anything");
}

// BOARD-WORK-6: how the text is set.
#[test]
fn the_cursors_row_is_strong_done_and_out_rows_are_muted_and_a_rejected_one_is_struck() {
    assert_eq!(text("doing", true), Text { strong: true, muted: false, struck: false });
    assert_eq!(text("todo", false), Text { strong: false, muted: false, struck: false });
    for state in ["done", "withdrawn", "superseded"] {
        assert_eq!(text(state, false), Text { strong: false, muted: true, struck: false }, "{state}");
    }
    assert_eq!(text("rejected", true), Text { strong: false, muted: false, struck: true }, "never strong, though it is the cursor's");
    assert_eq!(text("done", true), Text { strong: true, muted: true, struck: false });
}

#[test]
fn a_label_is_attention_for_an_open_decision_the_link_colour_for_another_and_faint_for_a_task() {
    assert_eq!(label_tone(&decision("open")), Tone::Attention);
    assert_eq!(label_tone(&decision("accepted")), Tone::Ref);
    assert_eq!(label_tone(&decision("rejected")), Tone::Faint);
    assert_eq!(label_tone(&decision("superseded")), Tone::Ref);
    assert_eq!(label_tone(&task("doing")), Tone::Faint);
}

// BOARD-WORK-7: a dot on every row says nothing.
#[test]
fn a_changed_dot_is_drawn_only_where_some_rows_changed_and_some_did_not() {
    let mut view = snapshots::open("feature-work");
    for row in &mut view.rows {
        row.changed = false;
    }
    assert!(!mixed(&view), "none changed");
    view.rows[0].changed = true;
    assert!(mixed(&view), "one of several");
    for row in &mut view.rows {
        row.changed = true;
    }
    assert!(!mixed(&view), "all of them");
}
