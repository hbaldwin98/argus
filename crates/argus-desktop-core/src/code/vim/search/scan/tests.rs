use proptest::prelude::*;

use super::*;
use crate::code::vim::search::pattern::parse_query;

fn q(pattern: &str) -> Query {
    parse_query(pattern, false, false, true).unwrap().unwrap()
}

#[test]
fn a_cursor_finds_the_next_match_from_where_it_was_put_and_then_the_one_after() {
    let query = q("a+");
    let mut c = Cursor::new(&query, "xa aa a", 0);
    assert!(c.find(false));
    assert_eq!(c.found(), Some((1, 2)));
    assert!(c.find(false));
    assert_eq!(c.found(), Some((3, 5)));
    assert!(c.find(false));
    assert_eq!(c.found(), Some((6, 7)));
    assert!(!c.find(false));
    assert_eq!(c.found(), Some((6, 7)), "the last one found stays");
}

#[test]
fn a_cursor_put_inside_a_match_finds_the_next_not_that_one() {
    let query = q("aa");
    let mut c = Cursor::new(&query, "aa aa", 1);
    assert!(c.find(false));
    assert_eq!(c.found(), Some((3, 5)));
}

#[test]
fn going_back_it_finds_the_last_match_that_ends_before_where_it_was() {
    let query = q("a+");
    let mut c = Cursor::new(&query, "xa aa a", 7);
    assert!(c.find(true));
    assert_eq!(c.found(), Some((6, 7)));
    assert!(c.find(true));
    assert_eq!(c.found(), Some((3, 5)));
    assert!(c.find(true));
    assert_eq!(c.found(), Some((1, 2)));
    assert!(!c.find(true));
    let mut c = Cursor::new(&query, "xa aa a", 4);
    assert!(c.find(true));
    assert_eq!(c.found(), Some((1, 2)), "a match the cursor is inside does not end before it");
}

#[test]
fn an_empty_match_is_stepped_past_a_character_at_a_time() {
    let query = q("x*");
    let mut c = Cursor::new(&query, "ab", 0);
    let mut found = Vec::new();
    while c.find(false) {
        found.push(c.found().unwrap());
    }
    assert_eq!(found, vec![(0, 0), (1, 1), (2, 2)]);
}

#[test]
fn a_cursor_steps_past_a_wide_character_whole() {
    let query = q("x*");
    let mut c = Cursor::new(&query, "é😀", 0);
    let mut found = Vec::new();
    while c.find(false) {
        found.push(c.found().unwrap());
    }
    assert_eq!(found, vec![(0, 0), (2, 2), (6, 6)]);
}

#[test]
fn n_skips_the_match_the_cursor_is_at_the_start_of_and_wraps() {
    let query = q("foo");
    let text = "foo x foo y foo";
    assert_eq!(find_next(&query, text, 0, false, 1), Some(6));
    assert_eq!(find_next(&query, text, 6, false, 1), Some(12));
    assert_eq!(find_next(&query, text, 12, false, 1), Some(0), "round the end");
    assert_eq!(find_next(&query, text, 7, false, 1), Some(12));
    assert_eq!(find_next(&query, text, 0, false, 2), Some(12));
    assert_eq!(find_next(&query, text, 0, false, 3), Some(0));
}

#[test]
fn backward_n_goes_to_a_match_that_ends_before_the_cursor_and_wraps() {
    let query = q("foo");
    let text = "foo x foo y foo";
    assert_eq!(find_next(&query, text, 12, true, 1), Some(6));
    assert_eq!(find_next(&query, text, 6, true, 1), Some(0));
    assert_eq!(find_next(&query, text, 0, true, 1), Some(12), "round the start");
    assert_eq!(find_next(&query, text, 14, true, 1), Some(6), "inside the last: the one before");
}

#[test]
fn no_match_is_none_even_after_wrapping() {
    let query = q("zzz");
    assert_eq!(find_next(&query, "abc", 1, false, 1), None);
    assert_eq!(find_next(&query, "abc", 1, true, 1), None);
    assert_eq!(find_next_inclusive(&query, "abc", 1, false, 1, false), None);
}

#[test]
fn gn_takes_the_match_the_cursor_is_in_or_the_next_one() {
    let query = q("foo");
    let text = "foo x foo y foo";
    assert_eq!(find_next_inclusive(&query, text, 7, false, 1, false), Some((6, 9)));
    assert_eq!(find_next_inclusive(&query, text, 9, false, 1, false), Some((12, 15)), "after the match: the next");
    assert_eq!(find_next_inclusive(&query, text, 0, false, 1, false), Some((0, 3)), "at a start: that one");
    assert_eq!(find_next_inclusive(&query, text, 4, false, 2, false), Some((12, 15)));
    assert_eq!(find_next_inclusive(&query, text, 7, true, 1, false), Some((6, 9)), "backward in a match with one after it: that match");
    assert_eq!(find_next_inclusive(&query, text, 13, true, 1, false), Some((6, 9)), "in the last, with none after: the one before");
}

#[test]
fn every_match_is_listed_in_order() {
    assert_eq!(all(&q("a"), "banana"), vec![(1, 2), (3, 4), (5, 6)]);
    assert_eq!(all(&q("na"), "banana"), vec![(2, 4), (4, 6)]);
    assert!(all(&q("z"), "banana").is_empty());
}

proptest! {
    // `n` lands on a match, or nowhere if the text has none; and whatever the count and way.
    #[test]
    fn n_lands_on_a_match_or_nowhere(
        text in "[ab \n]{0,30}", pos in 0usize..30, prev in any::<bool>(), count in 1usize..4
    ) {
        let query = q("ab?");
        let pos = pos.min(text.len());
        let starts: Vec<usize> = all(&query, &text).into_iter().map(|(s, _)| s).collect();
        match find_next(&query, &text, pos, prev, count) {
            Some(at) => prop_assert!(starts.contains(&at), "{at} in {starts:?}"),
            None => prop_assert!(starts.is_empty()),
        }
    }

    // The matches are in order, apart, and each is a match of the pattern.
    #[test]
    fn the_matches_are_in_order_and_apart(text in "[ab \n]{0,30}") {
        let query = q("a+|b");
        let found = all(&query, &text);
        for w in found.windows(2) {
            prop_assert!(w[0].1 <= w[1].0);
        }
        for (s, e) in found {
            prop_assert!(s <= e && e <= text.len());
        }
    }
}
