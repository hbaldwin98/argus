use proptest::prelude::*;
use ropey::Rope;

use super::*;

fn t(r: &Rope) -> Text<'_> {
    Text(r)
}

#[test]
fn a_row_down_keeps_the_column_within_the_row() {
    let r = Rope::from_str("abcdefghij\nxy");
    // Rows of four: abcd efgh ij.
    assert_eq!(vertical(t(&r), 0, true, Some(4), 0), Some(4));
    assert_eq!(vertical(t(&r), 1, true, Some(4), 1), Some(5));
    assert_eq!(vertical(t(&r), 5, true, Some(4), 1), Some(9));
    assert_eq!(vertical(t(&r), 7, true, Some(4), 3), Some(9), "a short last row gives its last character");
}

#[test]
fn the_last_row_goes_on_to_the_next_line_and_the_last_line_stops() {
    let r = Rope::from_str("abcdef\nxy");
    assert_eq!(vertical(t(&r), 5, true, Some(4), 1), Some(8), "the next line's own column");
    assert_eq!(vertical(t(&r), 8, true, Some(4), 1), None);
}

#[test]
fn a_row_up_goes_to_the_last_row_of_the_line_above() {
    let r = Rope::from_str("abcdefghij\nxy");
    assert_eq!(vertical(t(&r), 11, false, Some(4), 0), Some(8));
    assert_eq!(vertical(t(&r), 9, false, Some(4), 1), Some(5));
    assert_eq!(vertical(t(&r), 1, false, Some(4), 1), None, "the top of the text");
}

#[test]
fn with_no_width_a_line_is_one_row() {
    let r = Rope::from_str("abc\ndef");
    assert_eq!(vertical(t(&r), 1, true, None, 1), Some(5));
    assert_eq!(vertical(t(&r), 5, false, None, 1), Some(1));
    assert_eq!(vertical(t(&r), 5, true, None, 1), None);
}

#[test]
fn the_start_of_a_row_is_the_smart_home_on_the_first_and_the_rows_own_start_after() {
    let r = Rope::from_str("  abcdefgh");
    assert_eq!(row_start(t(&r), 6, None), 2, "to the indent first");
    assert_eq!(row_start(t(&r), 2, None), 0, "then to the start");
    assert_eq!(row_start(t(&r), 0, None), 2, "and round");
    assert_eq!(row_start(t(&r), 7, Some(4)), 4, "a later row begins at its own column");
    assert_eq!(row_start(t(&r), 3, Some(4)), 2);
}

#[test]
fn a_line_with_no_indent_has_its_start() {
    let r = Rope::from_str("abc");
    assert_eq!(row_start(t(&r), 2, None), 0);
    let r = Rope::from_str("");
    assert_eq!(row_start(t(&r), 0, None), 0);
}

#[test]
fn the_end_of_a_row_is_its_last_character() {
    let r = Rope::from_str("abcdefghij");
    assert_eq!(row_end(t(&r), 1, Some(4)), 3);
    assert_eq!(row_end(t(&r), 5, Some(4)), 7);
    assert_eq!(row_end(t(&r), 9, Some(4)), 9, "a short last row ends on the line's last character");
    assert_eq!(row_end(t(&r), 1, None), 9);
}

proptest! {
    // Wherever a row move lands, it is a character of the text and on its own line or the next.
    #[test]
    fn a_row_move_lands_on_a_character(
        text in "[ab \n]{0,40}", from in 0usize..40, width in proptest::option::of(1usize..8), want in 0usize..10
    ) {
        let r = Rope::from_str(&text);
        let from = r.floor_char_boundary(from.min(text.len()));
        let t = Text(&r);
        for forward in [true, false] {
            if let Some(to) = vertical(t, from, forward, width, want) {
                prop_assert!(to <= text.len() && r.is_char_boundary(to));
            }
        }
        for to in [row_start(t, from, width), row_end(t, from, width)] {
            prop_assert!(to <= text.len() && r.is_char_boundary(to));
            prop_assert_eq!(t.line_of(to), t.line_of(from));
        }
    }
}
