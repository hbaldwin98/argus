use proptest::prelude::*;

use super::*;
use crate::code::vim::tests::Ed;

fn space(line: &str, max: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    find_space(&chars, max, 5)
}

#[test]
fn a_line_shorter_than_the_width_has_no_break() {
    assert_eq!(space("aaa bbb", 10), None);
    assert_eq!(space("aaa bbb", 8), None);
}

#[test]
fn the_break_is_the_blanks_ending_at_the_width_or_those_right_after_it() {
    assert_eq!(space("aaa bbb ccc", 8), Some((7, 8)));
    assert_eq!(space("aaa bbb ccc", 7), Some((7, 8)));
    assert_eq!(space("aaa bbb  ccc", 8), Some((7, 9)), "the blanks on both sides");
}

#[test]
fn a_word_over_the_width_is_broken_before_it_or_after_it() {
    assert_eq!(space("aaaaaaa bbbbbbbbb c", 12), Some((7, 8)), "before it, if that is more than five columns in");
    assert_eq!(space("aaa bbbbbbb ccc", 8), Some((11, 12)), "after it, when before is not");
    assert_eq!(space("a bbbbbbbbbb c", 8), Some((12, 13)));
    assert_eq!(space("aaaaaaaaaa bbb", 5), Some((10, 11)));
}

#[test]
fn a_break_at_or_before_the_fifth_column_is_not_taken() {
    // The library's rule: a line whose only place to break is that near its start is left long.
    assert_eq!(space("aaaaa aaaaa", 8), None);
    let mut ed = Ed::new("|aaaaa aaaaa");
    ed.keys(":set tw=8<CR>gqq");
    assert_eq!(ed.text(), "aaaaa aaaaa");
    let mut ed = Ed::new("|aaaaaa aaaaaa");
    ed.keys(":set tw=9<CR>gqq");
    assert_eq!(ed.text(), "aaaaaa\naaaaaa");
}

#[test]
fn a_line_with_no_blank_to_break_at_is_left() {
    assert_eq!(space("aaaaaaaaaaaaaaaa", 5), None);
}

#[test]
fn wrapping_joins_a_short_line_to_the_next_and_leaves_a_blank_line_between_paragraphs() {
    let mut ed = Ed::new("|a\nb\n\nc\nd");
    ed.keys("gqG");
    assert_eq!(ed.text(), "a b\n\nc d");
}

#[test]
fn trailing_blanks_are_taken_off_a_line_that_will_not_join() {
    let mut ed = Ed::new("|aaaaa   \nbbbbbbbbb");
    ed.keys(":set tw=10<CR>gqj");
    assert_eq!(ed.text(), "aaaaa\nbbbbbbbbb");
}

#[test]
fn a_read_only_file_is_not_wrapped() {
    let mut ed = Ed::new("|aaa bbb ccc ddd");
    ed.vim.set_read_only(true);
    ed.keys(":set tw=5<CR>gqq");
    assert_eq!(ed.text(), "aaa bbb ccc ddd");
}

#[test]
fn a_line_whose_indent_is_wider_than_the_width_is_left_not_broken_for_ever() {
    // The library breaks "        hell" at the blanks again and again, each time into the
    // same line under the same indent, and so never ends.
    let mut ed = Ed::new("|        hell
    a    a");
    ed.keys(":set tw=6<CR>gqG");
    assert_eq!(ed.text(), "        hell
    a
    a");
}

proptest! {
    // Wrapping moves only the blanks: the other characters are the same in the same order, and
    // a text of words long enough to be broken after is in lines no longer than the width.
    #[test]
    fn wrapping_keeps_the_words_and_fits_them_in_the_width(words in proptest::collection::vec("[a-z]{6,8}", 1..14), width in 16usize..30) {
        let text = words.join(" ");
        let mut ed = Ed::new(&text);
        ed.keys(&format!(":set tw={width}<CR>gqG"));
        let wrapped = ed.text();
        let squeeze = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        prop_assert_eq!(squeeze(&wrapped), text.clone());
        for line in wrapped.split('\n') {
            prop_assert!(line.chars().count() <= width, "{line:?} is over {width}");
        }
    }

    // Wrapping twice is wrapping once.
    #[test]
    fn wrapping_is_idempotent(words in proptest::collection::vec("[a-z]{1,8}", 1..14), width in 8usize..30) {
        let mut ed = Ed::new(&words.join(" "));
        ed.keys(&format!(":set tw={width}<CR>gqG"));
        let once = ed.text();
        ed.keys("gqG");
        prop_assert_eq!(ed.text(), once);
    }
}
