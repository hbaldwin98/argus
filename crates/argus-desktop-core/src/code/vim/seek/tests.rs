use ropey::Rope;

use super::*;
use crate::code::vim::tests::Ed;

fn word(text: &str, at: usize, words_only: bool) -> Option<(String, bool)> {
    let r = Rope::from_str(text);
    under_cursor(Text(&r), at, words_only).map(|(from, to, keyword)| (text[from..to].to_owned(), keyword))
}

#[test]
fn the_word_is_the_run_of_word_characters_the_cursor_is_in() {
    assert_eq!(word("foo bar", 1, true), Some(("foo".into(), true)));
    assert_eq!(word("foo bar", 4, true), Some(("bar".into(), true)));
    assert_eq!(word("foo_1 x", 0, true), Some(("foo_1".into(), true)), "an underscore and a digit are word characters");
    assert_eq!(word("éa x", 0, true), Some(("éa".into(), true)), "so is a letter that is not ASCII");
}

#[test]
fn off_a_word_it_is_the_first_word_after_on_the_line() {
    assert_eq!(word("  foo bar", 0, true), Some(("foo".into(), true)));
    assert_eq!(word("-- foo", 0, true), Some(("foo".into(), true)));
    assert_eq!(word("foo\nbar", 3, true), None, "never the next line");
}

#[test]
fn with_no_word_ahead_the_run_of_symbols_is_it() {
    assert_eq!(word("++ --", 0, false), Some(("++".into(), false)));
    assert_eq!(word("  ++", 0, false), Some(("++".into(), false)));
    assert_eq!(word("   ", 0, false), None);
    assert_eq!(word("", 0, false), None);
}

#[test]
fn a_symbol_run_stops_at_a_word_or_a_blank() {
    assert_eq!(word("+-a", 0, false), Some(("+-".into(), false)));
    assert_eq!(word("+- a", 1, false), Some(("+-".into(), false)));
}

#[test]
fn the_search_for_a_word_has_the_word_set_as_the_pattern_and_the_cursor_at_its_start() {
    let mut ed = Ed::new("a fo|o b foo");
    ed.keys("*");
    assert_eq!(ed.sh.search.query.as_ref().unwrap().source, "\\bfoo\\b");
    assert!(!ed.sh.search.reversed);
    ed.keys("#");
    assert!(ed.sh.search.reversed);
    assert_eq!(ed.sh.regs.get(Some('/')).unwrap().text, "\\bfoo\\b");
}

#[test]
fn a_symbol_search_is_escaped_and_not_by_word() {
    let mut ed = Ed::new("|+(+ +(+");
    ed.keys("*");
    assert_eq!(ed.sh.search.query.as_ref().unwrap().source, "\\+\\(\\+");
    let mut ed = Ed::new("|foo-bar foo");
    ed.keys("g*");
    assert_eq!(ed.sh.search.query.as_ref().unwrap().source, "foo");
}

#[test]
fn star_with_no_word_clears_nothing_and_does_not_move() {
    let mut ed = Ed::new("a|  \nb");
    ed.keys("*");
    assert_eq!(ed.shows(), "a|  \nb");
    assert_eq!(ed.vim.message(), Some("No word under cursor"));
}

#[test]
fn a_search_line_run_in_a_macro_goes_by_capitals_no_longer() {
    let mut ed = Ed::new("|a Foo foo");
    ed.sh.regs.set_macro('m', "/Foo<CR>");
    ed.keys("@m");
    assert!(ed.sh.search.query.as_ref().unwrap().ignore_case, "while playing, smart case is off");
    let mut ed = Ed::new("|a Foo foo");
    ed.keys("/Foo<CR>");
    assert!(!ed.sh.search.query.as_ref().unwrap().ignore_case, "typed: a capital makes it exact");
}
