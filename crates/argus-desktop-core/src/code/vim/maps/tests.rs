use super::*;
use crate::code::vim::key::parse;

fn full(maps: &Maps, keys: &str, mode: Mode) -> Option<Vec<Key>> {
    match maps.lookup(&parse(keys), mode) {
        Hit::Full(m) => Some(m.rhs.clone()),
        _ => None,
    }
}

#[test]
fn a_mapping_is_found_by_exactly_its_keys_and_a_prefix_is_partial() {
    let mut maps = Maps::default();
    maps.add("gx", "dd", None, false).unwrap();
    assert_eq!(full(&maps, "gx", Mode::Normal), Some(parse("dd")));
    assert_eq!(maps.lookup(&parse("g"), Mode::Normal), Hit::Partial);
    assert_eq!(maps.lookup(&parse("gy"), Mode::Normal), Hit::None);
    assert_eq!(maps.lookup(&parse("gxx"), Mode::Normal), Hit::None);
}

#[test]
fn a_mapping_with_no_mode_is_for_every_mode_but_insert() {
    let mut maps = Maps::default();
    maps.add("q", "w", None, false).unwrap();
    for mode in [Mode::Normal, Mode::Visual, Mode::OperatorPending] {
        assert!(full(&maps, "q", mode).is_some(), "{mode:?}");
    }
    assert!(full(&maps, "q", Mode::Insert).is_none());
}

#[test]
fn a_mapping_with_a_mode_is_for_that_mode_only() {
    let mut maps = Maps::default();
    maps.add("jk", "<Esc>", Some(Mode::Insert), true).unwrap();
    assert!(full(&maps, "jk", Mode::Insert).is_some());
    assert!(full(&maps, "jk", Mode::Normal).is_none());
}

#[test]
fn the_newest_mapping_wins() {
    let mut maps = Maps::default();
    maps.add("x", "a", None, false).unwrap();
    maps.add("x", "b", None, false).unwrap();
    assert_eq!(full(&maps, "x", Mode::Normal), Some(parse("b")));
}

#[test]
fn each_mapping_has_its_own_id_and_a_noremap_says_so() {
    let mut maps = Maps::default();
    maps.add("a", "b", None, true).unwrap();
    maps.add("c", "d", None, false).unwrap();
    let (Hit::Full(a), Hit::Full(c)) = (maps.lookup(&parse("a"), Mode::Normal), maps.lookup(&parse("c"), Mode::Normal)) else {
        panic!("both are mappings");
    };
    assert_ne!(a.id, c.id);
    assert!(a.noremap && !c.noremap);
}

#[test]
fn unmap_removes_the_mapping_of_that_mode() {
    let mut maps = Maps::default();
    maps.add("x", "a", None, false).unwrap();
    maps.add("x", "b", Some(Mode::Normal), false).unwrap();
    assert_eq!(maps.remove("x", None), Ok(true));
    assert_eq!(full(&maps, "x", Mode::Normal), Some(parse("b")), "the one with a mode is left");
    assert_eq!(maps.remove("x", None), Ok(false));
    assert_eq!(maps.remove("x", Some(Mode::Normal)), Ok(true));
}

#[test]
fn clearing_a_mode_keeps_the_others_and_copies_a_mapping_with_no_mode_to_them() {
    let mut maps = Maps::default();
    maps.add("a", "b", Some(Mode::Normal), false).unwrap();
    maps.add("c", "d", Some(Mode::Visual), false).unwrap();
    maps.add("e", "f", None, false).unwrap();
    maps.clear(Some(Mode::Normal));
    assert!(full(&maps, "a", Mode::Normal).is_none());
    assert!(full(&maps, "c", Mode::Visual).is_some());
    // As the library does it: for each of normal, insert and visual, but the one cleared.
    assert!(full(&maps, "e", Mode::Normal).is_none());
    assert!(full(&maps, "e", Mode::Insert).is_some());
    assert!(full(&maps, "e", Mode::Visual).is_some());
    maps.clear(None);
    assert!(full(&maps, "c", Mode::Visual).is_none());
}

#[test]
fn a_command_name_may_be_mapped_but_not_in_a_mode() {
    let mut maps = Maps::default();
    maps.add(":w2", ":w", None, false).unwrap();
    maps.add(":go", "l", None, false).unwrap();
    assert_eq!(maps.ex("w2"), Some(&ExMap::Command("w".into())));
    assert_eq!(maps.ex("go"), Some(&ExMap::Keys("l".into())));
    assert!(maps.add(":x", ":y", Some(Mode::Normal), false).is_err());
    assert_eq!(maps.remove(":w2", None), Ok(true));
    assert_eq!(maps.ex("w2"), None);
}

#[test]
fn a_command_mapped_again_is_mapped_to_the_new_one() {
    let mut maps = Maps::default();
    maps.add(":a", ":b", None, false).unwrap();
    maps.add(":a", ":c", None, false).unwrap();
    assert_eq!(maps.ex("a"), Some(&ExMap::Command("c".into())));
}
