//! The operators the library has beyond the first few (EDITOR-VIM-9): `g?`,
//! `gq`, `gw`, `gc` and `=`, and `]p` and `[p`.

use super::*;

#[test]
fn g_question_mark_is_rot13() {
    table(&[
        ("|Hello world", "g?w", "|Uryyb world"),
        ("|Hello world", "g??", "|Uryyb jbeyq"),
        ("|Hello world", "g?g?", "|Uryyb jbeyq"),
        ("|Hello world", "veg?", "|Uryyb world"),
        ("|Hello\nabc", "Vjg?", "|Uryyb\nnop"),
        ("|Uryyb", "g??", "|Hello"),
        ("|é1-z", "g??", "|é1-m"),
    ]);
}

#[test]
fn gq_wraps_lines_to_the_text_width_and_leaves_the_cursor_on_the_last() {
    let mut ed = Ed::new("|aaa bbb ccc ddd eee");
    ed.keys(":set tw=10<CR>gqq");
    assert_eq!(ed.shows(), "aaa bbb\nccc ddd\n|eee");
}

#[test]
fn gq_joins_short_lines_and_keeps_an_indent_on_the_wrapped_part() {
    let mut ed = Ed::new("|aaa\nbbb");
    ed.keys("gqj");
    assert_eq!(ed.shows(), "|aaa bbb");
    let mut ed = Ed::new("|  aaa bbb ccc ddd");
    ed.keys(":set tw=10<CR>gqq");
    assert_eq!(ed.text(), "  aaa bbb\n  ccc ddd");
}

#[test]
fn gw_wraps_and_keeps_the_cursor_where_it_was() {
    let mut ed = Ed::new("aaa |bbb ccc ddd eee");
    ed.keys(":set tw=10<CR>gww");
    assert_eq!(ed.text(), "aaa bbb\nccc ddd\neee");
    assert_eq!(ed.vim.cursor(), 4, "at the line and column it had");
}

#[test]
fn the_text_width_is_for_this_editor_and_eighty_to_begin_with() {
    let long = "word ".repeat(30);
    let mut ed = Ed::new(&format!("|{}", long.trim_end()));
    ed.keys("gqq");
    let text = ed.text();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.len() > 1 && lines.iter().all(|l| l.chars().count() <= 80), "{lines:?}");
    ed.keys(":set tw?<CR>");
    assert_eq!(ed.vim.message(), Some("  tw=80"));
}

#[test]
fn gc_toggles_line_comments_with_the_token_the_file_has() {
    let mut ed = Ed::new("|a\nb");
    ed.vim.set_comment_token(Some("//"));
    ed.keys("gcj");
    assert_eq!(ed.shows(), "|// a\n// b");
    ed.keys("gcj");
    assert_eq!(ed.shows(), "|a\nb");
    ed.keys("gcc");
    assert_eq!(ed.shows(), "|// a\nb");
}

#[test]
fn gc_comments_at_the_common_indent_and_skips_blank_lines() {
    let mut ed = Ed::new("|  a\n\n    b");
    ed.vim.set_comment_token(Some("#"));
    ed.keys("gcG");
    assert_eq!(ed.text(), "  # a\n\n  #   b");
}

#[test]
fn gc_changes_nothing_when_the_file_has_no_comment_token() {
    // The page's editor loads no language, so there is none to find.
    table(&[("|a\nb", "gcj", "|a\nb"), ("|a\nb", "Vjgc", "|a\nb")]);
}

#[test]
fn equals_has_no_indent_rules_and_moves_the_cursor_to_the_first_non_blank() {
    table(&[("  a|b", "==", "  |ab"), ("  a|b\n c", "=j", "  |ab\n c"), ("  a|b", "v=", "  |ab")]);
}

#[test]
fn a_put_with_a_bracket_fits_the_indent_to_the_line() {
    table(&[
        ("  a\nb", "jyyk]p", "  a\n  |b\nb"),
        ("  a\nb", "jyyk[p", "  |b\n  a\nb"),
        ("a\n    b", "jyyk]p", "a\n|b\n    b"),
    ]);
}

#[test]
fn a_bracket_put_of_text_with_its_own_indent_keeps_the_difference() {
    let mut ed = Ed::new("|a");
    ed.sh.regs.yank(None, crate::code::vim::registers::Reg::new("    x\n      y", true));
    ed.keys("]p");
    assert_eq!(ed.shows(), "a\n|x\n  y");
}
