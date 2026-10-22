use super::*;

fn word(t: &str) -> Reg {
    Reg::new(t, false)
}

fn lines(t: &str) -> Reg {
    Reg::new(t, true)
}

// Source: EDITOR-VIM-12 and -17; Vim's register rules, which the library follows.

#[test]
fn a_yank_is_in_zero_and_the_unnamed_register() {
    let mut r = Registers::default();
    r.yank(None, word("ab"));
    assert_eq!(r.get(None), Some(word("ab")));
    assert_eq!(r.get(Some('0')), Some(word("ab")));
}

#[test]
fn an_unnamed_delete_of_a_line_shifts_the_numbered_registers() {
    let mut r = Registers::default();
    r.delete(None, lines("one"), false);
    r.delete(None, lines("two"), false);
    assert_eq!(r.get(Some('1')), Some(lines("two")));
    assert_eq!(r.get(Some('2')), Some(lines("one")));
    assert_eq!(r.get(None), Some(lines("two")));
}

#[test]
fn a_small_delete_goes_to_the_dash_register_and_leaves_the_numbered_ones() {
    let mut r = Registers::default();
    r.delete(None, lines("one"), false);
    r.delete(None, word("x"), false);
    assert_eq!(r.get(Some('-')), Some(word("x")));
    assert_eq!(r.get(Some('1')), Some(lines("one")));
    assert_eq!(r.get(None), Some(word("x")));
}

#[test]
fn a_far_motion_shifts_even_when_small() {
    let mut r = Registers::default();
    r.delete(None, word("(a)"), true);
    assert_eq!(r.get(Some('1')), Some(word("(a)")));
    assert_eq!(r.get(Some('-')), None);
}

#[test]
fn a_named_delete_stays_out_of_the_numbered_registers() {
    let mut r = Registers::default();
    r.delete(Some('a'), lines("one"), false);
    assert_eq!(r.get(Some('a')), Some(lines("one")));
    assert_eq!(r.get(Some('1')), None);
    assert_eq!(r.get(None), Some(lines("one")));
}

#[test]
fn a_capital_appends_and_lines_make_the_result_lines() {
    let mut r = Registers::default();
    r.yank(Some('a'), word("ab"));
    r.yank(Some('A'), word("cd"));
    assert_eq!(r.get(Some('a')), Some(word("abcd")));
    r.yank(Some('A'), lines("ef"));
    assert_eq!(r.get(Some('a')), Some(lines("abcd\nef")));
    assert_eq!(r.get(Some('A')), r.get(Some('a')), "reading a capital reads the register");
}

#[test]
fn the_black_hole_keeps_nothing_and_leaves_the_rest_alone() {
    let mut r = Registers::default();
    r.yank(None, word("keep"));
    r.delete(Some('_'), word("gone"), false);
    r.yank(Some('_'), word("gone"));
    assert_eq!(r.get(None), Some(word("keep")));
    assert_eq!(r.get(Some('_')), None);
}

#[test]
fn the_clipboard_is_written_out_on_a_plus_yank_and_read_as_the_host_offers_it() {
    let mut r = Registers::default();
    r.yank(Some('+'), word("out"));
    assert_eq!(r.take_clipboard_write(), Some("out".into()));
    assert_eq!(r.take_clipboard_write(), None);
    r.offer_clipboard(Some("in".into()));
    assert_eq!(r.get(Some('+')), Some(word("in")));
    assert_eq!(r.get(Some('*')), Some(word("in")));
}

#[test]
fn a_plain_yank_leaves_the_clipboard_alone() {
    // EDITOR-VIM-23: the unnamed register does not sync to it.
    let mut r = Registers::default();
    r.yank(None, word("a"));
    r.delete(None, word("b"), false);
    assert_eq!(r.take_clipboard_write(), None);
}

#[test]
fn the_last_typed_text_is_read_only() {
    let mut r = Registers::default();
    r.typed("hello".into());
    r.yank(Some('.'), word("no"));
    assert_eq!(r.get(Some('.')), Some(word("hello")));
}
