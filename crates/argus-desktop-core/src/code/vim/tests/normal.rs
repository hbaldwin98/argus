//! Normal mode: motions, operators, and the commands of their own. The
//! expectations are Vim's (EDITOR-VIM-7 to -9), not the library's source.

use super::*;

#[test]
fn the_editor_opens_in_normal_mode() {
    // EDITOR-VIM-1
    assert_eq!(Ed::new("abc").mode(), Mode::Normal);
}

#[test]
fn characters_and_lines() {
    table(&[
        ("|abc", "l", "a|bc"),
        ("a|bc", "h", "|abc"),
        ("|abc", "5l", "ab|c"),
        ("ab|c", "l", "ab|c"),
        ("|abc", "h", "|abc"),
        ("|a\nb\nc", "jj", "a\nb\n|c"),
        ("|a\nb\nc", "3j", "a\nb\n|c"),
        ("a\nb\n|c", "k", "a\n|b\nc"),
        ("a\nb\n|c", "gg", "|a\nb\nc"),
        ("|a\nb\nc", "G", "a\nb\n|c"),
        ("|a\nb\nc", "2G", "a\n|b\nc"),
        ("|a\nb\nc", "2gg", "a\n|b\nc"),
        ("|abc", " ", "a|bc"),
        ("a|bc", "<BS>", "|abc"),
        ("|abc", "<Right>", "a|bc"),
        ("|a\nb", "<Down>", "a\n|b"),
    ]);
}

#[test]
fn j_and_k_keep_the_column_they_aim_for() {
    table(&[
        ("abc|def\nx\nabcdef", "jj", "abcdef\nx\nabc|def"),
        ("abcde|f\nxy\nabcdef", "jj", "abcdef\nxy\nabcde|f"),
        ("abc|\nxy", "$j", "abc\nx|y"),
    ]);
}

#[test]
fn line_positions() {
    table(&[
        ("ab|c", "0", "|abc"),
        ("  ab|c", "^", "  |abc"),
        ("|abc", "$", "ab|c"),
        ("|a\nbcd\nef", "2$", "a\nbc|d\nef"),
        ("|abcdef", "4|", "abc|def"),
        ("   |x", "0", "|   x"),
    ]);
}

#[test]
fn words() {
    table(&[
        ("|abc def", "w", "abc |def"),
        ("|foo.bar baz", "w", "foo|.bar baz"),
        ("|foo.bar baz", "W", "foo.bar |baz"),
        ("|one two three", "2w", "one two |three"),
        ("|abc def", "e", "ab|c def"),
        ("ab|c def", "e", "abc de|f"),
        ("abc |def", "b", "|abc def"),
        ("abc d|ef", "b", "abc |def"),
        ("abc de|f", "ge", "ab|c def"),
        ("|a\n\nb", "w", "a\n|\nb"),
        ("a\n|\nb", "w", "a\n\n|b"),
        ("|ab cd", "$b", "ab |cd"),
        ("|foo-bar", "E", "foo-ba|r"),
    ]);
}

#[test]
fn finding_a_character_on_the_line() {
    table(&[
        ("|a b a b", "fb", "a |b a b"),
        ("|a b a b", "2fb", "a b a |b"),
        ("|a b a b", "tb", "a| b a b"),
        ("a b a |b", "Fa", "a b |a b"),
        ("a b a |b", "Ta", "a b a| b"),
        ("|a b a b", "fb;", "a b a |b"),
        ("|a b a b", "fb;,", "a |b a b"),
        ("|a b a b", "tb;", "a b a| b"),
        ("|abc", "fz", "|abc"),
    ]);
}

#[test]
fn paragraphs_and_matching_symbols() {
    table(&[
        ("|a\nb\n\nc", "}", "a\nb\n|\nc"),
        ("a\nb\n\n|c", "{", "a\nb\n|\nc"),
        ("|(a (b) c)", "%", "(a (b) c|)"),
        ("(a (b) c|)", "%", "|(a (b) c)"),
        ("|x [a] y", "%", "x [a|] y"),
        ("|abc", "%", "|abc"),
    ]);
}

#[test]
fn deleting_with_motions() {
    table(&[
        ("|abc def", "dw", "|def"),
        ("abc |def", "dw", "abc| "),
        ("|a b c d", "2dw", "|c d"),
        ("|a b c d", "d2w", "|c d"),
        ("a|bc def", "d0", "|bc def"),
        ("a|bc def", "d$", "|a"),
        ("abc |def", "db", "|def"),
        ("|abc def", "de", "| def"),
        ("|abc def", "dfd", "|ef"),
        ("|abc def", "dtd", "|def"),
        ("a|bcd", "dh", "|bcd"),
        ("a|bcd", "dl", "a|cd"),
        ("one\ntwo|\nthree", "dk", "|three"),
        ("|one\ntwo\nthree", "dj", "|three"),
        ("|one\ntwo\nthree", "dG", "|"),
        ("one\ntw|o\nthree", "dgg", "|three"),
        ("|(a b)", "d%", "|"),
        ("|a\n\nb", "d}", "|\nb"),
    ]);
}

#[test]
fn the_shortcuts() {
    table(&[
        ("a|bc", "x", "a|c"),
        ("ab|c", "x", "a|b"),
        ("|abc", "2x", "|c"),
        ("a|bc", "X", "|bc"),
        ("a|bc", "D", "|a"),
        ("a|bc def", "3x", "a|def"),
        ("a|bc", "<Del>", "a|c"),
        ("|", "x", "|"),
    ]);
}

#[test]
fn lines() {
    table(&[
        ("|abc\ndef", "dd", "|def"),
        ("abc\n|def", "dd", "|abc"),
        ("|abc\ndef\nghi", "2dd", "|ghi"),
        ("a\n|b\nc", "dd", "a\n|c"),
        ("|a\nb\nc", "5dd", "|"),
        ("  |a\n  b", "dd", "  |b"),
        ("|abc", "dd", "|"),
        ("ab|c\ndef", "yyp", "abc\n|abc\ndef"),
        ("abc\n|def", "yyP", "abc\n|def\ndef"),
        ("|abc\ndef", "ddp", "def\n|abc"),
        ("a|b", "yy2p", "ab\n|ab\nab"),
        ("|a\nb", "Yjp", "a\nb\n|a"),
        ("|abc", "S", "|"),
    ]);
}

#[test]
fn indent_and_case() {
    table(&[
        ("|abc", ">>", "    |abc"),
        ("    |abc", "<<", "|abc"),
        ("  |abc", "<<", "|abc"),
        ("|a\nb", "2>>", "    |a\n    b"),
        ("|a\n\nb", ">2j", "    |a\n\n    b"),
        ("|abc", "~", "A|bc"),
        ("|abc", "3~", "AB|C"),
        ("|aBc", "g~~", "|AbC"),
        ("|abc def", "gUw", "|ABC def"),
        ("|ABC", "guu", "|abc"),
        ("a|bc", "gUU", "A|BC"),
        ("|abc def", "gUiw", "|ABC def"),
    ]);
}

#[test]
fn replacing_and_joining() {
    table(&[
        ("|abc", "rx", "|xbc"),
        ("|abc", "3rx", "xx|x"),
        ("|abc", "4rx", "|abc"),
        ("a|bc", "rÿ", "a|ÿc"),
        ("|a\n  b", "J", "a| b"),
        ("|a\nb\nc", "3J", "a b| c"),
        ("|a\nb", "gJ", "a|b"),
        ("|a\n)", "J", "a|)"),
        ("|a\n", "J", "|a"),
        ("|", "J", "|"),
    ]);
}

#[test]
fn undo_and_redo_take_back_a_command_whole() {
    table(&[
        ("|abc", "xu", "|abc"),
        ("|abc", "xxu", "|bc"),
        ("|abc", "xxuu", "|abc"),
        ("|abc", "xxuu<C-r>", "|bc"),
        ("|abc", "xxuu<C-r><C-r>", "|c"),
        ("|abc", "3xu", "|abc"),
        ("|abc def", "dwu", "|abc def"),
        ("|a\nb", "ddu", "|a\nb"),
        ("|a b", "ihi <Esc>u", "|a b"),
    ]);
}

#[test]
fn counted_undo() {
    table(&[("|abcd", "xxx2u", "|bcd")]);
}

#[test]
fn repeating_the_last_change_with_a_dot() {
    table(&[
        ("|a b c d", "dw.", "|c d"),
        ("|a b c d", "dw..", "|d"),
        ("|abcdef", "x.", "|cdef"),
        ("|abcdef", "x2.", "|def"),
        ("|abcdef", "2x.", "|ef"),
        ("|aaa bbb", "ciwX<Esc>w.", "X |X"),
        ("|a\nb\nc", "dd.", "|c"),
        ("|a b", "Ax<Esc>.", "a bx|x"),
        ("|ab ab", "rxl.", "x|x ab"),
        ("|abc", "3ix<Esc>.", "xxxx|xxabc"),
        ("|a b", "yw.", "|a b"),
    ]);
}

#[test]
fn a_count_on_the_dot_replaces_the_original() {
    table(&[("|abcdefgh", "2x3.", "|fgh")]);
}

#[test]
fn a_dot_after_an_insert_types_it_again() {
    table(&[("|a", "ox<Esc>.", "a\nx\n|x")]);
}

#[test]
fn keys_that_begin_nothing_are_swallowed_and_chords_are_the_hosts() {
    let mut ed = Ed::new("|abc");
    assert!(ed.keys("Z").handled, "an unbound letter is taken");
    assert_eq!(ed.text(), "abc");
    assert!(!ed.key(Key::ctrl('s')).handled, "Ctrl-s is the host's");
    assert!(!ed.key(Key::of(super::super::key::Code::Tab)).handled);
    assert!(ed.keys("d").handled, "a command half typed is taken");
    assert_eq!(ed.vim.pending().len(), 1);
    ed.keys("<Esc>");
    assert!(ed.vim.pending().is_empty());
}

#[test]
fn a_bad_second_key_drops_the_command() {
    table(&[("|abc def", "dZx", "|bc def"), ("|abc def", "d<Esc>x", "|bc def")]);
}

#[test]
fn next_and_previous_change_are_the_hosts_to_do() {
    let mut ed = Ed::new("|abc");
    ed.keys("]c[c");
    assert_eq!(ed.effects, vec![Effect::NextChange, Effect::PreviousChange]);
}

#[test]
fn a_read_only_file_is_not_edited_or_entered() {
    // EDITOR-VIM-18
    let mut ed = Ed::new("|abc");
    ed.vim.set_read_only(true);
    ed.keys("xdwiZ<Esc>ox<Esc>");
    assert_eq!(ed.text(), "abc");
    assert_eq!(ed.mode(), Mode::Normal);
    ed.keys("l");
    assert_eq!(ed.shows(), "a|bc", "it can still be walked");
}

#[test]
fn the_cursor_can_be_placed_and_a_change_from_outside_moves_it() {
    let mut ed = Ed::new("abcdef");
    ed.vim.place(3);
    assert_eq!(ed.shows(), "abc|def");
    let remote = crate::code::change::ChangeSet::of(
        &[crate::code::change::Change { from: 0, to: 0, insert: "XX".into() }],
        6,
    )
    .unwrap();
    ed.vim.past(&remote);
    assert_eq!(ed.vim.doc().to_string(), "XXabcdef");
    assert_eq!(ed.vim.cursor(), 5);
    ed.host = Rope::from_str("XXabcdef");
    ed.keys("x");
    assert_eq!(ed.text(), "XXabcef");
}
