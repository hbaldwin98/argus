//! What `.` repeats (EDITOR-VIM-9) beyond the first few: an operator on a
//! selection, made again over a selection of the same size from the cursor,
//! and the library's rule that a yank is an operator too.

use super::*;

#[test]
fn an_operator_on_a_selection_is_repeated_over_the_same_amount() {
    table(&[
        ("|abcdefgh", "vld.", "|efgh"),
        ("|abcdefgh", "vlld.", "|gh"),
        ("|a\nb\nc\nd\ne", "Vjd.", "|e"),
        ("|abcd\nefgh\nijkl\nmnop", "<C-v>jld.", "|\n\nijkl\nmnop"),
        ("|abcdef", "vl~l.", "A|bCdef"),
    ]);
}

#[test]
fn a_selection_over_lines_is_made_again_with_the_same_lines_and_the_cursors_column() {
    // As the library does: the last line is taken as far as the column of the cursor.
    table(&[("|ab\ncd\nef\ngh", "vjd.", "|f\ngh")]);
}

#[test]
fn a_selection_command_that_is_an_action_is_repeated_as_a_normal_command() {
    table(&[("|a\nb\nc\nd\ne", "VjJ.", "a b| c\nd\ne"), ("|abcd", "vlrx$.", "xxc|x")]);
}

#[test]
fn a_yank_is_what_dot_repeats_when_it_was_the_last_operator() {
    // The library keeps every operator for `.`, a yank too.
    table(&[("|abc", "xyy.", "|bc"), ("|a b c", "dwyw.", "|b c"), ("|abcd", "xlyl.", "b|cd")]);
}

#[test]
fn redoing_in_a_selection_moves_its_other_end_with_the_text() {
    // The redo put a line before the emoji, and the anchor, which was a byte offset into the
    // text before it, stood inside the emoji until it was carried along.
    let mut ed = Ed::new("|\n😀'é");
    ed.keys("o<Esc>uevh<C-r>");
    assert_eq!(ed.mode(), Mode::Visual);
    let (from, to) = ed.vim.selection().unwrap();
    assert!(ed.vim.doc().is_char_boundary(from) && ed.vim.doc().is_char_boundary(to));
}
