use super::*;

fn rope(s: &str) -> Rope {
    Rope::from_str(s)
}

#[test]
fn lines_split_on_line_feeds_and_a_trailing_break_leaves_an_empty_last_line() {
    let r = rope("ab\ncd\n");
    let t = Text(&r);
    assert_eq!(t.line_count(), 3);
    assert_eq!((t.line_start(1), t.line_end(1)), (3, 5));
    assert_eq!((t.line_start(2), t.line_end(2)), (6, 6));
    assert_eq!(t.line_span(0), (0, 3));
    assert_eq!(t.line_span(2), (6, 6));
    assert_eq!(t.last_line(), 2);
    assert!(t.is_blank_line(2));
}

#[test]
fn an_empty_text_is_one_empty_line() {
    let r = rope("");
    let t = Text(&r);
    assert_eq!((t.line_count(), t.line_start(0), t.line_end(0), t.last_char(0)), (1, 0, 0, 0));
}

#[test]
fn next_and_prev_step_whole_characters() {
    let r = rope("é😀a");
    let t = Text(&r);
    assert_eq!(t.next(0), 2);
    assert_eq!(t.next(2), 6);
    assert_eq!(t.next(7), 7);
    assert_eq!(t.prev(6), 2);
    assert_eq!(t.prev(2), 0);
    assert_eq!(t.prev(0), 0);
}

#[test]
fn columns_count_characters_and_stop_on_the_last_one() {
    let r = rope("é😀a\nxy");
    let t = Text(&r);
    assert_eq!(t.col(6), 2);
    assert_eq!(t.at_col(0, 1), 2);
    assert_eq!(t.at_col(0, 9), 6, "a short line gives its last character");
    assert_eq!(t.at_col(1, 1), t.line_start(1) + 1);
}

#[test]
fn the_first_nonblank_skips_spaces_and_tabs() {
    let r = rope("  \t x\n   ");
    let t = Text(&r);
    assert_eq!(t.first_nonblank(0), 4);
    assert_eq!(t.first_nonblank(1), t.line_end(1));
}

#[test]
fn classes_split_words_from_punctuation_unless_big() {
    assert_eq!(Text::class('a', false), Class::Word);
    assert_eq!(Text::class('_', false), Class::Word);
    assert_eq!(Text::class('-', false), Class::Punct);
    assert_eq!(Text::class('-', true), Class::Word);
    assert_eq!(Text::class('\n', true), Class::Space);
}
