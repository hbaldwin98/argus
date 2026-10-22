//! Visual block mode (EDITOR-VIM-5, -9): the rectangle, what an operator
//! does to it, what is typed after `I`, `A` and `c`, and a block as a
//! register.

use super::*;

const T: &str = "|abc\ndef\nghi";

#[test]
fn ctrl_v_starts_a_block_and_ctrl_q_does_too() {
    let mut ed = Ed::new(T);
    ed.keys("<C-v>");
    assert_eq!(ed.mode(), Mode::VisualBlock);
    ed.keys("<Esc><C-q>");
    assert_eq!(ed.mode(), Mode::VisualBlock);
    ed.keys("<C-v>");
    assert_eq!(ed.mode(), Mode::Normal, "again leaves it");
}

#[test]
fn the_selection_modes_switch_into_one_another() {
    let mut ed = Ed::new(T);
    ed.keys("v<C-v>");
    assert_eq!(ed.mode(), Mode::VisualBlock);
    ed.keys("V");
    assert_eq!(ed.mode(), Mode::VisualLine);
    ed.keys("<C-v>v");
    assert_eq!(ed.mode(), Mode::Visual);
}

#[test]
fn a_block_has_a_row_on_each_line_of_its_columns() {
    let mut ed = Ed::new(T);
    ed.keys("<C-v>jl");
    assert_eq!(ed.vim.selections(), vec![(0, 2), (4, 6)]);
    ed.keys("j");
    assert_eq!(ed.vim.selections(), vec![(0, 2), (4, 6), (8, 10)]);
    assert_eq!(ed.vim.selection(), Some((0, 10)));
}

#[test]
fn a_row_too_short_to_reach_the_columns_is_empty_and_dollar_runs_to_each_line_end() {
    let mut ed = Ed::new("|abc\nd\nghi");
    ed.keys("<C-v>jjll");
    assert_eq!(ed.vim.selections(), vec![(0, 3), (4, 5), (6, 9)]);
    let mut ed = Ed::new("ab|cd\nx\nabcd");
    ed.keys("<C-v>jj");
    assert_eq!(ed.vim.selections(), vec![(2, 3), (6, 6), (9, 10)]);
    ed.keys("$");
    assert_eq!(ed.vim.selections(), vec![(2, 4), (6, 6), (9, 11)]);
}

#[test]
fn deleting_a_block_takes_its_rows() {
    table(&[
        ("|abc\ndef\nghi", "<C-v>jld", "|c\nf\nghi"),
        ("|abc\ndef\nghi", "<C-v>jx", "|bc\nef\nghi"),
        ("|abc\nd\nghi", "<C-v>jjld", "|c\n\ni"),
        ("a|bc\ndef\nghi", "<C-v>jjd", "a|c\ndf\ngi"),
        ("|ab\nabcd", "<C-v>j$d", "|\n"),
        ("|abc\ndef", "ll<C-v>jhd", "|a\nd"),
    ]);
}

#[test]
fn d_and_c_in_a_block_with_capitals_go_to_the_ends_of_the_rows() {
    table(&[("a|bc\ndef", "<C-v>jD", "|a\nd"), ("a|bc\ndef", "<C-v>jCX<Esc>", "a|X\ndX")]);
}

#[test]
fn a_block_is_a_register_of_rows() {
    let mut ed = Ed::new(T);
    ed.keys("<C-v>jl\"ay");
    let reg = ed.sh.regs.get(Some('a')).unwrap();
    assert_eq!((reg.text.as_str(), reg.blockwise, reg.linewise), ("ab\nde", true, false));
    assert_eq!(ed.shows(), T);
    let mut ed = Ed::new("|abc\nd\nghi");
    ed.keys("<C-v>jjly");
    assert_eq!(ed.sh.regs.get(None).unwrap().text, "ab\nd\ngh");
}

#[test]
fn a_block_is_put_back_as_rows_one_under_another() {
    table(&[
        ("|abc\ndef\nghi", "<C-v>jly$p", "abc|ab\ndefde\nghi"),
        ("|abc\ndef\nghi", "<C-v>jlyP", "|ababc\ndedef\nghi"),
        ("|abc\ndef\nghi", "<C-v>jlyGp", "abc\ndef\ng|abhi\n de"),
        ("a|bc\ndef", "<C-v>jy0P", "|babc\nedef"),
    ]);
}

#[test]
fn a_block_put_beyond_the_text_adds_lines_and_pads_short_ones() {
    table(&[("|ab\n\ncd", "<C-v>jjy$p", "ab|a\n   \ncdc"), ("|abc\ndef", "<C-v>jlyGp", "abc\nd|abef\n de")]);
}

#[test]
fn inserting_before_a_block_puts_the_text_in_every_row() {
    table(&[
        ("|abc\ndef\nghi", "<C-v>jIX<Esc>", "|Xabc\nXdef\nghi"),
        ("|abc\ndef\nghi", "<C-v>jjIXY<Esc>", "|XYabc\nXYdef\nXYghi"),
        ("a|bc\ndef\nghi", "<C-v>jIX<Esc>", "a|Xbc\ndXef\nghi"),
        ("a|bc\nd\nghi", "<C-v>jjIX<Esc>", "a|Xbc\ndX\ngXhi"),
    ]);
}

#[test]
fn appending_after_a_block_and_after_each_line_end() {
    table(&[
        ("|abc\ndef\nghi", "<C-v>jAX<Esc>", "a|Xbc\ndXef\nghi"),
        ("|abc\ndef\nghi", "<C-v>jlAX<Esc>", "ab|Xc\ndeXf\nghi"),
        ("|abc\ndef\nghi", "<C-v>j$AX<Esc>", "abc|X\ndefX\nghi"),
        ("|abc\ndef\nghi", "<C-v>jj$AXY<Esc>", "abc|XY\ndefXY\nghiXY"),
    ]);
}

#[test]
fn changing_a_block_types_in_its_place() {
    table(&[
        ("|abc\ndef\nghi", "<C-v>jlcZ<Esc>", "|Zc\nZf\nghi"),
        ("|abc\ndef\nghi", "<C-v>jcZY<Esc>", "|ZYbc\nZYef\nghi"),
    ]);
}

#[test]
fn a_blocks_inserted_text_is_not_repeated_if_it_has_a_line_break() {
    table(&[("|abc\ndef", "<C-v>jIX<CR>Y<Esc>", "|X\nYabc\ndef")]);
}

#[test]
fn replacing_in_a_block() {
    table(&[
        ("|abc\ndef\nghi", "<C-v>jlrx", "|xxc\nxxf\nghi"),
        ("|abc\nd\nghi", "<C-v>jjlrx", "|xxc\nx\nxxi"),
        ("|a\tb\ncde", "<C-v>jlrx", "|xxxxxb\nxxe"),
    ]);
}

#[test]
fn case_in_a_block() {
    table(&[
        ("|abc\ndef\nghi", "<C-v>jl~", "|ABc\nDEf\nghi"),
        ("|abc\ndef\nghi", "<C-v>jlU", "|ABc\nDEf\nghi"),
        ("|ABC\nDEF\nghi", "<C-v>jlu", "|abC\ndeF\nghi"),
        ("|abc\ndef\nghi", "<C-v>jlg?", "|noc\nqrf\nghi"),
    ]);
}

#[test]
fn indenting_a_block_moves_its_left_edge() {
    table(&[
        ("a|bc\ndef", "<C-v>j>", "a|    bc\nd    ef"),
        ("a|bc\ndef", "<C-v>j2>", "a|        bc\nd        ef"),
        ("a  |bc\nd  ef", "<C-v>jh<", "a |bc\nd ef"),
    ]);
}

#[test]
fn o_swaps_the_corner_and_a_capital_o_the_side() {
    let mut ed = Ed::new("a|bc\ndef\nghi");
    ed.keys("<C-v>jl");
    assert_eq!(ed.vim.selections(), vec![(1, 3), (5, 7)]);
    ed.keys("o");
    assert_eq!(ed.shows(), "a|bc\ndef\nghi");
    assert_eq!(ed.vim.selections(), vec![(1, 3), (5, 7)], "the same block");
    ed.keys("O");
    assert_eq!(ed.shows(), "ab|c\ndef\nghi");
    assert_eq!(ed.vim.selections(), vec![(1, 3), (5, 7)], "the same block");
}

#[test]
fn joining_the_rows_of_a_block_joins_its_lines() {
    table(&[("|abc\ndef\nghi", "<C-v>jJ", "abc| def\nghi")]);
}

#[test]
fn text_pasted_over_a_block_replaces_it() {
    table(&[
        ("|abc\ndef", "yl<C-v>jlp", "|ac\nf"),
        ("|abc\ndef", "<C-v>jy0l<C-v>jlp", "a|a\ndd"),
    ]);
}

#[test]
fn a_selection_taken_into_a_block_with_gv_comes_back_a_block() {
    let mut ed = Ed::new(T);
    ed.keys("<C-v>jl<Esc>gg0gv");
    assert_eq!(ed.mode(), Mode::VisualBlock);
    assert_eq!(ed.vim.selections(), vec![(0, 2), (4, 6)]);
}

#[test]
fn a_block_that_ran_to_line_ends_is_one_again_after_gv() {
    let mut ed = Ed::new("|ab\nabcd");
    ed.keys("<C-v>j$<Esc>gg0gv");
    assert_eq!(ed.vim.selections(), vec![(0, 2), (3, 7)]);
}
