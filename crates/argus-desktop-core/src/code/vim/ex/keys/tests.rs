use crate::code::vim::tests::{table, Ed};
use crate::code::vim::Mode;

#[test]
fn the_keys_run_from_the_start_of_each_line_of_the_range() {
    table(&[("a\nb\nc", ":%norm Ax<CR>", "ax\nbx\nc|x"), ("  a\n  b", ":%norm 0iZ<CR>", "Z  a\nZ|  b")]);
}

#[test]
fn with_no_range_they_run_where_the_cursor_is() {
    table(&[("a|bc", ":norm x<CR>", "a|c"), ("a|bc", ":norm! D<CR>", "|a")]);
}

#[test]
fn an_insert_they_leave_open_ends_there() {
    let mut ed = Ed::new("|a\nb");
    ed.keys(":%norm Ax<CR>");
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(ed.text(), "ax\nbx");
}

#[test]
fn a_selection_they_leave_open_stays() {
    let mut ed = Ed::new("|abc");
    ed.keys(":norm v<CR>");
    assert_eq!(ed.mode(), Mode::Visual);
}

#[test]
fn a_range_past_the_text_runs_on_the_last_line_only_once() {
    table(&[("a\nb", ":1,9norm Ax<CR>", "ax\nb|x")]);
}

#[test]
fn a_blank_before_the_keys_is_taken_off_and_a_bang_means_no_mappings() {
    let mut ed = Ed::new("|abc");
    ed.keys(":nmap x dd<CR>");
    ed.keys(":norm   ! x<CR>");
    assert_eq!(ed.text(), "");
}
