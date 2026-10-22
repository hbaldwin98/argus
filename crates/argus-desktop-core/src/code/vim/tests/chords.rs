//! The chords Vim hears before the host (EDITOR-VIM-6, -21, EDITOR-KEYS-7, -8),
//! `Ctrl-a`/`Ctrl-x`, and the ex commands over a range (EDITOR-VIM-13).

use super::*;

#[test]
fn ctrl_p_and_ctrl_n_are_k_and_j_in_normal_and_visual_mode() {
    // EDITOR-KEYS-8: main's behaviour, pinned in each of the three modes.
    let mut ed = Ed::new("ab\ncd\nef\n");
    ed.vim.place(3);
    let step = ed.key(Key::ctrl('p'));
    assert!(step.handled, "normal: Vim takes Ctrl-p, so the stage's Go to file does not open");
    assert_eq!(ed.shows(), "|ab\ncd\nef\n");
    assert!(ed.key(Key::ctrl('n')).handled);
    assert_eq!(ed.shows(), "ab\n|cd\nef\n");

    ed.keys("v");
    assert!(ed.key(Key::ctrl('n')).handled, "visual: Ctrl-n extends by a line");
    assert_eq!(ed.mode(), Mode::Visual);
    assert_eq!(ed.vim.selection(), Some((3, 7)));
    assert!(ed.key(Key::ctrl('p')).handled);
    assert_eq!(ed.vim.selection(), Some((3, 4)));
}

#[test]
fn ctrl_p_in_insert_mode_is_the_hosts() {
    let mut ed = Ed::new("a|b");
    ed.keys("i");
    assert_eq!(ed.mode(), Mode::Insert);
    assert!(!ed.key(Key::ctrl('p')).handled, "insert: the stage's Go to file opens");
    assert!(!ed.key(Key::ctrl('n')).handled);
    assert_eq!(ed.text(), "ab");
}

#[test]
fn the_chords_vim_owns_are_taken_and_other_chords_are_the_hosts() {
    for chord in ['f', 'b', 'd', 'u', 'e', 'y', 'o', 'i', 'w'] {
        let mut ed = Ed::new("a|b");
        assert!(ed.key(Key::ctrl(chord)).handled, "Ctrl-{chord}");
        assert_eq!(ed.text(), "ab", "Ctrl-{chord} changes nothing");
        assert_eq!(ed.mode(), Mode::Normal);
    }
    let mut ed = Ed::new("a|b");
    for chord in ['s', 'k', 'z'] {
        assert!(!ed.key(Key::ctrl(chord)).handled, "Ctrl-{chord} is the host's");
    }
}

#[test]
fn ctrl_a_and_ctrl_x_change_the_number_at_or_after_the_cursor() {
    table(&[
        ("|x 7 y", "<C-a>", "x |8 y"),
        ("x |7 y", "5<C-a>", "x 1|2 y"),
        ("x 1|99 y", "<C-a>", "x 20|0 y"),
        ("|x 10", "<C-x>", "x |9"),
        ("|x 0", "<C-x>", "x -|1"),
        ("x -|5", "<C-a>", "x -|4"),
        // The library takes a `-` before digits as the sign whatever precedes it.
        ("|a-3", "<C-a>", "a-|2"),
        ("|abc", "<C-a>", "|abc"),
        ("|a 1\nb 2", "<C-a>j<C-a>", "a 2\nb |3"),
        ("|v 1", "<C-a>.", "v |3"),
    ]);
}

#[test]
fn ctrl_a_does_nothing_in_a_read_only_file() {
    let mut ed = Ed::new("|1");
    ed.vim.set_read_only(true);
    ed.keys("<C-a>");
    assert_eq!(ed.text(), "1");
}

#[test]
fn delete_yank_and_join_take_a_range() {
    table(&[
        ("|a\nb\nc\nd", ":2,3d<CR>", "a\n|d"),
        ("a\n|b\nc", ":d<CR>", "a\n|c"),
        ("|a\nb\nc", ":%d<CR>", "|"),
        ("|a\nb\nc\nd", ":2,3y<CR>G:pu<CR>", "a\nb\nc\nd\n|b\nc"),
        ("|a\nb\nc", ":2,3y<CR>:pu!<CR>", "|b\nc\na\nb\nc"),
        ("|a\nb\nc", ":j<CR>", "a| b\nc"),
        ("|a\nb\nc\nd", ":1,3j<CR>", "a b| c\nd"),
        ("|a\nb\nc", ":3<CR>", "a\nb\n|c"),
    ]);
}

#[test]
fn a_named_register_is_taken_after_the_command() {
    let mut ed = Ed::new("|a\nb");
    ed.keys(":1d x<CR>");
    assert_eq!(ed.text(), "b");
    ed.keys(":pu x<CR>");
    assert_eq!(ed.text(), "b\na");
}

#[test]
fn the_last_selection_is_the_range_of_a_command_typed_in_visual_mode() {
    table(&[("|a\nb\nc\nd", "Vj:d<CR>", "|c\nd")]);
}
