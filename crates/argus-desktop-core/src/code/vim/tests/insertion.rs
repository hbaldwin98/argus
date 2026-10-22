//! Ways into and out of insert mode beyond the first few (EDITOR-VIM-5, -6):
//! `Ctrl-o` for one normal command, `I` and `A` on a selection, `r` with the
//! line break, and the `.` register.

use super::*;

#[test]
fn ctrl_o_in_insert_mode_runs_one_normal_command_and_returns() {
    table(&[
        ("|abc", "A<C-o>0X<Esc>", "|Xabc"),
        ("a|bc", "i<C-o>$X<Esc>", "ab|Xc"),
        ("|abc def", "i<C-o>dw<Esc>", "|def"),
        ("|abc def", "ix<C-o>wy<Esc>", "xabc |ydef"),
        ("|a\nb", "A<C-o>jX<Esc>", "a\n|Xb"),
    ]);
}

#[test]
fn ctrl_o_waits_for_a_command_that_is_half_typed() {
    let mut ed = Ed::new("|abc def");
    ed.keys("i<C-o>d");
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(ed.vim.pending().len(), 1);
    ed.keys("wX<Esc>");
    assert_eq!(ed.text(), "Xdef");
}

#[test]
fn ctrl_o_then_a_selection_stays_a_selection() {
    let mut ed = Ed::new("|abc");
    ed.keys("i<C-o>v");
    assert_eq!(ed.mode(), Mode::Visual);
    ed.keys("<Esc>");
    assert_eq!(ed.mode(), Mode::Normal);
}

#[test]
fn ctrl_o_may_stand_past_the_last_character_for_its_command() {
    // After `A` the caret is past the last character, and `Ctrl-o` leaves it there.
    table(&[("|abc", "A<C-o>iX<Esc>", "abc|X")]);
}

#[test]
fn a_command_of_ctrl_o_that_enters_insert_mode_is_not_followed_by_another_insert() {
    let mut ed = Ed::new("|abc");
    ed.keys("i<C-o>ax<Esc>");
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(ed.text(), "axbc");
}

#[test]
fn what_was_typed_before_ctrl_o_is_the_last_insert() {
    let mut ed = Ed::new("|abc");
    ed.keys("ixy<C-o>0z<Esc>");
    assert_eq!(ed.sh.regs.get(Some('.')).unwrap().text, "z");
    assert_eq!(ed.text(), "zxyabc");
}

#[test]
fn i_on_a_selection_inserts_at_the_start_of_the_selections_first_line() {
    // The library puts it at column 0 of the anchor's line, not at the selection.
    table(&[("ab|cd", "vlIX<Esc>", "|Xabcd"), ("ab|cd\nef", "VjIX<Esc>", "|Xabcd\nef")]);
}

#[test]
fn a_on_a_selection_inserts_after_its_head() {
    table(&[("ab|cd", "vlAX<Esc>", "abcd|X"), ("|abc\ndef", "VjAX<Esc>", "abc\nd|Xef")]);
}

#[test]
fn replace_with_a_line_break_breaks_the_line_and_keeps_the_indent() {
    table(&[
        ("a|bc", "r<CR>", "a\n|c"),
        ("|abcd", "2r<CR>", "\n|cd"),
        ("  a|b", "r<CR>", "  a\n | "),
        ("|abc", "5r<CR>", "|abc"),
    ]);
}

#[test]
fn the_dot_register_holds_the_text_of_the_last_insert() {
    let mut ed = Ed::new("|abc");
    ed.keys("ixy<Esc>");
    assert_eq!(ed.sh.regs.get(Some('.')).unwrap().text, "xy");
    ed.keys("a<C-r>.<Esc>");
    assert_eq!(ed.text(), "xyxyabc");
}

#[test]
fn gi_goes_back_to_where_the_last_edit_ended() {
    table(&[("|abc\ndef", "ixy<Esc>jgiZ<Esc>", "xy|Zabc\ndef"), ("|abc\ndef", "xjgiZ<Esc>", "|Zbc\ndef")]);
}
