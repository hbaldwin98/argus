use proptest::prelude::*;
use ropey::Rope;

use super::*;

const DOC: &str = "a\nb\nc\nd\ne";

fn read_at(doc: &str, cursor_line: usize, marks: &Marks, input: &str) -> Result<Parsed, String> {
    let rope = Rope::from_str(doc);
    let reader = Reader { t: Text(&rope), cursor_line, visual: false, marks, last_pattern: Some("c"), };
    reader.parse(input)
}

fn read(input: &str) -> Parsed {
    read_at(DOC, 1, &Marks::default(), input).unwrap()
}

fn rope_lines(p: &Parsed) -> (usize, usize) {
    let rope = Rope::from_str(DOC);
    p.lines(Text(&rope))
}

#[test]
fn a_line_with_no_range_is_the_cursors_line_and_the_command() {
    let p = read("d");
    assert_eq!((p.line, p.line_end, p.name.as_str(), p.arg.as_deref()), (None, None, "d", None));
    assert_eq!(rope_lines(&p), (1, 1));
}

#[test]
fn the_command_name_is_a_word_or_one_of_the_symbols_and_the_rest_is_its_argument() {
    assert_eq!(read("s/a/b/g").name, "s");
    assert_eq!(read("s/a/b/g").arg.as_deref(), Some("/a/b/g"));
    assert_eq!(read("  sort  n").name, "sort");
    assert_eq!(read("sort  n").arg.as_deref(), Some("  n"));
    assert_eq!(read("g!/x/d").arg.as_deref(), Some("!/x/d"));
    assert_eq!(read(">>").name, ">");
    assert_eq!(read("!!").name, "!!");
    assert_eq!(read("@@").name, "@@");
    assert_eq!(read("w2").name, "w2");
}

#[test]
fn leading_colons_are_skipped_but_a_blank_before_a_range_makes_it_a_name() {
    let p = read("::3d");
    assert_eq!((p.line, p.name.as_str()), (Some(2), "d"));
    let p = read(":  3d");
    assert_eq!((p.line, p.name.as_str()), (None, "3d"), "as the library reads it: digits are word characters");
}

#[test]
fn a_number_a_dot_and_a_dollar_name_lines() {
    assert_eq!(read("3d").line, Some(2));
    assert_eq!(read(".d").line, Some(1));
    assert_eq!(read("$d").line, Some(4));
    assert_eq!(read("99d").line, Some(4), "past the text is its last line");
}

#[test]
fn two_addresses_make_a_range_in_either_order() {
    let p = read("2,4d");
    assert_eq!((p.line, p.line_end), (Some(1), Some(3)));
    assert_eq!(rope_lines(&p), (1, 3));
    assert_eq!(rope_lines(&read("4,2d")), (1, 3));
    assert_eq!(rope_lines(&read(".,$d")), (1, 4));
    assert_eq!(rope_lines(&read("1,d")), (0, 0), "a second address that is not there");
}

#[test]
fn percent_is_every_line() {
    let p = read("%d");
    assert_eq!((p.line, p.line_end), (Some(0), Some(4)));
}

#[test]
fn offsets_follow_an_address() {
    assert_eq!(read(".+1d").line, Some(2));
    assert_eq!(read(".-1d").line, Some(0));
    assert_eq!(read("$-1d").line, Some(3));
    assert_eq!(read("2+2d").line, Some(3));
    assert_eq!(read(".+d").line, Some(2), "a sign alone is one");
    assert_eq!(read(".++d").line, Some(3));
    assert_eq!(read("+2d").line, Some(3), "from the cursor's line");
    assert_eq!(read("-1d").line, Some(0));
    assert_eq!(read("-9d").line, Some(0), "not before the first");
    assert_eq!(read(".+1,$-1d").line_end, Some(3));
}

#[test]
fn a_mark_is_a_line_and_one_not_set_is_an_error() {
    let mut marks = Marks::default();
    marks.set('a', 4);
    let p = read_at(DOC, 0, &marks, "'ad").unwrap();
    assert_eq!(p.line, Some(2), "the mark at byte 4 is on the third line");
    assert_eq!(read_at(DOC, 0, &marks, "'zd"), Err("Mark not set".into()));
    assert_eq!(read_at(DOC, 0, &marks, "'"), Err("Mark not set".into()));
}

#[test]
fn a_pattern_is_the_next_or_last_line_with_it() {
    assert_eq!(read("/c/d").line, Some(2));
    assert_eq!(read("/e/d").line, Some(4));
    assert_eq!(read_at(DOC, 4, &Marks::default(), "?b?d").unwrap().line, Some(1));
    assert_eq!(read("/c/+1d").line, Some(3), "with an offset after it");
    assert_eq!(read_at(DOC, 0, &Marks::default(), "/b/,/d/d").unwrap().line_end, Some(3), "the second goes from the cursor's line too");
}

#[test]
fn a_pattern_search_goes_from_the_line_after_the_cursors_or_before_it() {
    assert_eq!(read_at(DOC, 2, &Marks::default(), "/c/d"), Err("Pattern not found/c/".into()), "past this line none follows");
    assert_eq!(read_at(DOC, 2, &Marks::default(), "?c?d"), Err("Pattern not found/c/".into()), "before this line none is");
}

#[test]
fn an_empty_pattern_or_a_backslash_slash_is_the_last_one_searched() {
    assert_eq!(read("//d").line, Some(2));
    // A backslash form is an offset, so it follows an address; on its own it is no address.
    assert_eq!(read(".\\/d").line, Some(2));
    assert_eq!(read("\\/d").line, None);
    assert_eq!(read_at(DOC, 4, &Marks::default(), ".\\?d").unwrap().line, Some(2));
}

#[test]
fn a_pattern_not_found_is_an_error_naming_it() {
    assert_eq!(read_at(DOC, 0, &Marks::default(), "/zzz/d"), Err("Pattern not found/zzz/".into()));
}

#[test]
fn the_last_selection_is_the_range_after_a_star() {
    let mut marks = Marks::default();
    marks.set_hidden(ANCHOR, 6);
    marks.set_hidden(HEAD, 2);
    let p = read_at(DOC, 0, &marks, "*d").unwrap();
    assert_eq!((p.line, p.line_end), (Some(3), Some(1)));
    assert_eq!(rope_lines(&p), (1, 3));
}

#[test]
fn a_selection_in_visual_mode_is_the_range_with_none_typed() {
    let mut marks = Marks::default();
    marks.set('<', 2);
    marks.set('>', 6);
    let rope = Rope::from_str(DOC);
    let reader = Reader { t: Text(&rope), cursor_line: 0, visual: true, marks: &marks, last_pattern: None };
    let p = reader.parse("d").unwrap();
    assert_eq!(p.lines(Text(&rope)), (1, 3));
}

proptest! {
    // Whatever is typed, reading it ends in a range inside the text or an error.
    #[test]
    fn a_range_read_is_in_the_text(input in "[0-9.$%*',+/?a-z \\\\-]{0,12}", cursor in 0usize..5) {
        if let Ok(p) = read_at(DOC, cursor, &Marks::default(), &input) {
            let rope = Rope::from_str(DOC);
            let (a, b) = p.lines(Text(&rope));
            prop_assert!(a <= b && b <= 4);
            prop_assert!(p.line.is_none_or(|l| l <= 4) && p.line_end.is_none_or(|l| l <= 4));
        }
    }
}
