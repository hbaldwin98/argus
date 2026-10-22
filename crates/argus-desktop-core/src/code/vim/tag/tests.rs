use proptest::prelude::*;
use ropey::Rope;

use super::*;

/// The element `count` out from `at`, as the text of its tags and body.
fn the(text: &str, at: usize, inner: bool, count: usize) -> Option<String> {
    let r = Rope::from_str(text);
    object(Text(&r), at, inner, count).map(|(a, b)| text[a..b].to_owned())
}

#[test]
fn an_element_is_its_tags_and_the_text_between() {
    assert_eq!(the("<a>x</a>", 3, false, 1).as_deref(), Some("<a>x</a>"));
    assert_eq!(the("<a>x</a>", 3, true, 1).as_deref(), Some("x"));
}

#[test]
fn the_innermost_element_is_first_and_a_count_goes_outward() {
    let t = "<a><b>x</b></a>";
    assert_eq!(the(t, 6, false, 1).as_deref(), Some("<b>x</b>"));
    assert_eq!(the(t, 6, false, 2).as_deref(), Some(t));
    assert_eq!(the(t, 6, true, 2).as_deref(), Some("<b>x</b>"));
    assert_eq!(the(t, 6, false, 3), None);
}

#[test]
fn a_position_on_either_tag_is_in_the_element() {
    let t = "<a>x</a>";
    for at in [0, 1, 2, 5, 6, 7] {
        assert_eq!(the(t, at, true, 1).as_deref(), Some("x"), "{at}");
    }
    assert_eq!(the(t, 8.min(t.len()), true, 1), None, "past the end of the close tag");
}

#[test]
fn attributes_may_hold_a_greater_than_sign_in_quotes() {
    assert_eq!(the("<a title=\"x>y\">z</a>", 15, true, 1).as_deref(), Some("z"));
    assert_eq!(the("<a title='x>y'>z</a>", 15, true, 1).as_deref(), Some("z"));
}

#[test]
fn void_and_self_closing_tags_have_no_body() {
    assert_eq!(the("<p>a<br>b</p>", 6, true, 1).as_deref(), Some("a<br>b"));
    assert_eq!(the("<p><x/>b</p>", 8, true, 1).as_deref(), Some("<x/>b"));
    assert_eq!(the("<img>", 2, true, 1), None);
}

#[test]
fn comments_declarations_and_instructions_are_skipped() {
    assert_eq!(the("<a><!-- </a> -->x</a>", 18, true, 1).as_deref(), Some("<!-- </a> -->x"));
    assert_eq!(the("<!DOCTYPE html><a>x</a>", 19, true, 1).as_deref(), Some("x"));
    assert_eq!(the("<?xml version=\"1.0\"?><a>x</a>", 25, true, 1).as_deref(), Some("x"));
    assert_eq!(the("<a><![CDATA[ </a> ]]>x</a>", 24, true, 1).as_deref(), Some("<![CDATA[ </a> ]]>x"));
}

#[test]
fn names_are_matched_without_regard_to_case_and_a_close_with_no_open_is_ignored() {
    assert_eq!(the("<DIV>x</div>", 6, true, 1).as_deref(), Some("x"));
    assert_eq!(the("</b><a>x</a>", 8, true, 1).as_deref(), Some("x"));
    assert_eq!(the("<a>x</b></a>", 4, true, 1).as_deref(), Some("x</b>"));
}

#[test]
fn an_element_never_closed_is_no_element() {
    assert_eq!(the("<a>x", 3, true, 1), None);
    assert_eq!(the("<a", 1, true, 1), None);
}

#[test]
fn a_close_tag_closes_the_nearest_open_of_its_name_and_what_was_left_open_inside_it() {
    assert_eq!(the("<a><b>x</a>", 6, true, 1).as_deref(), Some("<b>x"));
}

proptest! {
    // However the text, an element found holds the position, on characters, in order.
    #[test]
    fn an_element_found_is_a_span_of_the_text_around_the_position(
        text in "[<>/ab!\\-? \"']{0,40}", at in 0usize..40, inner in any::<bool>(), count in 1usize..4
    ) {
        let r = Rope::from_str(&text);
        let at = r.floor_char_boundary(at.min(text.len()));
        if let Some((from, to)) = object(Text(&r), at, inner, count) {
            prop_assert!(from <= to && to <= text.len());
            prop_assert!(text.is_char_boundary(from) && text.is_char_boundary(to));
            // The whole element has the position in it, or just before it, on a `<`.
            if !inner {
                prop_assert!(from <= at + 1 && at < to + 1);
            }
        }
    }
}
