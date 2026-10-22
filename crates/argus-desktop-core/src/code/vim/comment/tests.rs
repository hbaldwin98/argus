use proptest::prelude::*;

use crate::code::vim::tests::Ed;

fn commented(text: &str, token: Option<&str>, keys: &str) -> String {
    let mut ed = Ed::new(text);
    ed.vim.set_comment_token(token);
    ed.keys(keys);
    ed.text()
}

#[test]
fn lines_are_commented_at_their_least_indent_with_the_token_and_a_blank() {
    assert_eq!(commented("a\nb", Some("//"), "gcj"), "// a\n// b");
    assert_eq!(commented("  a\n    b", Some("#"), "gcj"), "  # a\n  #   b");
    assert_eq!(commented("\ta", Some("--"), "gcc"), "\t-- a");
}

#[test]
fn lines_that_are_all_commented_are_uncommented_with_the_blank_after_the_token() {
    assert_eq!(commented("// a\n// b", Some("//"), "gcj"), "a\nb");
    assert_eq!(commented("//a\n//  b", Some("//"), "gcj"), "a\n b");
    assert_eq!(commented("  # a", Some("#"), "gcc"), "  a");
}

#[test]
fn a_mix_is_commented_whole() {
    assert_eq!(commented("// a\nb", Some("//"), "gcj"), "// // a\n// b");
}

#[test]
fn blank_lines_are_left_alone_unless_every_line_is_blank() {
    assert_eq!(commented("a\n\nb", Some("//"), "gcG"), "// a\n\n// b");
    assert_eq!(commented("\n ", Some("//"), "gcG"), "// \n//  ");
}

#[test]
fn with_no_token_nothing_changes() {
    assert_eq!(commented("a\nb", None, "gcj"), "a\nb");
}

#[test]
fn a_read_only_file_is_left() {
    let mut ed = Ed::new("a");
    ed.vim.set_comment_token(Some("//"));
    ed.vim.set_read_only(true);
    ed.keys("gcc");
    assert_eq!(ed.text(), "a");
}

#[test]
fn a_selection_is_commented_too() {
    assert_eq!(commented("a\nb\nc", Some("//"), "Vjgc"), "// a\n// b\nc");
}

proptest! {
    // Twice is once and back, for text that has no comment in it to begin with.
    #[test]
    fn toggling_twice_restores_the_lines(lines in proptest::collection::vec("[ ]{0,3}[a-z]{0,4}", 1..6)) {
        let text = lines.join("\n");
        let mut ed = Ed::new(&text);
        ed.vim.set_comment_token(Some("//"));
        ed.keys("gcGgcG");
        prop_assert_eq!(ed.text(), text);
    }
}
