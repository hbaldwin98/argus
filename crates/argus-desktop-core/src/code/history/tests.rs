use super::*;
use crate::code::change::Change;

fn at(len: usize, from: usize, to: usize, insert: &str) -> ChangeSet {
    ChangeSet::of(&[Change { from, to, insert: insert.into() }], len).unwrap()
}

/// Makes `set` on `doc` and keeps its inverse, as an editor does.
fn make(h: &mut History, doc: &mut Rope, set: ChangeSet) {
    h.push(set.invert(doc));
    set.apply_in_place(doc);
}

#[test]
fn undo_and_redo_walk_the_units_back_and_forth() {
    let mut doc = Rope::from_str("abc");
    let mut h = History::default();
    make(&mut h, &mut doc, at(3, 3, 3, "d"));
    make(&mut h, &mut doc, at(4, 0, 0, "_"));
    h.undo(&mut doc).unwrap();
    assert_eq!(doc.to_string(), "abcd");
    h.undo(&mut doc).unwrap();
    assert_eq!(doc.to_string(), "abc");
    assert!(h.undo(&mut doc).is_none());
    h.redo(&mut doc).unwrap();
    h.redo(&mut doc).unwrap();
    assert_eq!(doc.to_string(), "_abcd");
    assert!(h.redo(&mut doc).is_none());
}

#[test]
fn a_new_unit_ends_what_could_be_redone() {
    let mut doc = Rope::from_str("abc");
    let mut h = History::default();
    make(&mut h, &mut doc, at(3, 3, 3, "d"));
    h.undo(&mut doc).unwrap();
    assert!(h.can_redo());
    make(&mut h, &mut doc, at(3, 0, 0, "x"));
    assert!(!h.can_redo());
}

#[test]
fn an_undo_never_takes_back_a_change_from_the_disk() {
    // EDITOR-VIM-10: an agent's change is not in the history.
    let mut doc = Rope::from_str("one\ntwo");
    let mut h = History::default();
    make(&mut h, &mut doc, at(7, 0, 3, "ONE"));
    // The agent writes a line at the end, which the buffer takes in.
    let remote = at(7, 7, 7, "\nthree");
    remote.apply_in_place(&mut doc);
    h.past(&remote);
    h.undo(&mut doc).unwrap();
    assert_eq!(doc.to_string(), "one\ntwo\nthree");
    h.redo(&mut doc).unwrap();
    assert_eq!(doc.to_string(), "ONE\ntwo\nthree");
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    fn edit(text: &str, a: usize, b: usize, piece: u8) -> ChangeSet {
        let mut bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        bounds.push(text.len());
        let (a, b) = (bounds[a % bounds.len()], bounds[b % bounds.len()]);
        let pieces = ["x", "yz", "\n", "é", "😀", ""];
        at(text.len(), a.min(b), a.max(b), pieces[piece as usize])
    }

    proptest! {
        // Undoing everything and redoing everything are inverses, with
        // disk changes folded in at any point between.
        #[test]
        fn undoing_all_then_redoing_all_returns_to_the_same_text(
            ops in proptest::collection::vec((0usize..32, 0usize..32, 0u8..6, any::<bool>()), 1..20)
        ) {
            let mut doc = Rope::from_str("héllo\nworld😀");
            let mut h = History::default();
            for (a, b, p, remote) in ops {
                let text = doc.to_string();
                let set = edit(&text, a, b, p);
                if remote {
                    set.apply_in_place(&mut doc);
                    h.past(&set);
                } else {
                    make(&mut h, &mut doc, set);
                }
            }
            let end = doc.to_string();
            while h.undo(&mut doc).is_some() {}
            while h.redo(&mut doc).is_some() {}
            prop_assert_eq!(doc.to_string(), end);
        }

        #[test]
        fn undoing_all_with_no_disk_change_restores_the_start(
            ops in proptest::collection::vec((0usize..32, 0usize..32, 0u8..6), 1..20)
        ) {
            let start = "héllo\nworld😀";
            let mut doc = Rope::from_str(start);
            let mut h = History::default();
            for (a, b, p) in ops {
                let text = doc.to_string();
                make(&mut h, &mut doc, edit(&text, a, b, p));
            }
            while h.undo(&mut doc).is_some() {}
            prop_assert_eq!(doc.to_string(), start);
        }
    }
}
