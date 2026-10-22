use super::*;
use crate::code::vim::tests::{table, Ed};

#[test]
fn the_width_of_an_indent_counts_a_tab_as_four() {
    assert_eq!(indent_of(""), 0);
    assert_eq!(indent_of("  a"), 2);
    assert_eq!(indent_of("\t a"), 5);
    assert_eq!(indent_of("a  "), 0);
}

#[test]
fn text_is_moved_to_an_indent_with_its_own_differences_kept() {
    assert_eq!(matched("x\n  y\n", 2), "  x\n    y\n");
    assert_eq!(matched("    x\n      y\n", 0), "x\n  y\n");
    assert_eq!(matched("    x\n  y", 0), "x\ny", "not before the start of the line");
    assert_eq!(matched("x", 4), "    x");
}

#[test]
fn a_blank_line_stays_blank_when_text_is_moved() {
    assert_eq!(matched("x\n\ny\n", 2), "  x\n\n  y\n");
    assert_eq!(matched("x\n   \ny", 2), "  x\n\n  y");
}

#[test]
fn a_charwise_paste_goes_after_the_cursor_or_before_and_a_count_repeats_it() {
    table(&[
        ("|abc", "ylp", "a|abc"),
        ("|abc", "ylP", "|aabc"),
        ("|abc", "yl3p", "aaa|abc"),
        ("|abc", "yw$p", "abcab|c"),
    ]);
}

#[test]
fn a_linewise_paste_goes_on_lines_of_its_own() {
    table(&[
        ("|a\nb", "yyp", "a\n|a\nb"),
        ("|a\nb", "yyP", "|a\na\nb"),
        ("a\n|b", "yyp", "a\nb\n|b"),
        ("|a\nb", "yy2p", "a\n|a\na\nb"),
    ]);
}

#[test]
fn a_paste_with_nothing_in_the_register_does_nothing() {
    table(&[("|abc", "p", "|abc"), ("|abc", "\"zp", "|abc"), ("|abc", "\"_p", "|abc")]);
}

#[test]
fn a_paste_in_a_read_only_file_does_nothing() {
    let mut ed = Ed::new("|abc");
    ed.keys("yl");
    ed.vim.set_read_only(true);
    ed.keys("p");
    assert_eq!(ed.text(), "abc");
}

#[test]
fn p_over_a_selection_takes_what_it_replaced_and_a_capital_p_does_not() {
    let mut ed = Ed::new("|abc def");
    ed.keys("yiwwviwp");
    assert_eq!(ed.text(), "abc abc");
    assert_eq!(ed.sh.regs.get(None).unwrap().text, "def");
    let mut ed = Ed::new("|abc def");
    ed.keys("yiwwviwP");
    assert_eq!(ed.text(), "abc abc");
    assert_eq!(ed.sh.regs.get(None).unwrap().text, "abc");
}

#[test]
fn lines_pasted_over_a_selection_of_characters_take_a_line_each_side() {
    let mut ed = Ed::new("|a b c\nx");
    ed.keys("jyyk0wviwp");
    assert_eq!(ed.text(), "a \nx\n c\nx");
}

#[test]
fn the_rows_of_a_block_put_one_under_another_are_padded_to_the_column() {
    let mut ed = Ed::new("|ab\n\ncd");
    ed.keys("<C-v>jjy$p");
    assert_eq!(ed.text(), "aba\n   \ncdc");
}
