use crate::code::vim::tests::{table, Ed};

#[test]
fn delete_takes_the_lines_named_into_a_register_and_leaves_the_cursor_on_the_next() {
    let mut ed = Ed::new("a\nb\nc\nd");
    ed.keys(":2,3d x<CR>");
    assert_eq!(ed.text(), "a\nd");
    assert_eq!(ed.sh.regs.get(Some('x')).unwrap().text, "b\nc\n");
    assert_eq!(ed.shows(), "a\n|d");
}

#[test]
fn delete_with_a_selection_takes_its_lines() {
    table(&[("|a\nb\nc", "Vj:d<CR>", "|c")]);
}

#[test]
fn yank_leaves_the_cursor_and_the_text() {
    let mut ed = Ed::new("a\n|b\nc");
    ed.keys(":1,2y<CR>");
    assert_eq!(ed.shows(), "a\n|b\nc");
    assert_eq!(ed.sh.regs.get(None).unwrap().text, "a\nb\n");
    assert_eq!(ed.sh.regs.get(Some('0')).unwrap().text, "a\nb\n");
}

#[test]
fn put_puts_the_register_as_lines_below_the_last_line_named_or_above_the_first_with_a_bang() {
    table(&[
        ("|a\nb\nc", "yy:3pu<CR>", "a\nb\nc\n|a"),
        ("|a\nb\nc", "yy:2,3pu<CR>", "a\nb\nc\n|a"),
        ("|a\nb\nc", "jyy:1pu!<CR>", "|b\na\nb\nc"),
        ("|a\nb", "\"ayy:2pu a<CR>", "a\nb\n|a"),
        ("|abc", "yl:pu<CR>", "abc\n|a"),
    ]);
}

#[test]
fn put_of_an_empty_register_does_nothing() {
    table(&[("|a", ":pu<CR>", "|a"), ("|a", ":pu z<CR>", "|a")]);
}

#[test]
fn join_takes_the_range_or_the_line_and_the_next() {
    table(&[
        ("|a\nb\nc", ":j<CR>", "a| b\nc"),
        ("|a\nb\nc", ":1,3j<CR>", "a b| c"),
        ("|a\n  b", ":j<CR>", "a| b"),
    ]);
}

#[test]
fn a_read_only_file_is_not_changed() {
    let mut ed = Ed::new("|a\nb");
    ed.vim.set_read_only(true);
    ed.keys(":d<CR>:j<CR>:%pu<CR>");
    assert_eq!(ed.text(), "a\nb");
}
