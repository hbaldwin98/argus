use ropey::Rope;

use super::*;

fn reach(to: usize, inclusive: bool, linewise: bool) -> Reach {
    Reach { to, linewise, inclusive, far: false, vertical: false }
}

#[test]
fn an_inclusive_reach_takes_its_last_character_but_not_a_line_break() {
    let r = Rope::from_str("abc\n\nx");
    let t = Text(&r);
    assert_eq!(of_reach(t, 0, reach(1, true, false)), Region { from: 0, to: 2, linewise: false });
    assert_eq!(of_reach(t, 4, reach(4, true, false)), Region { from: 4, to: 4, linewise: false });
}

#[test]
fn a_linewise_reach_takes_both_lines_whole() {
    let r = Rope::from_str("a\nb\nc");
    let t = Text(&r);
    assert_eq!(of_reach(t, 0, reach(2, true, true)), Region { from: 0, to: 4, linewise: true });
    assert_eq!(of_reach(t, 4, reach(4, true, true)), Region { from: 4, to: 5, linewise: true });
}

#[test]
fn an_exclusive_reach_ending_at_a_line_start_stops_before_the_break() {
    // `dw` on the last word: the line break stays.
    let r = Rope::from_str("one two\nthree");
    let t = Text(&r);
    assert_eq!(of_reach(t, 4, reach(8, false, false)), Region { from: 4, to: 7, linewise: false });
}

#[test]
fn and_from_a_line_start_it_takes_the_lines() {
    let r = Rope::from_str("a\nb\n\nc");
    let t = Text(&r);
    assert_eq!(of_reach(t, 0, reach(4, false, false)), Region { from: 0, to: 4, linewise: true });
}

#[test]
fn a_reach_backwards_is_the_same_span() {
    let r = Rope::from_str("abcdef");
    let t = Text(&r);
    assert_eq!(of_reach(t, 4, reach(1, false, false)), Region { from: 1, to: 4, linewise: false });
}

#[test]
fn a_selection_takes_both_ends_and_lines_take_whole_lines() {
    let r = Rope::from_str("é1\nbc\nd");
    let t = Text(&r);
    assert_eq!(of_selection(t, 3, 0, false), Region { from: 0, to: 4, linewise: false });
    assert_eq!(of_selection(t, 1, 5, true), Region { from: 0, to: 7, linewise: true });
}

#[test]
fn the_lines_a_region_touches_do_not_include_the_one_it_ends_at_the_start_of() {
    let r = Rope::from_str("a\nb\nc");
    let t = Text(&r);
    assert_eq!(Region { from: 0, to: 2, linewise: true }.lines(t), (0, 0));
    assert_eq!(Region { from: 0, to: 4, linewise: true }.lines(t), (0, 1));
    assert_eq!(Region { from: 1, to: 1, linewise: false }.lines(t), (0, 0));
}
