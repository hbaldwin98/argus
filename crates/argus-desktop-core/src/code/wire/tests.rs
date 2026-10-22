use super::*;

fn wire(from: u32, to: u32, insert: &str) -> TextChange {
    TextChange { from, to, insert: insert.into() }
}

#[test]
fn positions_count_utf16_units_where_the_rope_counts_bytes() {
    // "é" is two bytes and one unit; the emoji four bytes and two.
    let doc = Rope::from_str("é😀x");
    let set = from_wire(&[wire(3, 4, "y")], &doc).unwrap();
    assert_eq!(set.changes(), vec![Change { from: 6, to: 7, insert: "y".into() }]);
    assert_eq!(set.apply(&doc).to_string(), "é😀y");
    assert_eq!(to_wire(&set, &doc), vec![wire(3, 4, "y")]);
}

#[test]
fn a_position_between_a_surrogate_pair_is_refused() {
    let doc = Rope::from_str("a😀");
    assert_eq!(from_wire(&[wire(2, 2, "x")], &doc), Err(Refused::Splits(2)));
    assert!(from_wire(&[wire(1, 3, "")], &doc).is_ok());
}

#[test]
fn a_position_past_the_end_is_refused() {
    let doc = Rope::from_str("ab");
    assert_eq!(from_wire(&[wire(1, 3, "")], &doc), Err(Refused::Past(3)));
}

#[test]
fn changes_out_of_order_are_refused() {
    let doc = Rope::from_str("abcd");
    assert_eq!(
        from_wire(&[wire(2, 3, ""), wire(0, 1, "")], &doc),
        Err(Refused::Misfit(Misfit::Order { from: 0 }))
    );
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        // The wire form is the protocol's contract (`proto:code/edit.rs`):
        // a set must cross it and come back the same set.
        #[test]
        fn a_set_crosses_the_wire_and_back_unchanged(
            text in "[ab\né😀]{0,12}",
            picks in proptest::collection::vec((0usize..32, 0usize..32), 0..4),
        ) {
            let doc = Rope::from_str(&text);
            let mut bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
            bounds.push(text.len());
            let mut changes: Vec<Change> = picks
                .iter()
                .map(|&(a, b)| {
                    let (a, b) = (bounds[a % bounds.len()], bounds[b % bounds.len()]);
                    Change { from: a.min(b), to: a.max(b), insert: "é😀\n".into() }
                })
                .collect();
            changes.sort_by_key(|c| (c.from, c.to));
            let mut kept: Vec<Change> = Vec::new();
            for c in changes {
                if kept.last().is_none_or(|l| l.to <= c.from) {
                    kept.push(c);
                }
            }
            let set = ChangeSet::of(&kept, text.len()).unwrap();
            let sent = to_wire(&set, &doc);
            prop_assert_eq!(from_wire(&sent, &doc).unwrap(), set);
        }
    }
}
