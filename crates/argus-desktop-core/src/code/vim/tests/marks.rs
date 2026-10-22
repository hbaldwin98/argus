//! Marks and the jump list (EDITOR-VIM-7): `m`, `` ` ``, `'`, `]'`, `['`,
//! `Ctrl-o`, `Ctrl-i`, the special marks, and how marks follow edits.

use super::*;

#[test]
fn a_mark_is_gone_back_to_by_backtick_and_quote() {
    table(&[
        ("ab|c", "ma0`a", "ab|c"),
        ("x\n  b|c\nz", "magg'a", "x\n  |bc\nz"),
        ("x\n  b|c\nz", "magg`a", "x\n  b|c\nz"),
        ("a|b", "mbll`b", "a|b"),
        ("a|b", "mA0`A", "a|b"),
    ]);
}

#[test]
fn a_mark_that_was_never_set_goes_nowhere() {
    table(&[("a|b", "`z", "a|b"), ("a|b", "'z", "a|b")]);
}

#[test]
fn a_mark_follows_the_text_around_it() {
    table(&[
        ("a|bc", "ma0ix<Esc>`a", "xa|bc"),
        // An insertion at the mark leaves it before the new text.
        ("a|bc", "mai<Esc>`a", "a|bc"),
        ("a|bc", "maix<Esc>`a", "a|xbc"),
        ("|abc", "mallx0`a", "|ab"),
        // The mark stays at the start of what replaced the text it was on.
        ("|abc", "max`a", "|bc"),
    ]);
}

#[test]
fn a_mark_inside_deleted_text_is_gone() {
    // The library's bookmark is taken away when a replaced run holds it.
    table(&[("a|bcd", "ma0d3l$`a", "|d")]);
}

#[test]
fn an_operator_runs_to_a_mark() {
    table(&[
        ("a\nb\nc\nd", "jmajjd'a", "|a"),
        ("a\nb\nc\nd", "jmaGd'a", "|a"),
        ("abcdef", "lmallld`a", "a|ef"),
        ("abcdef", "llmahhy`aP", "a|babcdef"),
        ("ab|cd", "ma0y`aP", "a|babcd"),
    ]);
}

#[test]
fn the_jump_list_goes_back_and_forward() {
    table(&[
        ("|a\nb\nc\nd", "G<C-o>", "|a\nb\nc\nd"),
        ("|a\nb\nc\nd", "G<C-o><C-i>", "a\nb\nc\n|d"),
        ("|a\nb\nc\nd\ne", "Ggg<C-o>", "a\nb\nc\nd\n|e"),
        ("|a\nb\nc\nd\ne", "Ggg<C-o><C-o>", "|a\nb\nc\nd\ne"),
        ("|a\nb\nc\nd\ne", "Ggg<C-o><C-o><C-i>", "a\nb\nc\nd\n|e"),
        ("|a\nb\nc", "2<C-o>", "|a\nb\nc"),
    ]);
}

#[test]
fn quote_quote_goes_to_where_the_last_jump_began_and_back() {
    table(&[
        ("|a\nb\nc", "G''", "|a\nb\nc"),
        ("|a\nb\nc", "G''''", "a\nb\n|c"),
        ("|a\n  b\nc", "jG``", "a\n|  b\nc"),
    ]);
}

#[test]
fn only_the_jump_motions_are_put_in_the_list() {
    table(&[("|a b c d", "www<C-o>", "a b c |d")]);
}

#[test]
fn a_search_is_a_jump() {
    table(&[("|a foo b", "/foo<CR><C-o>", "|a foo b"), ("a |foo\nb foo", "*<C-o>", "a |foo\nb foo")]);
}

#[test]
fn a_jump_list_place_is_followed_through_edits() {
    table(&[("|a\nb\nc", "GggOx<Esc>j<C-o>", "x\na\nb\n|c")]);
}

#[test]
fn quote_bracket_moves_to_the_next_and_last_lowercase_mark() {
    table(&[
        ("|a\nb\nc\nd", "jmajjmbgg]'", "a\n|b\nc\nd"),
        ("|a\nb\nc\nd", "jmajjmbgg]']'", "a\nb\nc\n|d"),
        ("|a\nb\nc\nd", "jmajjmbG['", "a\n|b\nc\nd"),
    ]);
}

#[test]
fn the_ends_of_the_last_selection_are_marks() {
    table(&[
        ("|abc def", "vee<Esc>0`>", "abc de|f"),
        ("|abc def", "ve<Esc>$`<", "|abc def"),
        ("|abc\ndef", "Vj<Esc>gg:'<,'>d<CR>", "|"),
    ]);
}

#[test]
fn the_dot_mark_is_where_the_last_edit_ended() {
    table(&[("a|bc", "x0`.", "a|c"), ("|abc", "ixy<Esc>$`.", "xy|abc")]);
}

#[test]
fn marks_lists_them() {
    let mut ed = Ed::new("a|b\ncd");
    ed.keys("majmb:marks<CR>");
    let say = ed.vim.message().unwrap().to_owned();
    assert!(say.starts_with("-----------Marks-----------\nmark\tline\tcol\n\n"));
    assert!(say.contains("a\t0\t1\n"), "{say}");
    assert!(say.contains("b\t1\t1\n"), "{say}");
    ed.keys(":marks a<CR>");
    let say = ed.vim.message().unwrap();
    assert!(say.contains("a\t0\t1") && !say.contains("b\t1\t1"), "{say}");
}

#[test]
fn delmarks_removes_letters_and_ranges() {
    let mut ed = Ed::new("a|b");
    ed.keys("mamb mcmD".replace(' ', "").as_str());
    ed.keys(":delm a<CR>:marks<CR>");
    let say = ed.vim.message().unwrap().to_owned();
    assert!(!say.contains("\na\t") && say.contains("\nb\t") && say.contains("\nc\t"), "{say}");
    ed.keys(":delm b-c<CR>:marks<CR>");
    let say = ed.vim.message().unwrap().to_owned();
    assert!(!say.contains("\nb\t") && !say.contains("\nc\t") && say.contains("\nD\t"), "{say}");
    ed.keys(":delm<CR>");
    assert_eq!(ed.vim.message(), Some("Argument required"));
    ed.keys(":delm 1<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid argument: 1"));
    ed.keys(":delm d-a<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid argument: d-a"));
    ed.keys(":delm a-Z<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid argument: a-"));
}

#[test]
fn gv_selects_the_last_selection_again() {
    let mut ed = Ed::new("|abc def");
    ed.keys("ve<Esc>$gv");
    assert_eq!(ed.mode(), Mode::Visual);
    assert_eq!(ed.vim.selection(), Some((0, 3)));
}

#[test]
fn gv_in_a_selection_swaps_it_with_the_last_one() {
    let mut ed = Ed::new("|abc def");
    ed.keys("vl<Esc>wvl");
    assert_eq!(ed.vim.selection(), Some((4, 6)));
    ed.keys("gv");
    assert_eq!(ed.vim.selection(), Some((0, 2)));
    ed.keys("gv");
    assert_eq!(ed.vim.selection(), Some((4, 6)));
}

#[test]
fn gv_does_nothing_if_the_text_edited_its_ends_away() {
    table(&[("|abc def", "ve<Esc>0dwgv", "|def")]);
}

#[test]
fn gv_remembers_a_line_and_a_block_selection() {
    let mut ed = Ed::new("|ab\ncd\nef");
    ed.keys("Vj<Esc>gg");
    ed.keys("gv");
    assert_eq!(ed.mode(), Mode::VisualLine);
    ed.keys("<Esc><C-v>jl<Esc>gg0gv");
    assert_eq!(ed.mode(), Mode::VisualBlock);
}
