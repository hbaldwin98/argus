use fancy_regex::Regex;
use proptest::prelude::*;

use super::*;
use crate::code::vim::tests::Ed;

fn expanded(pattern: &str, text: &str, replacement: &str) -> String {
    let re = Regex::new(pattern).unwrap();
    let caps = re.captures(text).unwrap().unwrap();
    expand(replacement, &caps)
}

#[test]
fn a_pcre_replacement_unescapes_a_backslash_n_r_t_slash_and_ampersand() {
    assert_eq!(unescape_replacement("a\\nb"), "a\nb");
    assert_eq!(unescape_replacement("a\\rb"), "a\rb");
    assert_eq!(unescape_replacement("a\\tb"), "a\tb");
    assert_eq!(unescape_replacement("a\\/b"), "a/b");
    assert_eq!(unescape_replacement("a\\\\b"), "a\\b");
    assert_eq!(unescape_replacement("a\\&b"), "a&b");
    assert_eq!(unescape_replacement("a\\1b"), "a\\1b", "any other backslash stays");
    assert_eq!(unescape_replacement("a\\"), "a\\");
}

#[test]
fn a_vim_replacement_turns_backslash_digits_into_groups() {
    assert_eq!(translate_replacement("\\1x\\2"), "$1x$2");
    assert_eq!(translate_replacement("a\\nb"), "a\nb");
    assert_eq!(translate_replacement("a\\tb"), "a\tb");
    assert_eq!(translate_replacement("a\\/b"), "a/b");
    assert_eq!(translate_replacement("$x"), "$$x", "a dollar is a character");
    assert_eq!(translate_replacement("a\\\\b"), "a\\b");
}

#[test]
fn the_match_and_the_groups_are_put_in_a_replacement() {
    assert_eq!(expanded("(a)(b)", "ab", "$2$1"), "ba");
    assert_eq!(expanded("(a)(b)", "ab", "[$&]"), "[ab]");
    assert_eq!(expanded("(a)(b)", "ab", "$$"), "$");
    assert_eq!(expanded("(a)(b)", "ab", "$0"), "ab");
}

#[test]
fn a_group_that_is_not_there_is_left_as_written_or_loses_digits() {
    assert_eq!(expanded("(a)", "a", "$3"), "$3", "no group three");
    assert_eq!(expanded("(a)", "a", "$12"), "a2", "group one, then a two");
    assert_eq!(expanded("(a)", "a", "$$1"), "$1");
    assert_eq!(expanded("(a)|(b)", "a", "[$2]"), "[]", "a group that did not take part is nothing");
    assert_eq!(expanded("a", "a", "cost: $"), "cost: $");
}

#[test]
fn a_substitute_with_a_replacement_but_no_pattern_to_use_says_so() {
    let mut ed = Ed::new("|abc");
    ed.sh.search.replacement = Some("x".into());
    ed.keys(":s<CR>");
    assert_eq!(ed.vim.message(), Some("No previous regular expression"));
}

#[test]
fn matches_that_run_together_are_replaced_whole() {
    let mut ed = Ed::new("|aaaa");
    ed.keys(":s/aa/b/g<CR>");
    assert_eq!(ed.text(), "bb");
}

#[test]
fn an_empty_match_is_replaced_once_and_not_twice_in_a_row() {
    let mut ed = Ed::new("|ab");
    ed.keys(":s/x*/-/g<CR>");
    assert_eq!(ed.text(), "-a-b-");
}

#[test]
fn a_read_only_file_is_not_changed() {
    let mut ed = Ed::new("|aaa");
    ed.vim.set_read_only(true);
    ed.keys(":s/a/b/g<CR>");
    assert_eq!(ed.text(), "aaa");
}

#[test]
fn the_cursor_goes_to_where_the_last_replaced_match_began_after_text_that_changed_length() {
    let mut ed = Ed::new("|a a a");
    ed.keys(":s/a/xyz/g<CR>");
    assert_eq!(ed.text(), "xyz xyz xyz");
    assert_eq!(ed.vim.cursor(), 8);
}

proptest! {
    // Replacing each match with itself changes nothing, however the text and the pattern.
    #[test]
    fn replacing_a_match_with_itself_leaves_the_text(text in "[ab \n]{1,20}", pat in "a+|b|ab") {
        let mut ed = Ed::new(&text);
        ed.keys(&format!(":%s/{pat}/&/g<CR>"));
        prop_assert_eq!(ed.text(), text);
    }

    // Deleting every match leaves none of it, and what is left is the text between the matches.
    #[test]
    fn deleting_every_match_leaves_no_match(text in "[ab]{1,20}") {
        let mut ed = Ed::new(&text);
        ed.keys(":s/a//g<CR>");
        prop_assert_eq!(ed.text(), text.replace('a', ""));
    }
}
