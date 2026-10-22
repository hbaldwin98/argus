use super::*;

#[test]
fn a_pattern_typed_is_filed_in_the_slash_register_and_made_the_search() {
    let (mut s, mut regs) = (Search::default(), Registers::default());
    s.update(&mut regs, "foo", true, true, true).unwrap();
    assert_eq!(s.query.as_ref().unwrap().source, "foo");
    assert!(s.highlight);
    assert_eq!(regs.get(Some('/')).unwrap().text, "foo");
}

#[test]
fn an_empty_pattern_leaves_the_last_search_and_the_register() {
    let (mut s, mut regs) = (Search::default(), Registers::default());
    s.update(&mut regs, "foo", true, true, true).unwrap();
    s.highlight = false;
    s.update(&mut regs, "", true, true, true).unwrap();
    assert_eq!(s.query.as_ref().unwrap().source, "foo");
    assert!(!s.highlight);
    assert_eq!(regs.get(Some('/')).unwrap().text, "foo");
}

#[test]
fn a_pattern_that_is_only_flags_is_filed_but_changes_no_search() {
    let (mut s, mut regs) = (Search::default(), Registers::default());
    s.update(&mut regs, "foo", true, true, true).unwrap();
    s.update(&mut regs, "/i", true, true, true).unwrap();
    assert_eq!(s.query.as_ref().unwrap().source, "foo");
    assert_eq!(regs.get(Some('/')).unwrap().text, "/i");
}

#[test]
fn a_pattern_that_does_not_compile_is_an_error_and_keeps_the_last_search() {
    let (mut s, mut regs) = (Search::default(), Registers::default());
    s.update(&mut regs, "foo", true, true, true).unwrap();
    assert_eq!(s.update(&mut regs, "(", true, true, true), Err("Invalid regex: (".into()));
    assert_eq!(s.query.as_ref().unwrap().source, "foo");
    assert_eq!(regs.get(Some('/')).unwrap().text, "(", "the register has what was typed");
}
