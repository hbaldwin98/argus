//! What Vim draws in and under the editor: the block cursor with its glyph in the editor's colour,
//! its outline while the editor has not the keys, the matches of a search, and the panel where the
//! `:` line and Vim's messages are said (EDITOR-COLOUR-8, EDITOR-VIM-5, -7, -16).

use std::ops::Range;

use gpui_kit::test::TestWindowExt as _;

use super::vim::settings;
use super::*;
use crate::keys::root::tests::{press, type_text};

fn open_with(cx: &mut TestAppContext, text: &str) -> Opened {
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![settings(true, false), opened_with("src/lib.rs", text)]);
    settle(cx, &opened);
    sent(&opened);
    opened
}

/// The ranges of the block, its outline, the glyph drawn in the editor's colour and the matches.
fn drawn(cx: &mut TestAppContext) -> [Vec<Range<usize>>; 4] {
    let editor = editor();
    cx.update(|cx| {
        let editor = editor.read(cx);
        let look = editor.sessions.get(editor.shown.as_ref().unwrap()).unwrap().look.as_ref().expect("Vim drew");
        [look.block.get_ranges(cx), look.outline.get_ranges(cx), look.inverted.get_ranges(cx), look.matches.get_ranges(cx)]
    })
}

/// A single range, as a list.
fn one(from: usize, to: usize) -> Vec<Range<usize>> {
    std::iter::once(from..to).collect()
}

fn frame(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
}

/// EDITOR-COLOUR-8: the block's character is drawn in the editor's background, so it shows on the
/// fill, and only while the block is a fill on a character.
#[gpui_kit::test]
fn the_block_draws_its_glyph_in_the_editors_colour(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    press(cx, &opened, "l");
    let [block, outline, inverted, _] = drawn(cx);
    assert_eq!(block, one(1, 2));
    assert!(outline.is_empty());
    assert_eq!(inverted, one(1, 2));
    press(cx, &opened, "i");
    assert_eq!(drawn(cx)[2], Vec::<Range<usize>>::new(), "insert mode has the caret and no glyph to invert");
}

/// EDITOR-COLOUR-8: on an empty line the block is the one cell the line break has, and there is no
/// glyph to invert.
#[gpui_kit::test]
fn the_block_is_drawn_on_an_empty_line(cx: &mut TestAppContext) {
    let opened = open_with(cx, "a\n\nb\n");
    press(cx, &opened, "j");
    let [block, _, inverted, _] = drawn(cx);
    assert_eq!(block, one(2, 3));
    assert_eq!(inverted, Vec::<Range<usize>>::new());
}

/// EDITOR-VIM-5, EDITOR-COLOUR-8: when the editor is not focused the block is an outline and the
/// glyph keeps its colour; focused again it is filled.
#[gpui_kit::test]
fn the_block_is_an_outline_while_the_editor_is_not_focused(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    assert_eq!(drawn(cx)[0], one(0, 1));
    cx.update_window(opened.handle, |_, window, cx| window.blur(cx)).unwrap();
    frame(cx, &opened);
    let [block, outline, inverted, _] = drawn(cx);
    assert!(block.is_empty() && inverted.is_empty());
    assert_eq!(outline, one(0, 1));
    let editing = state(cx);
    cx.update_window(opened.handle, |_, window, cx| editing.update(cx, |state, cx| state.focus(window, cx))).unwrap();
    frame(cx, &opened);
    assert_eq!(drawn(cx)[0], one(0, 1));
    assert_eq!(drawn(cx)[1], Vec::<Range<usize>>::new());
}

/// EDITOR-VIM-7: the matches of a search are marked, every one of them, until `:noh` clears them.
#[gpui_kit::test]
fn the_matches_of_a_search_are_marked_until_noh(cx: &mut TestAppContext) {
    let opened = open_with(cx, "ab\ncab\nb\n");
    assert_eq!(drawn(cx)[3], Vec::<Range<usize>>::new());
    type_text(cx, &opened, "/b");
    press(cx, &opened, "enter");
    assert_eq!(drawn(cx)[3], vec![1..2, 5..6, 7..8]);
    type_text(cx, &opened, ":noh");
    press(cx, &opened, "enter");
    assert_eq!(drawn(cx)[3], Vec::<Range<usize>>::new());
}

/// EDITOR-VIM-7: what is typed into the file moves the matches with it, found again in the text as it is.
#[gpui_kit::test]
fn the_matches_are_found_again_after_a_change(cx: &mut TestAppContext) {
    let opened = open_with(cx, "b\nb\n");
    type_text(cx, &opened, "/b");
    press(cx, &opened, "enter");
    assert_eq!(drawn(cx)[3], vec![0..1, 2..3]);
    type_text(cx, &opened, "dd");
    assert_eq!(drawn(cx)[3], one(0, 1));
}

/// EDITOR-VIM-16: the `:` line and Vim's messages are in a panel under the editor, not in the bar;
/// the mode stays in the bar.
#[gpui_kit::test]
fn the_command_line_is_in_a_panel_under_the_editor(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    assert!(!present(cx, &opened, "vim-line"), "no panel while Vim has nothing to say");
    type_text(cx, &opened, ":s/a");
    frame(cx, &opened);
    let (panel, editor, bar) = cx
        .update_window(opened.handle, |_, window, _| (window.find("vim-line").bounds(), window.find("code-editor").bounds(), window.find("editor-bar").bounds()))
        .unwrap();
    assert!(panel.origin.y >= bar.bottom(), "not in the bar");
    assert_eq!(panel.bottom(), editor.bottom(), "at the foot of the editor");
    assert_eq!(panel.size.width, editor.size.width, "the width of the editor");
    assert_eq!(mode(cx, &opened).as_deref(), Some("COMMAND"));
}

fn mode(cx: &mut TestAppContext, opened: &Opened) -> Option<String> {
    label(cx, opened, "vim-mode")
}
