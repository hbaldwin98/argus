use crate::code::vim::tests::{table, Ed};
use crate::code::vim::{Effect, Mode};

#[test]
fn write_asks_the_host_to_write_once_per_command() {
    let mut ed = Ed::new("|abc");
    ed.keys(":w<CR>:w<CR>");
    assert_eq!(ed.effects, vec![Effect::Write, Effect::Write]);
}

#[test]
fn start_insert_enters_insert_mode_at_the_cursor_and_with_a_bang_at_the_end() {
    let mut ed = Ed::new("a|bc");
    ed.keys(":startinsert<CR>X<Esc>");
    assert_eq!(ed.text(), "aXbc");
    let mut ed = Ed::new("a|bc");
    ed.keys(":start!<CR>X<Esc>");
    assert_eq!(ed.text(), "abcX");
}

#[test]
fn start_insert_in_a_read_only_file_does_not_enter_it() {
    let mut ed = Ed::new("|abc");
    ed.vim.set_read_only(true);
    ed.keys(":startinsert<CR>");
    assert_eq!(ed.mode(), Mode::Normal);
}

#[test]
fn a_line_number_goes_to_the_first_non_blank_and_none_is_beyond_the_text() {
    table(&[
        ("|a\n  b\nc", ":2<CR>", "a\n  |b\nc"),
        ("|a\n  b\nc", ":0<CR>", "|a\n  b\nc"),
        ("|a\n  b\nc", ":99<CR>", "a\n  b\n|c"),
        ("|a\n   \nc", ":2<CR>", "a\n  | \nc"),
        ("a\nb|\nc", ":+1<CR>", "a\nb\n|c"),
    ]);
}

#[test]
fn a_line_number_is_no_jump() {
    let mut ed = Ed::new("|a\nb\nc");
    ed.keys(":3<CR>");
    ed.keys("<C-o>");
    assert_eq!(ed.shows(), "a\nb\n|c", "the jump list has nothing");
}

#[test]
fn undo_and_redo_by_name() {
    table(&[("|abc", "xx:undo<CR>", "|bc"), ("|abc", "xu:redo<CR>", "|bc"), ("|abc", "xx:u<CR>:u<CR>", "|abc")]);
}

#[test]
fn nohlsearch_clears_the_marks() {
    let mut ed = Ed::new("|a b");
    ed.keys("/b<CR>");
    assert!(ed.vim.highlight(&ed.sh).is_some());
    ed.keys(":nohlsearch<CR>");
    assert!(ed.vim.highlight(&ed.sh).is_none());
}
