use proptest::prelude::*;

use super::indents;
use crate::code::vim::tests::Ed;

fn reindented(text: &str, keys: &str) -> String {
    let mut ed = Ed::new(text);
    ed.vim.set_indent_unit(Some(4));
    ed.vim.set_comment_token(Some("//"));
    ed.keys(keys);
    ed.text()
}

/// EDITOR-VIM-24: with a brace language loaded, `=` indents a line by the brackets open above it.
#[test]
fn lines_are_indented_a_unit_for_each_open_bracket() {
    assert_eq!(reindented("|fn a() {\nlet x = 1;\nif x {\ny();\n}\n}", "=G"), "fn a() {\n    let x = 1;\n    if x {\n        y();\n    }\n}");
}

#[test]
fn a_line_that_begins_with_closers_goes_back_by_each() {
    assert_eq!(reindented("|f(a, [\n1,\n])\nb", "=G"), "f(a, [\n        1,\n])\nb");
    assert_eq!(reindented("|f(a, [\n1,\n  ])\nb", "=G"), "f(a, [\n        1,\n])\nb");
}

#[test]
fn brackets_in_strings_and_line_comments_are_not_counted() {
    assert_eq!(reindented("|a(\"{\");\n// {\n'(';\nb;", "=G"), "a(\"{\");\n// {\n'(';\nb;");
}

/// EDITOR-VIM-24: a Rust lifetime is not a quote, so the brackets after it still count.
#[test]
fn lifetimes_and_generics_do_not_hide_brackets() {
    assert_eq!(indents(&["fn f<'a>(x: &'a str) {", "y();", "}"], Some("//"), 4), [Some(0), Some(4), Some(0)]);
    assert_eq!(indents(&["impl<'a, 'b> T for U<'a, 'b> {", "z('\\'');", "w('\\n', '(');", "}"], Some("//"), 4), [Some(0), Some(4), Some(4), Some(0)]);
}

#[test]
fn blank_lines_are_left_and_lines_already_right_are_not_changed() {
    assert_eq!(reindented("|{\n\n  x\n}", "=G"), "{\n\n    x\n}");
    let mut ed = Ed::new("|{\n    x\n}");
    ed.vim.set_indent_unit(Some(4));
    let step = ed.keys("=G");
    assert!(step.change.is_none(), "nothing to change");
}

#[test]
fn only_the_lines_asked_for_are_indented_and_the_cursor_lands_on_the_first() {
    let mut ed = Ed::new("{\n|x\n  y\n}");
    ed.vim.set_indent_unit(Some(4));
    ed.keys("==");
    assert_eq!(ed.shows(), "{\n    |x\n  y\n}");
}

#[test]
fn a_read_only_file_is_not_indented() {
    let mut ed = Ed::new("|{\nx\n}");
    ed.vim.set_indent_unit(Some(4));
    ed.vim.set_read_only(true);
    ed.keys("=G");
    assert_eq!(ed.text(), "{\nx\n}");
}

#[test]
fn without_an_indent_unit_equals_changes_nothing() {
    let mut ed = Ed::new("|{\nx\n}");
    ed.keys("=G");
    assert_eq!(ed.text(), "{\nx\n}");
}

mod properties {
    use super::*;

    proptest! {
        /// A line's indent is a whole number of units, and re-indenting what is indented is no change.
        #[test]
        fn indenting_is_whole_units_and_settles_in_one_pass(text in "[a-z{}()\\[\\] \n]{0,60}") {
            let lines: Vec<&str> = text.split('\n').collect();
            let once = indents(&lines, None, 4);
            prop_assert!(once.iter().flatten().all(|w| w % 4 == 0));
            let settled: Vec<String> = lines.iter().zip(&once).map(|(l, w)| match w { Some(w) => format!("{}{}", " ".repeat(*w), l.trim_start()), None => (*l).to_owned() }).collect();
            let again: Vec<&str> = settled.iter().map(String::as_str).collect();
            prop_assert_eq!(indents(&again, None, 4), once);
        }
    }
}
