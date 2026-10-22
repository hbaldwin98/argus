//! Searching (EDITOR-VIM-7): `/ ? n N * # g* g# gn gN`, the line typed, and
//! the rules the library's search cursor and smart case give them. The
//! expectations are Vim's where the library follows it, and the library's
//! where it does not (a backward search goes to a match that ends before the
//! cursor, and `*` goes by neither capitals nor `ignorecase`).

use super::*;

#[test]
fn a_search_goes_to_the_next_match_and_n_and_capital_n_walk_the_matches() {
    table(&[
        ("|a foo b foo", "/foo<CR>", "a |foo b foo"),
        ("|a foo b foo", "/foo<CR>n", "a foo b |foo"),
        ("|a foo b foo", "/foo<CR>nn", "a |foo b foo"),
        ("|a foo b foo", "/foo<CR>N", "a foo b |foo"),
        ("|a foo b foo", "/foo<CR>nN", "a |foo b foo"),
        ("|a foo b foo", "2/foo<CR>", "a foo b |foo"),
        ("|a foo b foo", "/foo<CR>2n", "a |foo b foo"),
    ]);
}

#[test]
fn a_backward_search_goes_to_the_last_match_that_ends_before_the_cursor() {
    table(&[
        ("a foo b foo|", "?foo<CR>", "a |foo b foo"),
        ("a foo b |foo", "?foo<CR>", "a |foo b foo"),
        ("a foo b |b", "?foo<CR>", "a |foo b b"),
        // `n` after `?` goes on backward, and `N` turns back.
        ("a foo b foo c foo|", "?foo<CR>n", "a |foo b foo c foo"),
        ("a foo b foo c foo|", "?foo<CR>N", "a foo b foo c |foo"),
    ]);
}

#[test]
fn n_wraps_round_the_ends_of_the_text() {
    table(&[("a foo|", "/foo<CR>", "a |foo"), ("|foo a", "?foo<CR>", "|foo a"), ("foo a|", "/foo<CR>", "|foo a")]);
}

#[test]
fn a_pattern_without_capitals_ignores_case_and_one_with_does_not() {
    table(&[
        ("|Foo foo FOO", "/foo<CR>", "Foo |foo FOO"),
        ("|Foo foo FOO", "/Foo<CR>", "|Foo foo FOO"),
        ("|a Foo foo", "/Foo<CR>", "a |Foo foo"),
        ("|a Foo foo", "/foo<CR>", "a |Foo foo"),
        ("|a Foo foo", "/FOO<CR>", "|a Foo foo"),
    ]);
}

#[test]
fn a_slash_i_after_the_pattern_forces_case_off() {
    table(&[("|a FOO foo", "/Foo/i<CR>", "a |FOO foo")]);
}

#[test]
fn the_pattern_is_a_regular_expression() {
    table(&[
        ("|a1 b22 c", "/[0-9]+<CR>", "a|1 b22 c"),
        ("|a1 b22 c", "/b\\d+<CR>", "a1 |b22 c"),
        ("|xay xby", "/x(?=b)<CR>", "xay |xby"),
        ("|abc abd", "/ab(?!c)<CR>", "abc |abd"),
        ("|abab", "/(ab)\\1<CR>", "|abab"),
        ("|a.b axb", "/a\\.b<CR>", "|a.b axb"),
        ("|a/b a b", "/a\\/b<CR>", "|a/b a b"),
    ]);
}

#[test]
fn a_pattern_that_matches_nothing_says_so_and_stays_put() {
    let mut ed = Ed::new("|abc");
    ed.keys("/xyz<CR>");
    assert_eq!(ed.shows(), "|abc");
    assert_eq!(ed.vim.message(), Some("No match found /xyz/im (set nopcre to use Vim regexps)"));
}

#[test]
fn a_pattern_that_does_not_compile_says_so() {
    let mut ed = Ed::new("|abc");
    ed.keys("/(<CR>");
    assert_eq!(ed.vim.message(), Some("Invalid regex: ("));
    assert_eq!(ed.shows(), "|abc");
}

#[test]
fn n_with_no_search_does_nothing() {
    table(&[("a|bc", "n", "a|bc"), ("a|bc", "N", "a|bc")]);
}

#[test]
fn the_word_under_the_cursor_is_searched_for_whole_by_star_and_hash() {
    table(&[
        ("|foo bar foo baz foo", "*", "foo bar |foo baz foo"),
        ("|foo bar foo baz foo", "**", "foo bar foo baz |foo"),
        ("|foo bar foo baz foo", "#", "foo bar foo baz |foo"),
        ("foo bar |foo baz foo", "#", "|foo bar foo baz foo"),
        ("f|oo bar foo", "*", "foo bar |foo"),
        ("|foo foobar foo", "*", "foo foobar |foo"),
        ("|foo foobar foo", "g*", "foo |foobar foo"),
        ("foo foobar |foo", "g#", "foo |foobar foo"),
        // Not on a word: the next one on the line.
        ("|  foo bar foo", "*", "  foo bar |foo"),
        ("|-- x -- x", "*", "-- x -- |x"),
    ]);
}

#[test]
fn star_goes_by_neither_capitals_nor_smart_case() {
    table(&[("|foo Foo foo", "*", "foo |Foo foo")]);
}

#[test]
fn a_symbol_with_no_word_after_it_is_searched_for_as_it_is() {
    table(&[("|++ ++ ++", "*", "++ |++ ++"), ("|++ a ++", "*", "++ |a ++")]);
}

#[test]
fn star_on_a_line_with_no_word_says_so() {
    let mut ed = Ed::new("|   ");
    ed.keys("*");
    assert_eq!(ed.vim.message(), Some("No word under cursor"));
}

#[test]
fn an_operator_runs_up_to_the_match() {
    table(&[
        ("|a foo b", "d/foo<CR>", "|foo b"),
        ("|a foo b foo", "d2/foo<CR>", "|foo"),
        ("a foo b |foo", "d?foo<CR>", "a |foo"),
        ("|a foo b", "c/foo<CR>X<Esc>", "|Xfoo b"),
        ("|a foo b", "y/foo<CR>P", "a| a foo b"),
        ("|foo bar foo", "d*", "|foo"),
        ("|foo x foo", "dn", "|foo x foo"),
        ("|foo x foo", "/foo<CR>gg0dn", "|foo"),
    ]);
}

#[test]
fn gn_selects_the_match_the_cursor_is_in_or_the_next() {
    let mut ed = Ed::new("|foo bar foo");
    ed.keys("/bar<CR>gg0gn");
    assert_eq!(ed.mode(), Mode::Visual);
    assert_eq!(ed.vim.selection(), Some((4, 7)), "from the start, the next match");
    let mut ed = Ed::new("|foo bar foo");
    ed.keys("/bar<CR>5hgn");
    assert_eq!(ed.vim.selection(), Some((4, 7)), "inside a match, that match");
    let mut ed = Ed::new("|foo bar foo");
    ed.keys("/foo<CR>gg0gn");
    assert_eq!(ed.vim.selection(), Some((0, 3)), "at the start of one, that one");
    ed.keys("<Esc>$gN");
    assert_eq!(ed.mode(), Mode::Visual);
    assert_eq!(ed.vim.selection(), Some((0, 3)), "backward from inside the last match, the one before it");
}

#[test]
fn change_gn_then_dot_changes_each_match_in_turn() {
    table(&[
        ("|foo foo foo", "/foo<CR>cgnX<Esc>.", "foo X |X"),
        ("|foo foo foo", "/foo<CR>cgnX<Esc>..", "|X X X"),
        ("|a b a b a", "/a<CR>dgn", "a b | b a"),
    ]);
}

#[test]
fn a_search_in_a_selection_extends_it() {
    let mut ed = Ed::new("|a foo b");
    ed.keys("v/foo<CR>");
    assert_eq!(ed.mode(), Mode::Visual);
    assert_eq!(ed.vim.selection(), Some((0, 3)));
    ed.keys("d");
    assert_eq!(ed.text(), "oo b");
}

#[test]
fn the_last_search_is_the_slash_register_and_the_colon_register_is_the_last_command() {
    let mut ed = Ed::new("|a foo b");
    ed.keys("/foo<CR>");
    assert_eq!(ed.sh.regs.get(Some('/')).unwrap().text, "foo");
    ed.keys(":s/a/c/<CR>");
    assert_eq!(ed.sh.regs.get(Some(':')).unwrap().text, "s/a/c/");
}

#[test]
fn the_line_is_typed_edited_and_given_up() {
    let mut ed = Ed::new("|abc");
    ed.keys("/ab");
    assert_eq!(ed.mode(), Mode::Search);
    assert_eq!(ed.vim.prompt(), Some(('/', "ab", 2)));
    ed.keys("<Left>X");
    assert_eq!(ed.vim.prompt(), Some(('/', "aXb", 2)));
    ed.keys("<BS><Home><Del>");
    assert_eq!(ed.vim.prompt(), Some(('/', "b", 0)));
    ed.keys("<C-u>");
    assert_eq!(ed.vim.prompt(), Some(('/', "", 0)));
    ed.keys("x<Esc>");
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(ed.vim.prompt(), None);
    ed.keys("?");
    assert_eq!(ed.vim.prompt(), Some(('?', "", 0)));
    ed.keys("<BS>");
    assert_eq!(ed.mode(), Mode::Normal, "a backspace on an empty line gives it up");
}

#[test]
fn up_and_down_walk_the_lines_that_began_with_what_was_typed() {
    let mut ed = Ed::new("|a foo b bar c baz");
    ed.keys("/foo<CR>/bar<CR>/baz<CR>");
    ed.keys("/<Up>");
    assert_eq!(ed.vim.prompt().unwrap().1, "baz");
    ed.keys("<Up>");
    assert_eq!(ed.vim.prompt().unwrap().1, "bar");
    ed.keys("<Down>");
    assert_eq!(ed.vim.prompt().unwrap().1, "baz");
    ed.keys("<Down>");
    assert_eq!(ed.vim.prompt().unwrap().1, "", "past the newest it is what was typed");
    ed.keys("b<Up>");
    assert_eq!(ed.vim.prompt().unwrap().1, "baz", "only lines that begin with `b`");
    ed.keys("<Up>");
    assert_eq!(ed.vim.prompt().unwrap().1, "bar");
}

#[test]
fn an_empty_search_goes_on_with_the_last_pattern() {
    table(&[("|a foo b foo c foo", "/foo<CR>/<CR>", "a foo b |foo c foo"), ("|a foo b foo c foo", "/foo<CR>nn?<CR>", "a foo b |foo c foo")]);
}

#[test]
fn a_chord_the_line_does_not_take_is_the_hosts() {
    let mut ed = Ed::new("|abc");
    ed.keys("/");
    assert!(!ed.key(Key::ctrl('s')).handled);
}

#[test]
fn the_matches_are_marked_until_noh() {
    let mut ed = Ed::new("|a foo b");
    assert!(ed.vim.highlight(&ed.sh).is_none());
    ed.keys("/foo<CR>");
    assert!(ed.vim.highlight(&ed.sh).is_some());
    ed.keys(":noh<CR>");
    assert!(ed.vim.highlight(&ed.sh).is_none());
}

#[test]
fn search_is_one_for_every_editor() {
    let mut one = Ed::new("|a foo");
    one.keys("/foo<CR>");
    let mut two = Ed::with_shared("|x foo y foo", std::mem::take(&mut one.sh));
    two.keys("n");
    assert_eq!(two.shows(), "x |foo y foo");
}
