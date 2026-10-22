use proptest::prelude::*;
use ropey::Rope;

use super::*;

fn at(text: &str, from: At, count: usize, forward: bool) -> At {
    let r = Rope::from_str(text);
    find(Text(&r), from, count, forward)
}

fn the(text: &str, from: At, count: usize, inclusive: bool) -> (At, At) {
    let r = Rope::from_str(text);
    object(Text(&r), from, count, inclusive)
}

const T: &str = "One two. Three four.  Five.";

#[test]
fn a_sentence_ends_at_a_stop_followed_by_a_blank_or_the_end_of_the_line() {
    assert_eq!(at(T, (0, 0), 1, true), (0, 9));
    assert_eq!(at(T, (0, 9), 1, true), (0, 22));
    assert_eq!(at(T, (0, 0), 2, true), (0, 22));
    assert_eq!(at(T, (0, 25), 1, true), (0, 26), "the last sentence ends on its last character");
}

#[test]
fn a_stop_with_no_blank_after_it_is_inside_a_sentence() {
    assert_eq!(at("a.b c. D", (0, 0), 1, true), (0, 7));
    assert_eq!(at("e.g. one. Two", (0, 0), 1, true).1, 5, "a blank after the stop ends it, whatever the stop is in");
}

#[test]
fn a_question_mark_and_an_exclamation_mark_end_one_too() {
    assert_eq!(at("What? Yes! No.", (0, 0), 2, true), (0, 11));
}

#[test]
fn backward_goes_to_the_start_of_this_sentence_and_then_the_one_before() {
    assert_eq!(at(T, (0, 25), 1, false), (0, 22));
    assert_eq!(at(T, (0, 22), 1, false), (0, 9));
    assert_eq!(at(T, (0, 12), 1, false), (0, 9));
    assert_eq!(at(T, (0, 9), 1, false), (0, 0));
    assert_eq!(at(T, (0, 0), 1, false), (0, 0));
}

#[test]
fn an_empty_line_is_where_a_walk_stops_and_the_next_text_is_the_walk_after() {
    let text = "One.\n\nTwo.";
    assert_eq!(at(text, (0, 0), 1, true), (1, 0));
    assert_eq!(at(text, (1, 0), 1, true), (2, 0));
    assert_eq!(at(text, (2, 0), 1, false), (1, 0));
    assert_eq!(at(text, (1, 0), 1, false), (0, 0));
}

#[test]
fn a_walk_crosses_lines_inside_a_sentence() {
    let text = "One two\nthree. Four";
    assert_eq!(at(text, (0, 0), 1, true), (1, 7));
}

#[test]
fn an_object_without_blanks_is_the_sentence_to_its_stop() {
    assert_eq!(the(T, (0, 12), 1, false), ((0, 9), (0, 20)));
    assert_eq!(the(T, (0, 2), 1, false), ((0, 0), (0, 8)));
}

#[test]
fn an_object_with_blanks_takes_those_after_it() {
    assert_eq!(the(T, (0, 12), 1, true), ((0, 9), (0, 22)));
    assert_eq!(the(T, (0, 0), 1, true), ((0, 0), (0, 9)));
}

#[test]
fn a_count_takes_that_many_sentences() {
    assert_eq!(the(T, (0, 0), 2, false), ((0, 0), (0, 20)));
    assert_eq!(the(T, (0, 0), 2, true), ((0, 0), (0, 22)));
}

#[test]
fn the_last_sentence_takes_the_blank_before_it_for_a() {
    let (from, to) = the("One. Two.", (0, 6), 1, true);
    assert_eq!((from.1, to.1), (4, 9));
}

proptest! {
    // However the text is, a walk stays in it: a line of the text, a column that is no further than that line's end.
    #[test]
    fn a_walk_stays_in_the_text(text in "[ab.!? \n]{0,40}", line in 0usize..4, col in 0usize..12, count in 1usize..4, forward in any::<bool>()) {
        let r = Rope::from_str(&text);
        let t = Text(&r);
        let line = line.min(t.last_line());
        // A cursor rests on a character, or on an empty line.
        let col = col.min(t.line_len(line).saturating_sub(1));
        let (l, c) = find(t, (line, col), count, forward);
        prop_assert!(l <= t.last_line());
        prop_assert!(c <= t.line_len(l));
        // Forward never goes back, nor backward forward, but at the ends of the
        // text, where the walk stops on the nearest character that is not blank.
        let (here, there) = (t.at(line, col), t.at(l, c));
        let blank = |s: String| s.chars().all(char::is_whitespace);
        if forward {
            prop_assert!(there >= here || blank(t.slice(here, t.len())));
        } else {
            prop_assert!(there <= here || blank(t.slice(0, here)));
        }
    }

    // An object is a span of the text that holds the position, or begins at the word after.
    #[test]
    fn an_object_is_a_span_of_the_text(text in "[ab.!? ]{1,30}", col in 0usize..30, count in 1usize..3, inclusive in any::<bool>()) {
        let r = Rope::from_str(&text);
        let t = Text(&r);
        let col = col.min(t.line_len(0).saturating_sub(1));
        let (from, to) = object(t, (0, col), count, inclusive);
        prop_assert!(from.0 == 0 && to.0 == 0);
        prop_assert!(from.1 <= to.1 && to.1 <= t.line_len(0) + 1);
    }
}
