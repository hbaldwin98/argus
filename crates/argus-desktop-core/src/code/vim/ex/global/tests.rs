use crate::code::vim::tests::{table, Ed};

#[test]
fn the_pattern_may_be_followed_by_any_command_and_may_hold_slashes_it_escapes() {
    table(&[
        ("|a/b\nc", ":g/a\\/b/d<CR>", "|c"),
        ("|a\nb", ":g/a/s/a/x/<CR>", "|x\nb"),
        ("|ab\nb", ":g/b/s/b/x/<CR>", "ax\n|x"),
    ]);
}

#[test]
fn a_line_a_command_takes_away_is_not_run_again() {
    table(&[("|a\nb\na\nb", ":g/./d<CR>", "|"), ("|x\ny\nx", ":g/x/+1d<CR>", "|x")]);
}

#[test]
fn a_command_that_adds_lines_does_not_make_the_run_longer() {
    let mut ed = Ed::new("|a\nb\na");
    ed.keys(":g/a/norm yyp<CR>");
    assert_eq!(ed.text(), "a\na\nb\na\na");
}

#[test]
fn global_uses_the_last_pattern_when_none_is_typed() {
    table(&[("|a\nb\na", "/a<CR>:g//d<CR>", "|b")]);
}

#[test]
fn global_makes_its_pattern_the_search() {
    let mut ed = Ed::new("|a\nb");
    ed.keys(":g/b/d<CR>");
    assert_eq!(ed.sh.search.query.as_ref().unwrap().source, "b");
    assert_eq!(ed.sh.regs.get(Some('/')).unwrap().text, "b");
}

#[test]
fn a_bad_pattern_is_the_librarys_complaint() {
    let mut ed = Ed::new("|a");
    ed.keys(":g/(/d<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid regex: ("));
    assert_eq!(ed.text(), "a");
}

#[test]
fn the_colon_register_holds_the_last_command_it_ran() {
    let mut ed = Ed::new("|a\nb");
    ed.keys(":g/a/d<CR>");
    assert_eq!(ed.sh.regs.get(Some(':')).unwrap().text, "1d");
}

#[test]
fn global_in_a_read_only_file_changes_nothing() {
    let mut ed = Ed::new("|a\nb");
    ed.vim.set_read_only(true);
    ed.keys(":g/a/d<CR>");
    assert_eq!(ed.text(), "a\nb");
}
