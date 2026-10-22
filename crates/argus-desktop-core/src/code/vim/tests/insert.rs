//! Insert and replace mode (EDITOR-VIM-5, -6).

use super::*;
use crate::code::vim::key::Code;

#[test]
fn the_ways_into_insert_mode() {
    table(&[
        ("a|c", "ibX<Esc>", "ab|Xc"),
        ("a|c", "aX<Esc>", "ac|X"),
        ("  a|b", "IX<Esc>", "  |Xab"),
        ("a|b", "AX<Esc>", "ab|X"),
        ("  a|b", "gIX<Esc>", "|X  ab"),
        ("|ab", "oX<Esc>", "ab\n|X"),
        ("a|b", "OX<Esc>", "|X\nab"),
        ("  a|b", "oX<Esc>", "  ab\n  |X"),
        ("|a", "o<Esc>", "a\n|"),
        ("|a", "<Ins>X<Esc>", "|Xa"),
    ]);
}

#[test]
fn a_count_types_the_text_again() {
    table(&[
        ("|a", "3ix<Esc>", "xx|xa"),
        ("|a", "3ax<Esc>", "axx|x"),
        ("|ab", "2ox<Esc>", "ab\nx\n|x"),
        ("|a", "3i<Esc>", "|a"),
    ]);
}

#[test]
fn escape_is_three_keys_and_leaves_the_cursor_on_the_last_character() {
    table(&[
        ("|a", "ix<C-c>", "|xa"),
        ("|a", "ix<C-[>", "|xa"),
        ("|a", "ixy<Esc>", "x|ya"),
        ("|a", "ix<Esc><Esc>", "|xa"),
    ]);
}

#[test]
fn replace_mode_types_over_the_text() {
    table(&[
        ("|abc", "Rxy<Esc>", "x|yc"),
        ("|ab", "Rxyz<Esc>", "xy|z"),
        ("|ab\ncd", "Rxyz<Esc>", "xy|z\ncd"),
        ("|abc", "Rx<BS>y<Esc>", "|ybc"),
    ]);
}

#[test]
fn editing_what_was_typed() {
    table(&[
        ("|ab", "Aab<BS><BS>c<Esc>", "ab|c"),
        ("|abc", "i<Del><Esc>", "|bc"),
        ("|abc", "i<Right><Right>X<Esc>", "ab|Xc"),
        ("|abc", "A<Left>X<Esc>", "ab|Xc"),
        ("a|c", "i<Home>X<Esc>", "|Xac"),
        ("a|c", "i<End>X<Esc>", "ac|X"),
        ("|ab\ncd", "i<Down>X<Esc>", "ab\n|Xcd"),
        ("ab\n|cd", "i<Up>X<Esc>", "|Xab\ncd"),
        ("|", "Afoo bar<C-w>x<Esc>", "foo |x"),
        ("|", "Afoo<C-u>x<Esc>", "|x"),
        ("a\n|b", "I<BS><Esc>", "|ab"),
        ("|", "iéx<Esc>", "é|x"),
    ]);
}

#[test]
fn enter_keeps_the_indent() {
    table(&[("  |a", "A<CR>x<Esc>", "  a\n  |x"), ("|a", "A<CR>x<Esc>", "a\n|x")]);
}

#[test]
fn the_line_can_be_indented_and_outdented_while_typing() {
    table(&[("|a", "i<C-t><Esc>", "   | a"), ("    |a", "i<C-d><Esc>", "|a")]);
}

#[test]
fn a_register_can_be_inserted() {
    table(&[("|ab", "yl$a<C-r>\"<Esc>", "ab|a")]);
}

#[test]
fn what_was_typed_is_the_dot_register() {
    table(&[("|a", "ihello<Esc>\".p", "hellohell|oa")]);
}

#[test]
fn an_insert_is_one_undo() {
    table(&[("|a", "ixyz<Esc>u", "|a"), ("|a", "ixyz<Esc>ibq<Esc>uu", "|a")]);
}

#[test]
fn keys_vim_does_not_use_go_back_to_the_host() {
    let mut ed = Ed::new("|a");
    ed.keys("i");
    assert!(!ed.key(Key::of(Code::Tab)).handled);
    assert!(!ed.key(Key::ctrl('s')).handled);
    assert!(!ed.key(Key::of(Code::PageDown)).handled);
    assert!(ed.keys("x").handled);
}
