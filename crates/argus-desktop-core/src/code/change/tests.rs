use ropey::Rope;

use super::*;

fn doc(s: &str) -> Rope {
    Rope::from_str(s)
}

fn at(len: usize, from: usize, to: usize, insert: &str) -> ChangeSet {
    ChangeSet::of(&[Change { from, to, insert: insert.into() }], len).unwrap()
}

#[test]
fn a_set_applies_each_change_where_it_was_made() {
    let set = ChangeSet::of(
        &[
            Change { from: 0, to: 3, insert: "ONE".into() },
            Change { from: 8, to: 8, insert: "!".into() },
        ],
        13,
    )
    .unwrap();
    assert_eq!(set.apply(&doc("one\ntwo\nthree")).to_string(), "ONE\ntwo\n!three");
    assert_eq!((set.old_len(), set.new_len()), (13, 14));
}

#[test]
fn changes_that_do_not_fit_are_refused() {
    let c = |from, to| Change { from, to, insert: String::new() };
    assert_eq!(ChangeSet::of(&[c(2, 1)], 5), Err(Misfit::Range { from: 2, to: 1 }));
    assert_eq!(ChangeSet::of(&[c(4, 6)], 5), Err(Misfit::Range { from: 4, to: 6 }));
    assert_eq!(ChangeSet::of(&[c(0, 3), c(2, 4)], 5), Err(Misfit::Order { from: 2 }));
}

#[test]
fn changes_that_touch_are_one_run() {
    let set = ChangeSet::of(
        &[
            Change { from: 0, to: 1, insert: "a".into() },
            Change { from: 1, to: 2, insert: "b".into() },
        ],
        4,
    )
    .unwrap();
    assert_eq!(set.changes(), vec![Change { from: 0, to: 2, insert: "ab".into() }]);
}

#[test]
fn composing_then_applying_is_applying_twice() {
    let first = at(5, 1, 3, "XYZ");
    let then = at(6, 0, 2, "");
    let both = first.compose(&then);
    assert_eq!(both.apply(&doc("abcde")).to_string(), then.apply(&first.apply(&doc("abcde"))).to_string());
    assert_eq!(both.apply(&doc("abcde")).to_string(), "YZde");
}

#[test]
fn text_inserted_and_deleted_again_is_never_there() {
    let first = at(3, 1, 1, "xy");
    let then = at(5, 1, 3, "");
    assert!(first.compose(&then).is_empty());
}

#[test]
fn an_inverse_restores_the_text() {
    let d = doc("héllo\nwörld");
    let set = ChangeSet::of(
        &[
            Change { from: 1, to: 3, insert: "E".into() },
            Change { from: 7, to: 10, insert: String::new() },
        ],
        d.len(),
    )
    .unwrap();
    assert_eq!(set.invert(&d).apply(&set.apply(&d)).to_string(), d.to_string());
}

#[test]
fn insertions_at_one_place_go_by_the_side_mapped_first() {
    let a = at(2, 1, 1, "A");
    let b = at(2, 1, 1, "B");
    let ab = a.compose(&b.map(&a, false));
    let ba = b.compose(&a.map(&b, true));
    assert_eq!(ab.apply(&doc("xy")).to_string(), "xABy");
    assert_eq!(ba.apply(&doc("xy")).to_string(), "xABy");
    // `before` puts this set's text first.
    let first = a.compose(&b.map(&a, true));
    assert_eq!(first.apply(&doc("xy")).to_string(), "xBAy");
}

#[test]
fn text_deleted_by_both_is_deleted_once_and_an_insertion_inside_it_stays() {
    let a = at(6, 1, 5, "");
    let b = ChangeSet::of(
        &[Change { from: 2, to: 4, insert: String::new() }, Change { from: 3, to: 3, insert: "!".into() }],
        6,
    );
    assert!(b.is_err(), "sets are sorted and apart");
    let b = at(6, 3, 3, "!");
    let moved = b.map(&a, false);
    assert_eq!(a.compose(&moved).apply(&doc("abcdef")).to_string(), "a!f");
    let c = at(6, 2, 4, "");
    assert_eq!(a.compose(&c.map(&a, false)).apply(&doc("abcdef")).to_string(), "af");
}

#[test]
fn a_position_is_carried_over_the_changes_before_it() {
    let set = at(6, 2, 4, "xyz");
    assert_eq!(set.map_pos(1, 1), 1);
    assert_eq!(set.map_pos(2, 1), 2, "the start of a replaced run stays before it");
    assert_eq!(set.map_pos(3, -1), 2, "inside goes to the start for -1");
    assert_eq!(set.map_pos(3, 1), 5, "and to the end for 1");
    assert_eq!(set.map_pos(4, 1), 5);
    assert_eq!(set.map_pos(6, 1), 7);
    let ins = at(4, 2, 2, "ab");
    assert_eq!(ins.map_pos(2, -1), 2);
    assert_eq!(ins.map_pos(2, 1), 4);
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    /// A text with multi-byte characters and line breaks, so a byte is
    /// not always a char.
    fn text() -> impl Strategy<Value = String> {
        "[abc\n é😀]{0,14}"
    }

    type Picks = Vec<(usize, usize, u8)>;

    fn picks() -> impl Strategy<Value = Picks> {
        proptest::collection::vec((0usize..64, 0usize..64, 0u8..6), 0..5)
    }

    /// A valid set on `text` from the picks, cut at char boundaries.
    fn set(text: &str, picks: &Picks) -> ChangeSet {
        let mut bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        bounds.push(text.len());
        let pieces = ["x", "yz", "\n", "é", "😀", ""];
        let mut changes: Vec<Change> = picks
            .iter()
            .map(|&(a, b, p)| {
                let (a, b) = (bounds[a % bounds.len()], bounds[b % bounds.len()]);
                Change { from: a.min(b), to: a.max(b), insert: pieces[p as usize].into() }
            })
            .collect();
        changes.sort_by_key(|c| (c.from, c.to));
        let mut kept: Vec<Change> = Vec::new();
        for c in changes {
            if kept.last().is_none_or(|last| last.to <= c.from) {
                kept.push(c);
            }
        }
        ChangeSet::of(&kept, text.len()).unwrap()
    }

    proptest! {
        // The algebra's own laws, as CodeMirror's `ChangeSet` keeps them:
        // `sync` rests on compose, invert and map agreeing with applying.
        #[test]
        fn composing_is_applying_in_turn(t in text(), p in picks(), q in picks()) {
            let d = Rope::from_str(&t);
            let a = set(&t, &p);
            let mid = a.apply(&d);
            let b = set(&mid.to_string(), &q);
            prop_assert_eq!(a.compose(&b).apply(&d).to_string(), b.apply(&mid).to_string());
        }

        #[test]
        fn an_inverse_undoes_what_the_set_did(t in text(), p in picks()) {
            let d = Rope::from_str(&t);
            let a = set(&t, &p);
            prop_assert_eq!(a.invert(&d).apply(&a.apply(&d)).to_string(), t);
        }

        #[test]
        fn two_sets_on_one_text_meet_in_one_text(t in text(), p in picks(), q in picks(), first in any::<bool>()) {
            let d = Rope::from_str(&t);
            let (a, b) = (set(&t, &p), set(&t, &q));
            let left = a.compose(&b.map(&a, !first)).apply(&d).to_string();
            let right = b.compose(&a.map(&b, first)).apply(&d).to_string();
            prop_assert_eq!(left, right);
        }

        #[test]
        fn a_position_never_runs_backwards_through_a_set(t in text(), p in picks(), assoc in prop::sample::select(vec![-1i8, 1])) {
            let a = set(&t, &p);
            let mut last = 0;
            for pos in t.char_indices().map(|(i, _)| i).chain([t.len()]) {
                let m = a.map_pos(pos, assoc);
                prop_assert!(m >= last && m <= a.new_len());
                last = m;
            }
        }

        #[test]
        fn a_set_is_what_its_changes_say(t in text(), p in picks()) {
            let a = set(&t, &p);
            prop_assert_eq!(ChangeSet::of(&a.changes(), t.len()).unwrap(), a);
        }
    }
}
