use super::*;

fn with(lines: &[&str]) -> Recall {
    let mut r = Recall::default();
    for l in lines {
        r.push(l);
        r.reset();
    }
    r
}

#[test]
fn up_goes_through_the_lines_newest_first_and_down_comes_back() {
    let mut r = with(&["one", "two", "three"]);
    assert_eq!(r.next_match("", true), "three");
    assert_eq!(r.next_match("", true), "two");
    assert_eq!(r.next_match("", false), "three");
    assert_eq!(r.next_match("", false), "", "past the newest it is what was typed");
}

#[test]
fn only_the_lines_that_begin_with_what_was_typed_are_walked() {
    let mut r = with(&["abc", "xyz", "abd"]);
    assert_eq!(r.next_match("ab", true), "abd");
    assert_eq!(r.next_match("abd", true), "abc", "the prefix stays what the walk began with");
    assert_eq!(r.next_match("abc", true), "abc", "past the oldest it stays");
}

#[test]
fn a_walk_that_begins_over_starts_from_the_newest() {
    let mut r = with(&["a", "b"]);
    r.next_match("", true);
    r.next_match("", true);
    r.reset();
    assert_eq!(r.next_match("", true), "b");
}

#[test]
fn a_line_that_is_run_again_moves_to_the_end_and_an_empty_one_is_not_kept() {
    let mut r = with(&["a", "b", "a", ""]);
    assert_eq!(r.next_match("", true), "a");
    assert_eq!(r.next_match("", true), "b");
    assert_eq!(r.next_match("b", true), "b", "two lines are all there are: past the oldest it stays");
}

#[test]
fn with_no_lines_the_typed_text_is_all_there_is() {
    let mut r = Recall::default();
    assert_eq!(r.next_match("x", true), "x");
    assert_eq!(r.next_match("x", false), "x");
}
