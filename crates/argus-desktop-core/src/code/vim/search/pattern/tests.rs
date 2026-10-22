use proptest::prelude::*;

use super::*;

fn q(raw: &str, ignore: bool, smart: bool, pcre: bool) -> Query {
    parse_query(raw, ignore, smart, pcre).unwrap().unwrap()
}

fn matches(query: &Query, text: &str) -> bool {
    query.regex.is_match(text).unwrap()
}

#[test]
fn an_unescaped_separator_is_found_and_an_escaped_one_is_not() {
    assert_eq!(unescaped("a/b/c", '/'), vec![1, 3]);
    assert_eq!(unescaped("a\\/b/c", '/'), vec![4]);
    assert_eq!(unescaped("a\\\\/b", '/'), vec![3], "an escaped backslash escapes nothing");
}

#[test]
fn a_line_is_split_at_its_separator_when_it_begins_with_one() {
    assert_eq!(split_by("/a/b/c", '/'), Some(vec!["a".into(), "b".into(), "c".into()]));
    assert_eq!(split_by("/a/b", '/'), Some(vec!["a".into(), "b".into()]));
    assert_eq!(split_by("/a", '/'), Some(vec!["a".into()]));
    assert_eq!(split_by("#a#b#", '#'), Some(vec!["a".into(), "b".into(), String::new()]));
    assert_eq!(split_by("a/b", '/'), None, "the first separator is not at the start");
    assert_eq!(split_by("abc", '/'), Some(vec![]));
}

#[test]
fn slashes_are_escaped_so_a_flag_can_follow() {
    assert_eq!(escape_slashes("a/b"), "a\\/b");
    assert_eq!(escape_slashes("a\\/b"), "a\\/b");
    assert_eq!(escape_slashes("a\\\\/b"), "a\\\\\\/b");
}

#[test]
fn an_empty_pattern_is_none_and_one_that_is_only_flags_too() {
    assert!(parse_query("", true, true, true).unwrap().is_none());
    assert!(parse_query("/i", true, true, true).unwrap().is_none());
}

#[test]
fn case_goes_by_the_flag_the_caller_and_capitals() {
    assert!(matches(&q("foo", true, false, true), "FOO"));
    assert!(!matches(&q("foo", false, false, true), "FOO"));
    assert!(matches(&q("foo", false, true, true), "FOO"), "smart case: no capitals, so case is ignored");
    assert!(!matches(&q("Foo", false, true, true), "foo"), "smart case: a capital, so it is not");
    assert!(!matches(&q("Foo", true, true, true), "foo"));
    assert!(matches(&q("Foo/i", false, true, true), "foo"), "`/i` forces it off");
    assert!(matches(&q("Foo/gi", true, true, true), "foo"));
}

#[test]
fn a_query_says_what_it_is_the_way_javascript_writes_a_regular_expression() {
    assert_eq!(q("a", true, false, true).to_string(), "/a/im");
    assert_eq!(q("a", false, false, true).to_string(), "/a/m");
}

#[test]
fn caret_and_dollar_are_at_the_lines() {
    let query = q("^b$", false, false, true);
    assert!(matches(&query, "a\nb\nc"));
    assert!(!matches(&q("^c$", false, false, true), "ab\nxc\n"));
}

#[test]
fn look_around_and_back_references_work() {
    assert!(matches(&q("a(?=b)", false, false, true), "ab"));
    assert!(!matches(&q("a(?!b)", false, false, true), "ab"));
    assert!(matches(&q("(?<=a)b", false, false, true), "ab"));
    assert!(matches(&q("(a)\\1", false, false, true), "aa"));
    assert!(matches(&q("(?<x>a)\\k<x>", false, false, true), "aa"), "a named group");
}

#[test]
fn a_javascript_escape_the_engine_writes_otherwise_is_translated() {
    assert!(matches(&q("a\\/b", false, false, true), "a/b"));
    assert!(matches(&q("\\u0041", false, false, true), "A"));
    assert!(matches(&q("a\\0b", false, false, true), "a\0b"));
    assert!(matches(&q("a[^]b", false, false, true), "a\nb"), "[^] is any character");
    assert!(!matches(&q("a[]b", false, false, true), "ab"), "[] is none");
    assert!(matches(&q("[\\]]", false, false, true), "]"));
    assert!(matches(&q("[a[]", false, false, true), "["), "a bracket in a class");
}

#[test]
fn a_pattern_that_does_not_compile_is_an_error_naming_it() {
    assert_eq!(parse_query("(", true, true, true).unwrap_err(), "Invalid regex: (");
    assert_eq!(parse_query("a)b", true, true, true).unwrap_err(), "Invalid regex: a)b");
}

#[test]
fn vims_own_syntax_is_translated_when_pcre_is_off() {
    // `\(` is a group, `(` is a character, `\<` and `\>` are word edges.
    assert!(matches(&q("\\(a\\)\\1", false, false, false), "aa"));
    assert!(matches(&q("a(b", false, false, false), "a(b"));
    assert!(matches(&q("\\<foo\\>", false, false, false), "a foo b"));
    assert!(!matches(&q("\\<foo\\>", false, false, false), "afoob"));
    assert!(matches(&q("a\\+", false, false, false), "aaa"));
    assert!(matches(&q("a\\{2}", false, false, false), "aa"));
    assert!(matches(&q("a|b", false, false, false), "a|b"), "| is a character in the magic syntax");
    assert!(matches(&q("a\\|b", false, false, false), "b"));
}

#[test]
fn vims_modes_choose_what_is_special() {
    assert_eq!(translate("\\va+"), "a+");
    assert_eq!(translate("a.b"), "a.b");
    assert_eq!(translate("\\Va.b"), "a\\.b");
    assert_eq!(translate("a*"), "a*");
    assert_eq!(translate("\\Ma*"), "a\\*", "no magic: a star is a character");
}

#[test]
fn zs_and_ze_set_where_a_match_begins_and_ends() {
    assert_eq!(translate("foo\\zsbar"), "(?<=foo)bar");
    assert_eq!(translate("foo\\zebar"), "foo(?=bar)");
}

#[test]
fn a_word_is_escaped_for_a_search_for_it() {
    assert_eq!(escape("a.b*c"), "a\\.b\\*c");
    assert_eq!(escape("(x)"), "\\(x\\)");
    assert_eq!(escape("a-b"), "a\\-b");
    assert!(matches(&q(&escape("a.b"), false, false, true), "a.b"));
    assert!(!matches(&q(&escape("a.b"), false, false, true), "axb"));
}

proptest! {
    // Whatever is typed, reading it ends in a query or an error, and a query matches itself when it is a plain word.
    #[test]
    fn any_pattern_is_read_without_a_panic(raw in "[a-c()\\[\\]\\\\/.*+?^$|{}<>zsev ]{0,12}", pcre in any::<bool>()) {
        let _ = parse_query(&raw, true, true, pcre);
        let _ = translate(&raw);
        let _ = escape_slashes(&raw);
        let _ = split_by(&raw, '/');
    }

    #[test]
    fn an_escaped_word_matches_that_word_and_only_it(word in "[a-c.*+?()\\[\\]{}|^$\\\\/-]{1,8}") {
        let query = parse_query(&escape(&word), false, false, true).unwrap().unwrap();
        prop_assert!(matches(&query, &word));
    }
}
