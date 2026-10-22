//! What depends on the window (EDITOR-VIM-7): `H M L`, paging, half pages,
//! `Ctrl-e`, `Ctrl-y` and the `z` family, which use the view the window
//! tells Vim, and `gj`, `gk`, `g0`, `g^` and `g$` on wrapped lines.

use super::*;
use crate::code::vim::screen::Place;

/// Twenty lines, `l00` to `l19`, with the cursor on the first.
fn twenty() -> Ed {
    let text: Vec<String> = (0..20).map(|i| format!("  l{i:02}")).collect();
    Ed::new(&format!("|{}", text.join("\n")))
}

fn line(ed: &Ed) -> usize {
    ed.vim.doc().byte_to_line_idx(ed.vim.cursor(), ropey::LineType::LF)
}

#[test]
fn h_m_and_l_go_to_the_top_middle_and_bottom_line_in_view() {
    let mut ed = twenty();
    ed.view(5, 14);
    ed.keys("H");
    assert_eq!(line(&ed), 5);
    assert_eq!(ed.vim.cursor(), ed.vim.doc().line_to_byte_idx(5, ropey::LineType::LF) + 2, "at the first non-blank");
    ed.keys("M");
    assert_eq!(line(&ed), 9);
    ed.keys("L");
    assert_eq!(line(&ed), 14);
    ed.keys("3H");
    assert_eq!(line(&ed), 7);
    ed.keys("2L");
    assert_eq!(line(&ed), 13);
}

#[test]
fn a_count_on_h_or_l_stops_at_the_view() {
    let mut ed = twenty();
    ed.view(5, 9);
    ed.keys("99H");
    assert_eq!(line(&ed), 9);
    ed.keys("99L");
    assert_eq!(line(&ed), 5);
}

#[test]
fn with_no_window_told_the_whole_text_is_in_view() {
    let mut ed = twenty();
    ed.keys("L");
    assert_eq!(line(&ed), 19);
    ed.keys("H");
    assert_eq!(line(&ed), 0);
}

#[test]
fn h_and_l_are_linewise_for_an_operator_and_jumps() {
    let mut ed = twenty();
    ed.view(0, 9);
    ed.keys("jjdL");
    assert_eq!(ed.text().lines().count(), 12, "lines 2 to 9 are gone");
    ed.keys("<C-o>");
    assert_eq!(line(&ed), 2, "and the jump list has where it began");
}

#[test]
fn a_page_is_the_lines_in_view() {
    let mut ed = twenty();
    ed.view(0, 9);
    ed.keys("<C-f>");
    assert_eq!(line(&ed), 10);
    ed.keys("<C-f>");
    assert_eq!(line(&ed), 19, "to the end and no further");
    ed.keys("<C-b>");
    assert_eq!(line(&ed), 9);
    ed.keys("2<C-b>");
    assert_eq!(line(&ed), 0);
    ed.keys("<PageDown>");
    assert_eq!(line(&ed), 10);
    ed.keys("<PageUp>");
    assert_eq!(line(&ed), 0);
}

#[test]
fn paging_keeps_the_column() {
    let mut ed = Ed::new("|abcdef\nabcdef\nab\nabcdef");
    ed.view(0, 1);
    ed.keys("3l<C-f>");
    assert_eq!(ed.shows(), "abcdef\nabcdef\na|b\nabcdef");
}

#[test]
fn a_half_page_moves_half_the_view_and_scrolls_with_the_cursor() {
    let mut ed = twenty();
    ed.view(0, 9);
    let step = ed.keys("<C-d>");
    assert_eq!(line(&ed), 5);
    assert_eq!(step.effects, vec![Effect::Reveal { line: 5, at: Place::Top }]);
    ed.view(5, 14);
    let step = ed.keys("<C-u>");
    assert_eq!(line(&ed), 0);
    assert_eq!(step.effects, vec![Effect::Reveal { line: 0, at: Place::Top }]);
}

#[test]
fn a_count_on_a_half_page_is_how_many_lines() {
    let mut ed = twenty();
    ed.view(0, 9);
    ed.keys("3<C-d>");
    assert_eq!(line(&ed), 3);
    ed.keys("<C-d>");
    assert_eq!(line(&ed), 8, "the count is not remembered: half the view again");
}

#[test]
fn ctrl_e_and_ctrl_y_scroll_by_lines_and_take_the_cursor_along() {
    let mut ed = twenty();
    ed.view(0, 9);
    let step = ed.keys("<C-e>");
    assert_eq!(step.effects, vec![Effect::Reveal { line: 1, at: Place::Top }]);
    assert_eq!(line(&ed), 1, "the cursor was above the new top");
    ed.view(1, 10);
    ed.keys("5j");
    let step = ed.keys("3<C-e>");
    assert_eq!(step.effects, vec![Effect::Reveal { line: 4, at: Place::Top }]);
    assert_eq!(line(&ed), 6, "still in view: where it was");
    ed.view(5, 14);
    ed.keys("L");
    let step = ed.keys("2<C-y>");
    assert_eq!(step.effects, vec![Effect::Reveal { line: 12, at: Place::Bottom }]);
    assert_eq!(line(&ed), 12, "the cursor was below the new bottom");
}

#[test]
fn the_z_commands_put_the_cursor_line_in_the_view() {
    let mut ed = twenty();
    ed.view(0, 9);
    ed.keys("5j");
    for (keys, at) in [("zt", Place::Top), ("zz", Place::Center), ("zb", Place::Bottom)] {
        let step = ed.keys(keys);
        assert_eq!(step.effects, vec![Effect::Reveal { line: 5, at }], "{keys}");
        assert_eq!(line(&ed), 5);
    }
}

#[test]
fn z_dot_z_enter_and_z_minus_also_go_to_the_first_non_blank() {
    let mut ed = twenty();
    ed.view(0, 9);
    ed.keys("5j$");
    let step = ed.keys("z.");
    assert_eq!(step.effects, vec![Effect::Reveal { line: 5, at: Place::Center }]);
    assert_eq!(ed.vim.cursor(), ed.vim.doc().line_to_byte_idx(5, ropey::LineType::LF) + 2);
    ed.keys("$");
    let step = ed.keys("z<CR>");
    assert_eq!(step.effects, vec![Effect::Reveal { line: 5, at: Place::Top }]);
    ed.keys("$");
    let step = ed.keys("z-");
    assert_eq!(step.effects, vec![Effect::Reveal { line: 5, at: Place::Bottom }]);
    assert_eq!(ed.vim.cursor(), ed.vim.doc().line_to_byte_idx(5, ropey::LineType::LF) + 2);
}

#[test]
fn a_z_with_nothing_after_it_that_means_something_is_dropped() {
    let mut ed = twenty();
    assert!(ed.keys("zq").effects.is_empty());
    assert!(ed.keys("z").handled);
    assert_eq!(ed.vim.pending().len(), 1);
}

#[test]
fn gj_and_gk_move_by_rows_of_a_wrapped_line() {
    let mut ed = Ed::new("|abcdefghij\nxy");
    ed.vim.set_wrap(Some(4));
    ed.keys("gj");
    assert_eq!(ed.shows(), "abcd|efghij\nxy");
    ed.keys("gj");
    assert_eq!(ed.shows(), "abcdefgh|ij\nxy");
    ed.keys("gj");
    assert_eq!(ed.shows(), "abcdefghij\n|xy");
    ed.keys("gk");
    assert_eq!(ed.shows(), "abcdefgh|ij\nxy");
    ed.keys("gk");
    assert_eq!(ed.shows(), "abcd|efghij\nxy");
}

#[test]
fn gj_keeps_the_column_within_the_row() {
    let mut ed = Ed::new("ab|cdefghij");
    ed.vim.set_wrap(Some(4));
    ed.keys("gj");
    assert_eq!(ed.shows(), "abcdef|ghij");
    ed.keys("gj");
    assert_eq!(ed.shows(), "abcdefghi|j", "a short last row gives its last character");
}

#[test]
fn with_no_wrap_gj_is_j() {
    table(&[("a|b\ncd", "gj", "ab\nc|d"), ("ab\nc|d", "gk", "a|b\ncd"), ("a|b\ncd", "g<Down>", "ab\nc|d")]);
}

#[test]
fn g0_g_caret_and_g_dollar_go_to_the_ends_of_the_row() {
    let mut ed = Ed::new("abcdefg|hij");
    ed.vim.set_wrap(Some(4));
    ed.keys("g0");
    assert_eq!(ed.shows(), "abcd|efghij");
    ed.keys("g$");
    assert_eq!(ed.shows(), "abcdefg|hij");
    ed.keys("g^");
    assert_eq!(ed.shows(), "abcd|efghij");
}

#[test]
fn g0_on_the_first_row_is_the_smart_home() {
    table(&[
        ("  ab|c", "g0", "  |abc"),
        ("  |abc", "g0", "|  abc"),
        ("  ab|c", "g^", "  |abc"),
        ("a|bc", "g0", "|abc"),
    ]);
}

#[test]
fn g_dollar_goes_to_the_last_character_when_nothing_wraps() {
    table(&[("|abc\nd", "g$", "ab|c\nd")]);
}
