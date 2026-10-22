//! The `:` line (EDITOR-VIM-2, -4, -13).

use super::*;

#[test]
fn write_asks_the_host_to_write() {
    for line in [":w<CR>", ":write<CR>"] {
        let mut ed = Ed::new("|abc");
        ed.keys(line);
        assert_eq!(ed.effects, vec![Effect::Write], "{line}");
        assert_eq!(ed.mode(), Mode::Normal);
    }
}

#[test]
fn a_line_number_goes_to_the_line() {
    table(&[
        ("|a\n  b\nc", ":2<CR>", "a\n  |b\nc"),
        ("|a\n  b\nc", ":99<CR>", "a\n  b\n|c"),
        ("|a\n  b\nc", ":$<CR>", "a\n  b\n|c"),
        ("a\nb|\nc", ":1<CR>", "|a\nb\nc"),
    ]);
}

#[test]
fn undo_and_redo() {
    table(&[("|abc", "x:u<CR>", "|abc"), ("|abc", "xu:red<CR>", "|bc")]);
}

#[test]
fn the_line_is_typed_and_can_be_left() {
    let mut ed = Ed::new("|abc");
    ed.keys(":wr");
    assert_eq!((ed.mode(), ed.vim.command_line()), (Mode::Command, Some("wr")));
    ed.keys("<BS><BS><BS>");
    assert_eq!(ed.mode(), Mode::Normal, "a backspace on an empty line leaves it");
    ed.keys(":w<Esc>");
    assert!(ed.effects.is_empty());
    assert_eq!(ed.vim.command_line(), None);
}

#[test]
fn only_the_editors_own_commands_exist() {
    // EDITOR-VIM-4: no :q, :wq or :x.
    for line in ["q", "wq", "x", "e other"] {
        let mut ed = Ed::new("|abc");
        ed.keys(&format!(":{line}<CR>"));
        assert!(ed.effects.is_empty(), "{line}");
        assert_eq!(ed.vim.message(), Some(format!("Not an editor command \":{line}\"").as_str()));
        assert_eq!(ed.text(), "abc");
    }
}

#[test]
fn write_takes_no_notice_of_what_follows_it() {
    // The page's own `write` callback ignores its arguments, and the library
    // does not check them, so `:w file` writes this file.
    let mut ed = Ed::new("|abc");
    ed.keys(":w file<CR>");
    assert_eq!(ed.effects, vec![Effect::Write]);
}

#[test]
fn an_option_the_library_does_not_have_is_an_error() {
    // EDITOR-VIM-14: no ignorecase, hlsearch, tabstop, number.
    for name in ["number", "ignorecase", "tabstop=2", "hlsearch"] {
        let mut ed = Ed::new("|abc");
        ed.keys(&format!(":set {name}<CR>"));
        let want = format!("Unknown option: {}", name.split('=').next().unwrap());
        assert_eq!(ed.vim.message(), Some(want.as_str()), "{name}");
    }
}

#[test]
fn start_insert_and_the_no_ops() {
    let mut ed = Ed::new("|abc");
    ed.keys(":noh<CR>");
    assert_eq!(ed.vim.message(), None);
    ed.keys(":startinsert<CR>");
    assert_eq!(ed.mode(), Mode::Insert);
}

#[test]
fn a_chord_goes_to_the_host_from_the_line() {
    let mut ed = Ed::new("|abc");
    ed.keys(":");
    assert!(!ed.key(Key::ctrl('s')).handled);
}
