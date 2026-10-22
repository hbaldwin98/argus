use super::*;

#[test]
fn edits_are_gathered_into_the_unit_until_it_is_committed() {
    let mut b = Buffer::new("abc");
    b.replace(0, 0, "x");
    b.replace(4, 4, "y");
    assert_eq!(b.doc().to_string(), "xabcy");
    b.commit();
    b.undo().unwrap();
    assert_eq!(b.doc().to_string(), "abc", "both edits were one unit");
    assert!(b.undo().is_none());
}

#[test]
fn the_step_is_every_edit_since_it_was_taken_as_one_set() {
    let mut b = Buffer::new("abc");
    b.replace(0, 1, "A");
    b.replace(1, 1, "-");
    let step = b.take_step().unwrap();
    assert_eq!(step.old_len(), 3);
    assert_eq!(step.apply(&Rope::from_str("abc")).to_string(), "A-bc");
    assert!(b.take_step().is_none());
}

#[test]
fn a_read_only_buffer_takes_no_edit() {
    let mut b = Buffer::new("abc");
    b.set_read_only(true);
    b.replace(0, 0, "x");
    assert_eq!(b.doc().to_string(), "abc");
    assert!(b.take_step().is_none());
}

#[test]
fn a_change_from_outside_is_not_undone() {
    let mut b = Buffer::new("one\ntwo");
    b.replace(0, 3, "ONE");
    b.commit();
    b.take_step();
    let remote = ChangeSet::of(&[Change { from: 7, to: 7, insert: "!".into() }], 7).unwrap();
    b.past(&remote);
    b.undo().unwrap();
    assert_eq!(b.doc().to_string(), "one\ntwo!");
}

#[test]
fn line_breaks_are_one_character() {
    assert_eq!(Buffer::new("a\r\nb").doc().to_string(), "a\nb");
}
