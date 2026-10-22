use crate::code::vim::tests::Ed;

#[test]
fn the_register_listing_has_each_register_with_text_and_the_numbered_ones_first() {
    let mut ed = Ed::new("|one\ntwo");
    ed.keys("yyjdd\"byl:reg<CR>");
    let say = ed.vim.message().unwrap().to_owned();
    let find = |needle: &str| say.find(needle).unwrap_or_else(|| panic!("{needle:?} in {say:?}"));
    assert!(find("\"0") < find("\"1"), "numbers by number");
    assert!(find("\"1") < find("\"b"), "then the rest");
    assert!(!say.contains("\"c "), "a register with no text is not listed: {say}");
}

#[test]
fn registers_named_after_it_are_shown_even_when_empty_and_unknown_names_are_skipped() {
    let mut ed = Ed::new("|a");
    ed.keys(":reg z<CR>");
    assert!(ed.vim.message().unwrap().contains("\"z    \n"));
    ed.keys(":reg !<CR>");
    assert_eq!(ed.vim.message().unwrap(), "----------Registers----------\n\n");
}

#[test]
fn the_colon_and_slash_registers_are_listed() {
    let mut ed = Ed::new("|a foo");
    ed.keys("/foo<CR>:reg<CR>");
    let say = ed.vim.message().unwrap();
    assert!(say.contains("\"/    foo\n"), "{say}");
}

#[test]
fn marks_lists_the_marks_set_with_line_and_column_and_none_with_none() {
    let mut ed = Ed::new("|a");
    ed.keys(":marks<CR>");
    assert_eq!(ed.vim.message().unwrap(), "-----------Marks-----------\nmark\tline\tcol\n\n");
    ed.keys("ma:marks<CR>");
    assert!(ed.vim.message().unwrap().ends_with("a\t0\t0\n"));
}

#[test]
fn marks_after_a_name_shows_only_that_mark_and_a_selection_marks_show_by_their_angle() {
    let mut ed = Ed::new("|ab\ncd");
    ed.keys("mavj<Esc>:marks<CR>");
    let say = ed.vim.message().unwrap().to_owned();
    assert!(say.contains("<\t0\t0") && say.contains(">\t1\t0") && say.contains("a\t0\t0"), "{say}");
    ed.keys(":marks <<CR>");
    let say = ed.vim.message().unwrap();
    assert!(say.contains("<\t0\t0") && !say.contains("a\t0\t0"), "{say}");
}
