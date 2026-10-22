//! Vim in the window's editor: its keys reach the buffer, the sync and the bar, the registers are
//! shared by every file, and Settings turns it off (EDITOR-VIM, EDITOR-KEYS, EDITOR-SET-1).

use argus_desktop_core::connection::page::SettingsPage;
use argus_protocol::TextChange;
use argus_desktop_core::settings::remembered::Preferences;
use gpui_kit::component::input::RopeExt as _;
use gpui_kit::ClipboardItem;

use super::*;
use crate::keys::root::tests::{press, type_text};

/// The `settings` event, with Vim on or off and grammars installed unasked or not.
pub(super) fn settings(vim: bool, installs: bool) -> Event {
    Event::Settings(Box::new(SettingsPage { chosen: Preferences { vim, install_grammars: installs, ..Preferences::default() }, min_opacity: 30, backdrops: Vec::new(), fonts: Vec::new(), version: "9.9.9".into() }))
}

/// A file open with `text`, Vim on, the calls so far cleared.
fn open_with(cx: &mut TestAppContext, text: &str) -> Opened {
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![settings(true, false), opened_with("src/lib.rs", text)]);
    settle(cx, &opened);
    sent(&opened);
    opened
}

fn cursor(cx: &mut TestAppContext) -> usize {
    let state = state(cx);
    cx.update(|cx| state.read(cx).cursor())
}

fn row(cx: &mut TestAppContext) -> usize {
    let state = state(cx);
    cx.update(|cx| {
        let state = state.read(cx);
        state.text().offset_to_point(state.cursor()).row + 1
    })
}

fn mode(cx: &mut TestAppContext, opened: &Opened) -> Option<String> {
    label(cx, opened, "vim-mode")
}

fn ctrl(key: &str) -> String {
    format!("{}-{key}", if cfg!(target_os = "macos") { "cmd" } else { "ctrl" })
}

fn after(cx: &mut TestAppContext, ms: u64) {
    cx.executor().advance_clock(std::time::Duration::from_millis(ms));
    cx.run_until_parked();
}

/// The block drawn on the character the cursor stands on, and the selection's ranges.
fn drawn(cx: &mut TestAppContext) -> (Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    let editor = editor();
    cx.update(|cx| {
        let editor = editor.read(cx);
        let look = editor.sessions.get(editor.shown.as_ref().unwrap()).unwrap().look.as_ref().expect("Vim drew");
        (look.block.get_ranges(cx), look.selected.get_ranges(cx))
    })
}

/// What Vim holds as its text, which is the buffer's.
fn vims_text(cx: &mut TestAppContext) -> Option<String> {
    let editor = editor();
    cx.update(|cx| {
        let editor = editor.read(cx);
        editor.sessions.get(editor.shown.as_ref()?)?.vim.as_ref().map(|vim| vim.doc().to_string())
    })
}

/// EDITOR-VIM-1, EDITOR-VIM-5: a file opened with Vim on shows the block on its first character
/// before any key is pressed, as the page's editor does.
#[gpui_kit::test]
fn opening_with_vim_on_draws_the_block_before_any_key(cx: &mut TestAppContext) {
    open_with(cx, "abc
def
");
    assert_eq!(drawn(cx).0, vec![0..1]);
    assert_eq!(vims_text(cx).as_deref(), Some("abc
def
"));
}

/// EDITOR-VIM-1: Vim is on, opens in normal mode and hears keys before any other keymap, so a
/// letter is a motion and types nothing; the stage's own letters are Vim's here too.
#[gpui_kit::test]
fn a_letter_is_a_motion_in_normal_mode_and_types_nothing(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\ndef\nghi\n");
    assert_eq!(mode(cx, &opened).as_deref(), Some("NORMAL"));
    press(cx, &opened, "j");
    assert_eq!(cursor(cx), 4);
    press(cx, &opened, "l");
    assert_eq!(cursor(cx), 5);
    assert_eq!(text(cx).as_deref(), Some("abc\ndef\nghi\n"));
    assert_eq!(sent(&opened), Vec::<String>::new(), "no `next line` for the stage, and nothing written");
}

/// EDITOR-VIM-5: `i` enters insert mode, typing goes in, Escape goes back; the bar names the mode.
#[gpui_kit::test]
fn i_types_until_escape_and_the_bar_names_the_mode(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    press(cx, &opened, "i");
    assert_eq!(mode(cx, &opened).as_deref(), Some("INSERT"));
    type_text(cx, &opened, "xy");
    assert_eq!(text(cx).as_deref(), Some("xyabc\n"));
    assert_eq!(bar(cx, &opened).as_deref(), Some("Unsaved"), "what Vim typed is the person's, not the disk's");
    press(cx, &opened, "escape");
    assert_eq!(mode(cx, &opened).as_deref(), Some("NORMAL"));
    assert_eq!(cursor(cx), 1, "the cursor steps back onto the last character typed");
}

/// EDITOR-SAVE-1, EDITOR-VIM-8: what a command changes is written like typing, as one change on
/// the version it was made on.
#[gpui_kit::test]
fn a_command_is_written_as_the_change_it_made(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\ndef\n");
    type_text(cx, &opened, "dd");
    assert_eq!(text(cx).as_deref(), Some("def\n"));
    assert_eq!(bar(cx, &opened).as_deref(), Some("Unsaved"));
    after(cx, 250);
    let writes = sent(&opened);
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert!(writes[0].starts_with("Edit { gesture: Change { checkout: 100, path: \"src/lib.rs\", base: 0, changes: [TextChange { from: 0, to: 4, insert: \"\" }] } }"), "{}", writes[0]);
}

/// EDITOR-VIM-9: `u` undoes the command by Vim's own history, and the undo is written too.
#[gpui_kit::test]
fn u_undoes_a_command_and_the_undo_is_written(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\ndef\n");
    type_text(cx, &opened, "dd");
    press(cx, &opened, "u");
    assert_eq!(text(cx).as_deref(), Some("abc\ndef\n"));
    after(cx, 250);
    let writes = sent(&opened);
    assert_eq!(writes.len(), 1, "one write for the burst, which nets to nothing: {writes:?}");
    press(cx, &opened, &ctrl("r"));
    assert_eq!(text(cx).as_deref(), Some("def\n"), "Ctrl-r redoes");
}

/// EDITOR-VIM-16: the keys of a command not finished are shown until it is.
#[gpui_kit::test]
fn the_keys_of_a_command_begun_are_shown_until_it_is_finished(cx: &mut TestAppContext) {
    let opened = open_with(cx, "a\nb\nc\nd\n");
    type_text(cx, &opened, "2d");
    assert_eq!(label(cx, &opened, "vim-pending").as_deref(), Some("2d"));
    press(cx, &opened, "d");
    assert!(!present(cx, &opened, "vim-pending"));
    assert_eq!(text(cx).as_deref(), Some("c\nd\n"));
}

/// EDITOR-VIM-16: the `:` line is shown as it is typed, and an error is said in the bar.
#[gpui_kit::test]
fn the_command_line_is_shown_as_it_is_typed_and_an_error_is_said(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    type_text(cx, &opened, ":s/a");
    assert_eq!(label(cx, &opened, "vim-line").as_deref(), Some(":s/a"));
    assert_eq!(mode(cx, &opened).as_deref(), Some("COMMAND"));
    press(cx, &opened, "escape");
    assert!(!present(cx, &opened, "vim-line"));
    type_text(cx, &opened, ":nosuch");
    press(cx, &opened, "enter");
    assert_eq!(label(cx, &opened, "vim-line").as_deref(), Some("Not an editor command \":nosuch\""));
}

/// EDITOR-VIM-2, EDITOR-SAVE-6: `:w` writes what was typed now, without waiting for the pause.
#[gpui_kit::test]
fn colon_w_writes_at_once(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    type_text(cx, &opened, "x");
    assert_eq!(sent(&opened), Vec::<String>::new());
    type_text(cx, &opened, ":w");
    press(cx, &opened, "enter");
    let writes = sent(&opened);
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert!(writes[0].contains("TextChange { from: 0, to: 1, insert: \"\" }"), "{}", writes[0]);
}

/// EDITOR-KEYS-4: Mod-s still writes with Vim on, as does the key's own `:w`.
#[gpui_kit::test]
fn mod_s_writes_with_vim_on(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    type_text(cx, &opened, "x");
    press(cx, &opened, &ctrl("s"));
    assert_eq!(sent(&opened).len(), 1);
    press(cx, &opened, &ctrl("shift-f"));
    assert_eq!(sent(&opened), ["Code { gesture: Action { action: Search } }"], "Mod-Shift-f is the checkout's search, not Vim's page down");
}

/// EDITOR-KEYS-8, kept as `main` has it: Ctrl-p is Vim's `k` in normal and visual mode, and the
/// stage's go to file in insert mode (open question 6, not decided here).
#[gpui_kit::test]
fn ctrl_p_is_k_in_normal_and_visual_mode_and_go_to_file_in_insert_mode(cx: &mut TestAppContext) {
    let opened = open_with(cx, "a\nb\nc\n");
    press(cx, &opened, "j");
    press(cx, &opened, "j");
    assert_eq!(row(cx), 3);
    press(cx, &opened, &ctrl("p"));
    assert_eq!(row(cx), 2, "normal: up a line");
    assert_eq!(sent(&opened), Vec::<String>::new());
    press(cx, &opened, "v");
    press(cx, &opened, &ctrl("p"));
    assert_eq!(row(cx), 1, "visual: up a line");
    assert_eq!(sent(&opened), Vec::<String>::new());
    press(cx, &opened, "escape");
    press(cx, &opened, "i");
    press(cx, &opened, &ctrl("p"));
    assert_eq!(sent(&opened), ["Code { gesture: Action { action: Filter } }"], "insert: the stage's go to file");
}

/// EDITOR-VIM-3, EDITOR-GUTTER-4: `]c` and `[c` step between the change marks.
#[gpui_kit::test]
fn bracket_c_steps_between_changes(cx: &mut TestAppContext) {
    let mut page = editing(10, 1);
    for line in page.lines.iter_mut().filter(|row| row.no == 4 || row.no == 8) {
        line.added = true;
    }
    let opened = with(cx, page);
    hear(cx, &opened, vec![settings(true, false), opened_with("src/lib.rs", &(1..=10).map(|n| format!("l{n}\n")).collect::<String>())]);
    settle(cx, &opened);
    type_text(cx, &opened, "]c");
    assert_eq!(row(cx), 4);
    type_text(cx, &opened, "]c");
    assert_eq!(row(cx), 8);
    type_text(cx, &opened, "[c");
    assert_eq!(row(cx), 4);
}

/// EDITOR-VIM-17: the registers are every file's: a yank in one is pasted in another.
#[gpui_kit::test]
fn a_yank_in_one_file_is_pasted_in_another(cx: &mut TestAppContext) {
    let opened = open_with(cx, "first\nsecond\n");
    type_text(cx, &opened, "yy");
    says(cx, &opened, other("src/other.rs"));
    hear(cx, &opened, vec![opened_with("src/other.rs", "other\n")]);
    settle(cx, &opened);
    assert_eq!(text(cx).as_deref(), Some("other\n"));
    press(cx, &opened, "p");
    assert_eq!(text(cx).as_deref(), Some("other\nfirst\n"));
}

/// EDITOR-VIM-12, -23: `"+y` writes the system clipboard and `"+p` reads it, through the host's
/// own; a plain `yy` leaves the clipboard alone.
#[gpui_kit::test]
fn the_plus_register_is_the_system_clipboard(cx: &mut TestAppContext) {
    let opened = open_with(cx, "one\ntwo\n");
    cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
    type_text(cx, &opened, "yy");
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some("kept"), "an unnamed yank does not reach the clipboard");
    type_text(cx, &opened, "\"+yy");
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some("one\n"));
    cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string("from outside\n".into())));
    type_text(cx, &opened, "\"+p");
    assert_eq!(text(cx).as_deref(), Some("ofrom outside\nne\ntwo\n"), "the clipboard is characters, put after the cursor");
}

/// EDITOR-DISK-1: an agent's write under Vim lands without moving the cursor off its text, and
/// Vim's next command acts on the text as it now is.
#[gpui_kit::test]
fn an_agents_write_leaves_vims_cursor_on_its_text(cx: &mut TestAppContext) {
    let opened = open_with(cx, "one\ntwo\nthree\n");
    type_text(cx, &opened, "jj");
    assert_eq!(cursor(cx), 8);
    heard_edit(cx, &opened, EditEvent::Changed { checkout: 100, path: "src/lib.rs".into(), from: 0, version: 1, changes: vec![TextChange { from: 0, to: 0, insert: "zero\n".into() }] });
    settle(cx, &opened);
    assert_eq!(cursor(cx), 13);
    assert_eq!(vims_text(cx), text(cx), "Vim's text is the buffer's");
    press(cx, &opened, "x");
    assert_eq!(text(cx).as_deref(), Some("zero\none\ntwo\nhree\n"));
    press(cx, &opened, "u");
    assert_eq!(text(cx).as_deref(), Some("zero\none\ntwo\nthree\n"), "undo reaches what Vim did and never the agent's write");
}

/// EDITOR-KEYS-5: a change Vim did not make (an input method's text, a paste from the menu) is
/// taken in, and the next key starts where the caret is.
#[gpui_kit::test]
fn a_change_vim_did_not_make_is_taken_in(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    press(cx, &opened, "l");
    let editing = state(cx);
    cx.update_window(opened.handle, |_, window, cx| editing.update(cx, |state, cx| state.insert("Z".to_owned(), window, cx))).unwrap();
    cx.run_until_parked();
    assert_eq!(text(cx).as_deref(), Some("aZbc\n"));
    assert_eq!(vims_text(cx).as_deref(), Some("aZbc\n"));
    press(cx, &opened, "x");
    assert_eq!(text(cx).as_deref(), Some("aZc\n"), "x acts on the character after the caret");
}

/// A click that moved the caret is where Vim's next key starts.
#[gpui_kit::test]
fn a_caret_the_pointer_moved_is_where_vims_next_key_starts(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\ndef\n");
    press(cx, &opened, "l");
    let editing = state(cx);
    cx.update(|cx| editing.update(cx, |state, cx| state.set_selected_range(5..5, cx)));
    press(cx, &opened, "x");
    assert_eq!(text(cx).as_deref(), Some("abc\ndf\n"));
}

/// EDITOR-COLOUR-8: the cursor is a block on the character it stands on, drawn beside the kit's
/// caret; insert mode has the caret alone. A visual selection is a fill over what it covers.
#[gpui_kit::test]
fn the_cursor_is_a_block_and_a_selection_is_filled(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\ndef\n");
    press(cx, &opened, "l");
    assert_eq!(drawn(cx).0, vec![1..2]);
    press(cx, &opened, "v");
    press(cx, &opened, "l");
    let (block, selected) = drawn(cx);
    assert_eq!(block, vec![2..3]);
    assert_eq!(selected, vec![1..3]);
    press(cx, &opened, "escape");
    assert_eq!(drawn(cx).1, Vec::<std::ops::Range<usize>>::new());
    press(cx, &opened, "i");
    assert_eq!(drawn(cx).0, Vec::<std::ops::Range<usize>>::new(), "insert mode has the caret itself");
}

/// A visual line selection covers whole lines, and the caret stays where the cursor is.
#[gpui_kit::test]
fn a_line_selection_covers_whole_lines(cx: &mut TestAppContext) {
    let opened = open_with(cx, "one\ntwo\nthree\n");
    press(cx, &opened, "shift-v");
    press(cx, &opened, "j");
    assert_eq!(drawn(cx).1, vec![0..8]);
    assert_eq!(mode(cx, &opened).as_deref(), Some("VISUAL LINE"));
}

/// EDITOR-GUTTER-6: with Vim's selection, Alt-c comments on the lines it covers.
#[gpui_kit::test]
fn alt_c_takes_the_lines_of_vims_selection(cx: &mut TestAppContext) {
    let opened = open_with(cx, "one\ntwo\nthree\nfour\n");
    press(cx, &opened, "shift-v");
    press(cx, &opened, "j");
    press(cx, &opened, "alt-c");
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Range { from: 1, to: 2 } } }", "Code { gesture: Action { action: Comment } }"]);
}

/// EDITOR-SET-1: turned off in Settings, the keys are text again and the mode is gone; turned on,
/// an editor is in normal mode.
#[gpui_kit::test]
fn settings_turns_vim_off_and_on_live(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    press(cx, &opened, "i");
    hear(cx, &opened, vec![settings(false, false)]);
    assert_eq!(mode(cx, &opened), None, "no mode while Vim is off");
    assert_eq!(drawn(cx).0, Vec::<std::ops::Range<usize>>::new());
    type_text(cx, &opened, "jk");
    assert_eq!(text(cx).as_deref(), Some("jkabc\n"));
    hear(cx, &opened, vec![settings(true, false)]);
    assert_eq!(mode(cx, &opened).as_deref(), Some("NORMAL"), "switched on, an editor is in normal mode");
    press(cx, &opened, "x");
    assert_eq!(text(cx).as_deref(), Some("jkbc\n"), "x acts after the caret, which Vim took from the editor");
}

/// EDITOR-VIM-18, EDITOR-STATE-6: a file the daemon has let go is read-only: no command changes it
/// and insert mode is not entered.
#[gpui_kit::test]
fn a_closed_file_takes_no_vim_edit_and_no_insert_mode(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    heard_edit(cx, &opened, EditEvent::Closed { checkout: 100, path: "src/lib.rs".into(), reason: "it is binary".into() });
    press(cx, &opened, "x");
    assert_eq!(text(cx).as_deref(), Some("abc\n"));
    press(cx, &opened, "i");
    assert_eq!(mode(cx, &opened).as_deref(), Some("NORMAL"));
}

/// EDITOR-VIM-7: `zt` and the other scrolls ask the window to scroll, which it does by the line
/// height.
#[gpui_kit::test]
fn zt_scrolls_the_cursor_line_to_the_top(cx: &mut TestAppContext) {
    let opened = open_with(cx, &(1..=200).map(|n| format!("l{n}\n")).collect::<String>());
    type_text(cx, &opened, "50G");
    type_text(cx, &opened, "zt");
    settle(cx, &opened);
    let editing = state(cx);
    let (offset, height) = cx.update(|cx| {
        let state = editing.read(cx);
        (state.scroll_offset(), state.line_height().expect("laid out"))
    });
    let top = (-f32::from(offset.y) / f32::from(height)).round() as usize;
    assert_eq!(top, 49, "line 50 is the top line");
}
