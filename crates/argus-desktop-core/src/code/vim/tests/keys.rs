//! What keys mean to the library (EDITOR-VIM-6, -20): the shifted and
//! chorded motions, `langmap`, Ctrl-c in a selection, and Escape.

use super::*;
use crate::code::vim::key::Code;

#[test]
fn shift_space_and_shift_backspace_are_w_and_b_and_the_ctrl_forms_the_big_ones() {
    let mut ed = Ed::new("|a-b c-d e");
    ed.key(Key::shifted(Code::Char(' ')));
    assert_eq!(ed.shows(), "a|-b c-d e");
    ed.key(Key::ctrl(' '));
    assert_eq!(ed.shows(), "a-b |c-d e");
    ed.key(Key::shifted(Code::Backspace));
    assert_eq!(ed.shows(), "a-|b c-d e");
}

#[test]
fn ctrl_backspace_is_a_big_word_back() {
    let mut ed = Ed::new("a-b c-d |e");
    ed.key(Key { ctrl: true, ..Key::of(Code::Backspace) });
    assert_eq!(ed.shows(), "a-b |c-d e");
}

#[test]
fn the_shifted_keys_are_the_notation_s_dash() {
    assert_eq!(crate::code::vim::key::parse("<S-Space><S-BS><C-Space><C-BS>"), vec![
        Key::shifted(Code::Char(' ')),
        Key::shifted(Code::Backspace),
        Key::ctrl(' '),
        Key { ctrl: true, ..Key::of(Code::Backspace) },
    ]);
}

#[test]
fn langmap_changes_what_a_typed_key_does_in_normal_and_visual_mode() {
    table(&[
        ("a\n|b", ":set lmap=jk<CR>j", "|a\nb"),
        ("a|b", ":set lmap=jk;hl<CR>j", "|ab"),
        ("|abc", ":set lmap=lh,hl<CR>l", "|abc"),
        ("a|bc", ":set lmap=lh,hl<CR>vh", "ab|c"),
    ]);
}

#[test]
fn langmap_leaves_the_character_after_f_and_r_alone_and_insert_mode_too() {
    table(&[
        ("|abc", ":set lmap=bx<CR>fb", "a|bc"),
        ("|abc", ":set lmap=bx<CR>rb", "|bbc"),
        ("|abc", ":set lmap=bx<CR>ib<Esc>", "|babc"),
    ]);
}

#[test]
fn langmap_is_for_every_editor() {
    let mut one = Ed::new("|a");
    one.keys(":set lmap=jk<CR>");
    let mut two = Ed::with_shared("a\n|b", std::mem::take(&mut one.sh));
    two.keys("j");
    assert_eq!(two.shows(), "|a\nb");
}

#[test]
fn ctrl_c_in_a_selection_copies_it_and_leaves_the_selection() {
    let mut ed = Ed::new("|abc");
    let step = ed.keys("vl<C-c>");
    assert_eq!(step.effects, vec![Effect::Copy("ab".into())]);
    assert_eq!(ed.mode(), Mode::Normal);
    let mut ed = Ed::new("|ab\ncd");
    let step = ed.keys("<C-v>jl<C-c>");
    assert_eq!(step.effects, vec![Effect::Copy("ab\ncd".into())]);
    assert_eq!(ed.mode(), Mode::Normal);
    let mut ed = Ed::new("|ab\ncd");
    assert_eq!(ed.keys("Vj<C-c>").effects, vec![Effect::Copy("ab\ncd".into())]);
    let mut ed = Ed::new("|ab\ncd\nef");
    assert_eq!(
        ed.keys("Vj<C-c>").effects,
        vec![Effect::Copy("ab\ncd".into())],
        "a line selection stops short of the last break"
    );
}

#[test]
fn ctrl_c_outside_a_selection_is_escape() {
    let mut ed = Ed::new("|abc");
    assert!(ed.keys("<C-c>").effects.is_empty());
    let step = ed.keys("ix<C-c>");
    assert!(step.effects.is_empty());
    assert_eq!(ed.mode(), Mode::Normal);
}

#[test]
fn escape_in_normal_mode_takes_the_marks_of_the_matches_away_and_in_a_selection_does_not() {
    let mut ed = Ed::new("|a foo b foo");
    ed.keys("/foo<CR>");
    assert!(ed.vim.highlight(&ed.sh).is_some());
    ed.keys("v<Esc>");
    assert!(ed.vim.highlight(&ed.sh).is_some(), "leaving a selection is not it");
    ed.keys("<Esc>");
    assert!(ed.vim.highlight(&ed.sh).is_none());
}

#[test]
fn ctrl_escape_is_escape() {
    let mut ed = Ed::new("|abc");
    ed.keys("ix");
    ed.key(Key { ctrl: true, ..Key::of(Code::Esc) });
    assert_eq!(ed.mode(), Mode::Normal);
}
