use super::*;

// Source: the page's `sync.ts` `clash`, and EDITOR-CLASH-1.

#[test]
fn overlapping_runs_clash() {
    assert!(clash((2, 5), (4, 8)));
    assert!(clash((2, 5), (3, 4)));
}

#[test]
fn runs_that_meet_end_to_end_do_not() {
    assert!(!clash((2, 5), (5, 8)));
    assert!(!clash((5, 8), (2, 5)));
}

#[test]
fn two_insertions_at_one_place_clash_but_apart_they_do_not() {
    assert!(clash((3, 3), (3, 3)));
    assert!(!clash((3, 3), (4, 4)));
}

#[test]
fn an_insertion_inside_a_replaced_run_clashes_but_at_its_edge_it_does_not() {
    assert!(clash((4, 4), (2, 6)));
    assert!(clash((2, 6), (4, 4)));
    assert!(!clash((2, 2), (2, 6)));
    assert!(!clash((6, 6), (2, 6)));
}

#[test]
fn a_set_splits_into_what_clears_and_what_clashes() {
    let c = |from, to, insert: &str| Change { from, to, insert: insert.into() };
    let theirs = ChangeSet::of(&[c(0, 1, "A"), c(8, 9, "3")], 12).unwrap();
    let mine = ChangeSet::of(&[c(8, 9, "2")], 12).unwrap();
    let (clear, clashing) = split(&theirs, &mine);
    assert_eq!(clear.changes(), vec![c(0, 1, "A")]);
    assert_eq!(clashing.changes(), vec![c(8, 9, "3")]);
    assert_eq!((clear.old_len(), clashing.old_len()), (12, 12));
}
