use crate::code::vim::tests::{table, Ed};
use crate::code::vim::Mode;

#[test]
fn v_toggles_and_the_selection_modes_swap() {
    let mut ed = Ed::new("|abc");
    ed.keys("v");
    assert_eq!(ed.mode(), Mode::Visual);
    ed.keys("V");
    assert_eq!(ed.mode(), Mode::VisualLine);
    ed.keys("<C-v>");
    assert_eq!(ed.mode(), Mode::VisualBlock);
    ed.keys("v");
    assert_eq!(ed.mode(), Mode::Visual);
    ed.keys("v");
    assert_eq!(ed.mode(), Mode::Normal);
}

#[test]
fn the_anchor_stays_where_the_selection_began_through_a_mode_switch() {
    let mut ed = Ed::new("a|bcd");
    ed.keys("lvV");
    assert_eq!(ed.vim.selection(), Some((0, 4)), "the lines");
    ed.keys("v");
    assert_eq!(ed.vim.selection(), Some((2, 3)));
}

#[test]
fn an_object_is_taken_into_the_selection_and_the_same_object_again_is_the_same() {
    let mut ed = Ed::new("a (b (c) d) e");
    ed.vim.place(6);
    ed.keys("vi(");
    assert_eq!(ed.vim.selection(), Some((6, 7)));
    ed.keys("i(");
    assert_eq!(ed.vim.selection(), Some((6, 7)));
    ed.keys("a(");
    assert_eq!(ed.vim.selection(), Some((5, 8)), "the pair with its brackets takes in what was selected");
}

#[test]
fn an_object_that_is_lines_makes_the_selection_lines() {
    let mut ed = Ed::new("a\nb\n\nc");
    ed.keys("vip");
    assert_eq!(ed.mode(), Mode::VisualLine);
}

#[test]
fn an_object_in_a_block_makes_it_a_character_selection() {
    let mut ed = Ed::new("|abc def");
    ed.keys("<C-v>iw");
    assert_eq!(ed.mode(), Mode::Visual);
    assert_eq!(ed.vim.selection(), Some((0, 3)));
}

#[test]
fn o_goes_to_the_other_end_and_the_selection_is_the_same() {
    let mut ed = Ed::new("|abcd");
    ed.keys("vll");
    assert_eq!(ed.vim.selection(), Some((0, 3)));
    ed.keys("o");
    assert_eq!(ed.vim.cursor(), 0);
    assert_eq!(ed.vim.selection(), Some((0, 3)));
}

#[test]
fn s_and_r_change_the_selections_lines_and_c_a_blocks_rows_to_their_ends() {
    table(&[
        ("|abc\ndef", "vlSX<Esc>", "|X\ndef"),
        ("|abc\ndef", "vlRX<Esc>", "|X\ndef"),
        ("|abc\ndef\nghi", "<C-v>jSX<Esc>", "|X\nghi"),
        ("a|bc\ndef", "<C-v>jCX<Esc>", "a|X\ndX"),
    ]);
}

#[test]
fn a_selection_taken_in_the_text_that_then_shrinks_stays_in_the_text() {
    let mut ed = Ed::new("|abc");
    ed.keys("v$");
    let (from, to) = ed.vim.selection().unwrap();
    assert!(from <= to && to <= 3);
}

#[test]
fn a_command_of_the_visual_mode_that_is_none_leaves_the_selection() {
    let mut ed = Ed::new("|abc");
    ed.keys("vlZ");
    assert_eq!(ed.mode(), Mode::Visual);
}
