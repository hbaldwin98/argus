use super::*;
use crate::board::snapshots;

// BOARD-WORK-21: the cursor's row is brought into view only when the cursor moved, so a push that
// redraws the same cursor on the same row never undoes the person's own scrolling.
#[test]
fn the_cursors_identity_is_its_feature_its_lens_and_its_row() {
    let work = lit(&snapshots::open("feature-work")).expect("the cursor is on a row");
    assert_eq!(work.which, Which::Outline);
    let record = lit(&snapshots::open("feature-record")).expect("the cursor is on a decision");
    let flows = lit(&snapshots::open("feature-flows")).expect("the cursor is on a step");
    assert_eq!((record.which, flows.which), (Which::Record, Which::Flows));
    assert!(work.identity.contains("/work/") && record.identity.contains("/record/") && flows.identity.contains("/flows/"), "{work:?} {record:?} {flows:?}");
    assert_ne!(work.identity, record.identity, "another lens is another cursor");
}

#[test]
fn a_push_that_leaves_the_cursor_where_it_was_leaves_its_identity_and_one_that_moves_it_changes_it() {
    let view = snapshots::open("feature-work");
    let before = lit(&view).unwrap();
    let mut moved = view.clone();
    let from = moved.rows.iter().position(|row| row.on).unwrap();
    moved.rows[from].on = false;
    moved.rows[from + 1].on = true;
    let after = lit(&moved).unwrap();
    assert_ne!(before.identity, after.identity, "the cursor moved");
    assert_eq!(after.child, before.child + 1, "and the row it is on is the next child of the list");
    let mut changed = view.clone();
    changed.rows[0].text = "reworded".into();
    changed.rows[from].state = "done";
    assert_eq!(lit(&changed).unwrap().identity, before.identity, "what a row says is not where the cursor is");
}

#[test]
fn the_list_keeps_a_child_for_the_line_that_says_the_tasks_are_still_being_read() {
    let mut view = snapshots::open("feature-work");
    let loaded = lit(&view).unwrap().child;
    view.loaded = false;
    assert_eq!(lit(&view).unwrap().child, loaded + 1);
}

#[test]
fn no_cursor_no_row_to_bring() {
    let mut view = snapshots::open("feature-work");
    for row in &mut view.rows {
        row.on = false;
    }
    assert!(lit(&view).is_none());
    let mut flows = snapshots::open("feature-flows");
    flows.flows.as_mut().unwrap().flow = None;
    assert!(lit(&flows).is_none());
}
