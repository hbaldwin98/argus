use super::*;
use crate::code::vim::key::Key;
use crate::code::vim::tests::Ed;

#[test]
fn the_prefix_says_which_line_it_is() {
    let line = |kind| Prompt { kind, text: String::new(), caret: 0, then: Then::Run };
    assert_eq!(line(Kind::Ex).prefix(), ':');
    assert_eq!(line(Kind::Search { forward: true }).prefix(), '/');
    assert_eq!(line(Kind::Search { forward: false }).prefix(), '?');
}

#[test]
fn typing_puts_characters_at_the_caret_and_a_wide_one_is_one_step() {
    let mut ed = Ed::new("|abc");
    ed.keys(":é😀x");
    assert_eq!(ed.vim.prompt(), Some((':', "é😀x", 3)));
    ed.keys("<Left><Left>Z");
    assert_eq!(ed.vim.prompt(), Some((':', "éZ😀x", 2)));
    ed.keys("<BS>");
    assert_eq!(ed.vim.prompt(), Some((':', "é😀x", 1)));
    ed.keys("<Del>");
    assert_eq!(ed.vim.prompt(), Some((':', "éx", 1)));
    ed.keys("<Home><Right><Right><Right><Right>");
    assert_eq!(ed.vim.prompt(), Some((':', "éx", 2)), "the caret stops at the end");
    ed.keys("<End><Left>");
    assert_eq!(ed.vim.prompt(), Some((':', "éx", 1)));
}

#[test]
fn backspace_at_the_start_with_text_after_it_does_nothing_and_on_an_empty_line_gives_it_up() {
    let mut ed = Ed::new("|abc");
    ed.keys(":xy<Home><BS>");
    assert_eq!(ed.vim.prompt(), Some((':', "xy", 0)));
    ed.keys("<Del><Del>");
    assert_eq!(ed.vim.prompt(), Some((':', "", 0)));
    ed.keys("<Del><BS>");
    assert_eq!(ed.mode(), Mode::Normal);
}

#[test]
fn control_u_clears_the_line_and_other_chords_are_the_hosts() {
    let mut ed = Ed::new("|abc");
    ed.keys(":xy");
    assert!(ed.key(Key::ctrl('u')).handled);
    assert_eq!(ed.vim.prompt(), Some((':', "", 0)));
    assert!(!ed.key(Key::ctrl('s')).handled);
    assert!(!ed.key(Key::of(Code::Tab)).handled);
}

#[test]
fn escape_and_the_chords_that_are_escape_give_the_line_up_and_remember_it() {
    for esc in ["<Esc>", "<C-[>", "<C-c>"] {
        let mut ed = Ed::new("|abc");
        ed.keys(&format!(":zz{esc}"));
        assert_eq!(ed.mode(), Mode::Normal, "{esc}");
        ed.keys(":<Up>");
        assert_eq!(ed.vim.prompt().unwrap().1, "zz", "it was kept for Up: {esc}");
    }
}

#[test]
fn a_search_line_given_up_in_a_selection_leaves_the_selection() {
    let mut ed = Ed::new("|abc");
    ed.keys("vl/x<Esc>");
    assert_eq!(ed.mode(), Mode::Visual);
    assert_eq!(ed.vim.selection(), Some((0, 2)));
}

#[test]
fn clicking_while_a_line_is_open_closes_it() {
    let mut ed = Ed::new("|abc");
    ed.keys(":xy");
    ed.vim.place(2);
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(ed.vim.prompt(), None);
    ed.keys("v/x");
    ed.vim.place(1);
    assert_eq!(ed.mode(), Mode::Visual, "a search line returns to the selection it came from");
}
