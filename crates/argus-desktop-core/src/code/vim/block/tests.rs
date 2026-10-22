use proptest::prelude::*;
use ropey::Rope;

use super::*;
use crate::code::vim::tests::Ed;

fn rows_of(text: &str, anchor: usize, cursor: usize, eol: bool) -> Vec<(usize, usize)> {
    let r = Rope::from_str(text);
    rows(Text(&r), anchor, cursor, eol)
}

#[test]
fn a_block_has_a_row_for_each_line_between_its_corners_in_either_order() {
    assert_eq!(rows_of("abc\ndef\nghi", 0, 5, false), vec![(0, 2), (4, 6)]);
    assert_eq!(rows_of("abc\ndef\nghi", 5, 0, false), vec![(0, 2), (4, 6)]);
    assert_eq!(rows_of("abc\ndef\nghi", 1, 8, false), vec![(0, 2), (4, 6), (8, 10)], "the columns of both corners");
}

#[test]
fn a_corner_in_a_wide_character_counts_characters_not_bytes() {
    assert_eq!(rows_of("éab\nxyz", 0, 6, false), vec![(0, 3), (5, 7)]);
}

#[test]
fn a_row_the_columns_do_not_reach_is_empty_at_its_lines_end() {
    assert_eq!(rows_of("abcd\nx\nabcd", 2, 9, false), vec![(2, 3), (6, 6), (9, 10)]);
}

#[test]
fn after_dollar_each_row_runs_to_its_lines_end() {
    assert_eq!(rows_of("abcd\nx\nabcd", 2, 9, true), vec![(2, 4), (6, 6), (9, 11)]);
}

#[test]
fn the_insert_goes_into_each_row_at_the_column_or_the_line_end() {
    let mut ed = Ed::new("|abc\nde\nfghij");
    ed.keys("l<C-v>jjIX<Esc>");
    assert_eq!(ed.text(), "aXbc\ndXe\nfXghij");
    let mut ed = Ed::new("|abc\nde\nfghij");
    ed.keys("<C-v>jj$AX<Esc>");
    assert_eq!(ed.text(), "abcX\ndeX\nfghijX");
}

#[test]
fn a_block_insert_of_nothing_changes_nothing() {
    let mut ed = Ed::new("|abc\ndef");
    ed.keys("<C-v>jI<Esc>");
    assert_eq!(ed.text(), "abc\ndef");
}

#[test]
fn a_read_only_file_is_not_changed_by_a_block() {
    let mut ed = Ed::new("|abc\ndef");
    ed.vim.set_read_only(true);
    ed.keys("<C-v>jld");
    assert_eq!(ed.text(), "abc\ndef");
    ed.keys("<C-v>jIX<Esc>");
    assert_eq!(ed.text(), "abc\ndef");
}

proptest! {
    // However the text and the corners, the rows are one a line, in order, apart, each inside its own line.
    #[test]
    fn the_rows_are_one_a_line_each_inside_its_line(
        text in "[ab\né]{1,30}", a in 0usize..30, c in 0usize..30, eol in any::<bool>()
    ) {
        let r = Rope::from_str(&text);
        let t = Text(&r);
        let (a, c) = (r.floor_char_boundary(a.min(text.len())), r.floor_char_boundary(c.min(text.len())));
        let found = rows(t, a, c, eol);
        let (top, bottom) = (t.line_of(a).min(t.line_of(c)), t.line_of(a).max(t.line_of(c)));
        prop_assert_eq!(found.len(), bottom - top + 1);
        for (i, &(from, to)) in found.iter().enumerate() {
            let line = top + i;
            prop_assert!(from <= to);
            prop_assert!(t.line_start(line) <= from && to <= t.line_end(line));
            prop_assert!(r.is_char_boundary(from) && r.is_char_boundary(to));
        }
    }

    // Deleting a block takes exactly the text of its rows: what is left plus what the register holds is every character.
    #[test]
    fn deleting_a_block_takes_the_text_of_its_rows(text in "[ab]{1,6}(\n[ab]{0,6}){0,3}", rows_down in 0usize..4, cols in 0usize..4) {
        let mut ed = Ed::new(&text);
        let keys = format!("<C-v>{}{}d", "j".repeat(rows_down), "l".repeat(cols));
        ed.keys(&keys);
        let held = ed.sh.regs.get(None).map(|r| r.text.replace('\n', "")).unwrap_or_default();
        let left: usize = ed.text().chars().filter(|c| *c != '\n').count();
        let all: usize = text.chars().filter(|c| *c != '\n').count();
        prop_assert_eq!(left + held.chars().count(), all);
    }
}
