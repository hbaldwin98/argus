use proptest::prelude::*;
use ropey::Rope;

use super::*;

fn go(text: &str, from: At, count: usize, forward: bool, symbol: char) -> At {
    let r = Rope::from_str(text);
    find(Text(&r), from, count, forward, symbol)
}

#[test]
fn the_next_unmatched_closing_bracket_ahead_and_opening_one_behind() {
    assert_eq!(go("x { b } c", (0, 4), 1, true, '}'), (0, 6));
    assert_eq!(go("x { b } c", (0, 4), 1, false, '{'), (0, 2));
    assert_eq!(go("f(a, b)", (0, 3), 1, true, ')'), (0, 6));
    assert_eq!(go("f(a, b)", (0, 3), 1, false, '('), (0, 1));
}

#[test]
fn a_bracket_that_is_matched_ahead_is_passed_over() {
    assert_eq!(go("a { b } c", (0, 0), 1, true, '}'), (0, 0), "no unmatched one: nowhere");
    assert_eq!(go("x(y(z)w)v", (0, 2), 1, true, ')'), (0, 7));
}

#[test]
fn a_count_takes_that_many() {
    assert_eq!(go("x(y(z)w)v", (0, 4), 1, true, ')'), (0, 5));
    assert_eq!(go("x(y(z)w)v", (0, 4), 2, true, ')'), (0, 7));
    assert_eq!(go("x(y(z)w)v", (0, 4), 1, false, '('), (0, 3));
    assert_eq!(go("x(y(z)w)v", (0, 4), 2, false, '('), (0, 1));
}

#[test]
fn nothing_ahead_leaves_the_cursor() {
    assert_eq!(go("a b c", (0, 2), 1, true, '}'), (0, 2));
    assert_eq!(go("a b c", (0, 2), 1, false, '{'), (0, 2));
}

#[test]
fn a_symbol_that_is_none_of_the_known_ones_goes_nowhere() {
    assert_eq!(go("a {", (0, 0), 1, true, 'x'), (0, 0));
    assert_eq!(go("a {", (0, 0), 1, true, 'z'), (0, 0));
}

#[test]
fn a_brace_in_the_first_column_is_a_section() {
    let t = "a\n{\nb\n}\n{\nc\n}";
    assert_eq!(go(t, (0, 0), 1, true, ']'), (1, 0), "]]");
    assert_eq!(go(t, (1, 0), 1, true, ']'), (4, 0));
    assert_eq!(go(t, (6, 0), 1, false, '['), (4, 0), "[[");
    assert_eq!(go(t, (0, 0), 1, true, '['), (3, 0), "][ is the next close in column 0");
}

#[test]
fn with_no_section_ahead_the_walk_ends_at_the_text_end_and_the_start() {
    assert_eq!(go("a\nb\nc", (0, 0), 1, true, ']'), (3, 0));
    assert_eq!(go("a\nb\nc", (2, 0), 1, false, '['), (0, 0));
}

#[test]
fn a_comment_end_is_where_a_star_is_followed_by_a_slash() {
    assert_eq!(go("/* a */ b", (0, 0), 1, true, '/'), (0, 6));
    assert_eq!(go("x /* a */", (0, 8), 1, false, '*'), (0, 2));
}

#[test]
fn a_method_is_the_next_open_or_close_brace() {
    assert_eq!(go("a { b }", (0, 0), 1, true, 'm'), (0, 2));
    assert_eq!(go("a { b }", (0, 0), 1, true, 'M'), (0, 6));
    assert_eq!(go("a { b } c { d }", (0, 14), 1, false, 'm'), (0, 10));
}

#[test]
fn a_preprocessor_endif_is_found_from_inside_an_if() {
    let t = "x\n#if a\n#if b\n#endif\n#endif";
    assert_eq!(go(t, (2, 3), 1, true, '#'), (3, 0), "from inside an #if the next #endif");
    assert_eq!(go(t, (2, 3), 2, true, '#'), (4, 0));
    assert_eq!(go(t, (0, 0), 1, true, '#'), (0, 0), "from outside any, the #ifs passed are counted and none is left");
}

proptest! {
    // A walk ends in the text, whatever the symbol and the text.
    #[test]
    fn a_walk_ends_in_the_text(
        text in "[a{}()#/* \n\\[\\]if]{0,40}", line in 0usize..5, col in 0usize..8,
        count in 1usize..4, forward in any::<bool>(), symbol in "[(){}\\[\\]*/mM#x]"
    ) {
        let r = Rope::from_str(&text);
        let t = Text(&r);
        let line = line.min(t.last_line());
        let col = col.min(t.line_len(line).saturating_sub(1));
        let symbol = symbol.chars().next().unwrap();
        let (l, c) = find(t, (line, col), count, forward, symbol);
        let to = t.at(l.min(t.last_line()), c);
        prop_assert!(to <= t.len() && r.is_char_boundary(to));
    }
}
