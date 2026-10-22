use ropey::Rope;

use super::*;

fn pos(text: &str, f: fn(Text, usize, bool) -> usize, from: usize, big: bool) -> usize {
    let r = Rope::from_str(text);
    f(Text(&r), from, big)
}

#[test]
fn w_goes_to_the_next_word_start_over_punctuation_and_lines() {
    let s = "foo.bar baz\nqux";
    assert_eq!(pos(s, forward, 0, false), 3);
    assert_eq!(pos(s, forward, 3, false), 4);
    assert_eq!(pos(s, forward, 4, false), 8);
    assert_eq!(pos(s, forward, 8, false), 12);
    assert_eq!(pos(s, forward, 12, false), 15, "the end of the text");
}

#[test]
fn big_w_takes_punctuation_into_the_word() {
    assert_eq!(pos("foo.bar baz", forward, 0, true), 8);
}

#[test]
fn w_stops_on_an_empty_line() {
    let s = "a\n\nb";
    assert_eq!(pos(s, forward, 0, false), 2);
    assert_eq!(pos(s, forward, 2, false), 3);
}

#[test]
fn e_goes_to_the_end_of_this_word_or_the_next_from_an_end() {
    let s = "foo bar";
    assert_eq!(pos(s, end, 0, false), 2);
    assert_eq!(pos(s, end, 2, false), 6);
    assert_eq!(pos(s, end, 6, false), 6, "stays on the last character");
}

#[test]
fn b_goes_to_the_start_of_this_word_or_the_one_before() {
    let s = "foo bar";
    assert_eq!(pos(s, back, 6, false), 4);
    assert_eq!(pos(s, back, 4, false), 0);
    assert_eq!(pos(s, back, 0, false), 0);
    assert_eq!(pos("a\n\nb", back, 3, false), 2, "an empty line is a stop");
}

#[test]
fn ge_goes_to_the_end_of_the_word_before() {
    let s = "foo bar";
    assert_eq!(pos(s, end_back, 5, false), 2);
    assert_eq!(pos(s, end_back, 4, false), 2);
    assert_eq!(pos(s, end_back, 1, false), 0);
}
