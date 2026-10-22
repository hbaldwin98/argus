use super::*;

fn rope(text: &str) -> Rope {
    Rope::from_str(text)
}

#[test]
fn the_same_text_is_no_change() {
    assert_eq!(found(&rope("a\nb"), &rope("a\nb")), None);
    assert_eq!(found(&rope(""), &rope("")), None);
}

#[test]
fn a_letter_typed_is_an_insertion_where_it_was_typed() {
    assert_eq!(found(&rope("fn f() {}"), &rope("fn fx() {}")), Some(Change { from: 4, to: 4, insert: "x".into() }));
}

#[test]
fn a_range_deleted_and_one_replaced_are_found_whole() {
    assert_eq!(found(&rope("hello world"), &rope("hello")), Some(Change { from: 5, to: 11, insert: String::new() }));
    assert_eq!(found(&rope("one two"), &rope("one 2")), Some(Change { from: 4, to: 7, insert: "2".into() }));
}

#[test]
fn repeated_text_is_kept_at_the_start_so_the_new_letter_is_at_the_end() {
    assert_eq!(found(&rope("aaa"), &rope("aaaa")), Some(Change { from: 3, to: 3, insert: "a".into() }));
}

#[test]
fn a_character_that_shares_its_first_bytes_is_replaced_whole() {
    // U+00E9 and U+00E8 share the byte 0xC3.
    let change = found(&rope("caf\u{e9}!"), &rope("caf\u{e8}!")).unwrap();
    assert_eq!(change, Change { from: 3, to: 5, insert: "\u{e8}".into() });
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    // `ts:code/sync.ts` `between` and EDITOR-SYNC-13: the change found turns the text before into
    // the text after, on character boundaries, whatever the two are.
    proptest! {
        #[test]
        fn the_change_found_turns_the_old_text_into_the_new(a in "[a\u{e9}\u{e8}\u{1f600}\n ]{0,24}", b in "[a\u{e9}\u{e8}\u{1f600}\n ]{0,24}") {
            let (old, new) = (rope(&a), rope(&b));
            match set(&old, &new) {
                None => prop_assert_eq!(a, b),
                Some(set) => {
                    let change = &set.changes()[0];
                    prop_assert!(a.is_char_boundary(change.from) && a.is_char_boundary(change.to));
                    prop_assert_eq!(set.apply(&old).to_string(), b);
                }
            }
        }
    }
}
