use ropey::Rope;

use super::*;

fn of(text: &str, at: usize, object: Object, inner: bool, count: usize) -> Option<(usize, usize)> {
    let r = Rope::from_str(text);
    span(object, inner, Text(&r), at, count).map(|s| (s.from, s.to))
}

const WORD: Object = Object::Word { big: false };
const PAREN: Object = Object::Pair { open: '(', close: ')' };

#[test]
fn a_word_is_its_run_and_a_around_word_takes_the_space() {
    assert_eq!(of("foo bar baz", 5, WORD, true, 1), Some((4, 7)));
    assert_eq!(of("foo bar baz", 5, WORD, false, 1), Some((4, 8)));
    assert_eq!(of("foo bar", 5, WORD, false, 1), Some((3, 7)), "the space before when none follows");
    assert_eq!(of("foo  bar", 3, WORD, true, 1), Some((3, 5)), "space is a run too");
    assert_eq!(of("foo  bar", 3, WORD, false, 1), Some((3, 8)), "and around it takes the word after");
}

#[test]
fn a_count_takes_that_many_words() {
    assert_eq!(of("a b c d", 0, WORD, false, 2), Some((0, 4)));
    assert_eq!(of("a b c d", 0, WORD, true, 3), Some((0, 3)));
}

#[test]
fn no_word_on_an_empty_line() {
    assert_eq!(of("a\n\nb", 2, WORD, true, 1), None);
}

#[test]
fn a_quoted_string_is_inside_or_around_its_quotes() {
    let q = Object::Quote('"');
    assert_eq!(of("x \"ab\" y", 4, q, true, 1), Some((3, 5)));
    assert_eq!(of("x \"ab\" y", 4, q, false, 1), Some((2, 7)));
    assert_eq!(of("x \"ab\"", 4, q, false, 1), Some((1, 6)), "the space before when none follows");
    assert_eq!(of("x \"ab", 4, q, true, 1), None, "an unclosed quote is none");
    assert_eq!(of("\"a\"\n\"b\"", 5, q, true, 1), Some((5, 6)), "never across lines");
}

#[test]
fn a_pair_is_what_encloses_the_cursor_or_the_next_one_ahead() {
    assert_eq!(of("(ab)", 2, PAREN, true, 1), Some((1, 3)));
    assert_eq!(of("(ab)", 2, PAREN, false, 1), Some((0, 4)));
    assert_eq!(of("(a(b)c)", 3, PAREN, true, 1), Some((3, 4)));
    assert_eq!(of("(a(b)c)", 3, PAREN, true, 2), Some((1, 6)));
    assert_eq!(of("(a(b)c)", 3, PAREN, true, 3), None);
    assert_eq!(of("x (ab)", 0, PAREN, true, 1), Some((3, 5)), "ahead when nothing encloses");
    assert_eq!(of("x ab", 0, PAREN, true, 1), None);
    assert_eq!(of("(ab", 1, PAREN, true, 1), None, "unclosed");
}

#[test]
fn a_block_on_lines_of_its_own_is_its_lines() {
    let b = Object::Pair { open: '{', close: '}' };
    assert_eq!(of("{\n  a\n}", 3, b, true, 1), Some((2, 6)));
    assert_eq!(of("{\n  a\n  }", 3, b, true, 1), Some((2, 6)));
    assert_eq!(of("{\n  a }", 3, b, true, 1), Some((2, 6)), "a close after text is no line of its own: the inner stops at the close");
}

#[test]
fn a_paragraph_is_lines_and_around_takes_the_blank_ones() {
    let t = "a\nb\n\nc";
    let p = |at, inner, count| {
        let r = Rope::from_str(t);
        span(Object::Paragraph, inner, Text(&r), at, count).map(|s| (s.from, s.to, s.linewise))
    };
    assert_eq!(p(0, true, 1), Some((0, 4, true)));
    assert_eq!(p(0, false, 1), Some((0, 5, true)));
    assert_eq!(p(4, true, 1), Some((4, 5, true)), "a blank line is a paragraph of blanks");
    assert_eq!(p(5, false, 1), Some((4, 6, true)), "no blank after: the one before comes");
}

#[test]
fn named_objects() {
    assert_eq!(Object::named('w'), Some(WORD));
    assert_eq!(Object::named('b'), Some(PAREN));
    assert_eq!(Object::named(')'), Some(PAREN));
    assert_eq!(Object::named('B'), Object::named('{'));
    assert_eq!(Object::named('x'), None);
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        // Not in any one source: an object is a span of the text on character
        // boundaries, from before to after, for any cursor.
        #[test]
        fn an_object_is_a_span_of_the_text(
            text in "[ab (){}\"é😀.\n]{0,30}",
            at in 0usize..40,
            which in 0usize..6,
            inner in any::<bool>(),
            count in 0usize..4,
        ) {
            let r = Rope::from_str(&text);
            let at = r.floor_char_boundary(at.min(text.len()));
            let object = [WORD, Object::Word { big: true }, Object::Quote('"'), PAREN, Object::Pair { open: '{', close: '}' }, Object::Paragraph][which];
            if let Some(s) = span(object, inner, Text(&r), at, count) {
                prop_assert!(s.from <= s.to && s.to <= text.len());
                prop_assert!(r.is_char_boundary(s.from) && r.is_char_boundary(s.to));
            }
        }
    }
}
