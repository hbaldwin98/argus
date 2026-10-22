//! Who hears which key while the editor has the keys, through the window's key walk
//! (EDITOR-KEYS, EDITOR-OPEN-9).

use gpui_kit::component::input::RopeExt as _;

use super::*;
use crate::keys::root::tests::{blur, press, type_text};

fn open_with(cx: &mut TestAppContext, text: &str) -> Opened {
    let opened = with(cx, editing(3, 1));
    hear(cx, &opened, vec![opened_with("src/lib.rs", text)]);
    settle(cx, &opened);
    sent(&opened);
    opened
}

fn ctrl(key: &str) -> String {
    format!("{}-{key}", if cfg!(target_os = "macos") { "cmd" } else { "ctrl" })
}

/// EDITOR-OPEN-9: the editor takes the keys when its file is shown, and what is typed is text:
/// the window's own letters (`j`, `e`, `c`) are not heard.
#[gpui_kit::test]
fn the_editor_takes_the_keys_and_the_stages_letters_are_letters_in_it(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    type_text(cx, &opened, "jec");
    assert_eq!(text(cx).as_deref(), Some("jecabc\n"));
    assert_eq!(sent(&opened), Vec::<String>::new(), "no `next line`, `open in editor` or `comment` was sent");
}

/// EDITOR-KEYS-1: with the keys elsewhere, the stage hears the same letters.
#[gpui_kit::test]
fn with_the_keys_elsewhere_the_stage_hears_its_letters(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    blur(cx, &opened);
    press(cx, &opened, "j");
    assert_eq!(sent(&opened), ["Code { gesture: Action { action: Next } }"]);
    assert_eq!(text(cx).as_deref(), Some("abc\n"));
}

/// EDITOR-SAVE-6, EDITOR-KEYS-4: Mod-s writes what is typed now, without waiting for the pause, and
/// the editor's own handling never sees the chord.
#[gpui_kit::test]
fn mod_s_writes_at_once(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    type_text(cx, &opened, "x");
    assert_eq!(sent(&opened), Vec::<String>::new());
    press(cx, &opened, &ctrl("s"));
    let writes = sent(&opened);
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert!(writes[0].contains("insert: \"x\""), "{}", writes[0]);
}

/// EDITOR-KEYS-4: Mod-p and Mod-Shift-f are the stage's go-to-file and search.
#[gpui_kit::test]
fn mod_p_and_mod_shift_f_are_the_stages_go_to_file_and_search(cx: &mut TestAppContext) {
    let opened = open_with(cx, "abc\n");
    press(cx, &opened, &ctrl("p"));
    assert_eq!(sent(&opened), ["Code { gesture: Action { action: Filter } }"]);
    press(cx, &opened, &ctrl("shift-f"));
    assert_eq!(sent(&opened), ["Code { gesture: Action { action: Search } }"]);
}

/// EDITOR-GUTTER-6: Alt-c makes the lines the selection covers the stage's pick and opens the box.
#[gpui_kit::test]
fn alt_c_picks_the_selected_lines_and_opens_the_comment_box(cx: &mut TestAppContext) {
    let opened = open_with(cx, "one\ntwo\nthree\nfour\n");
    let editing = state(cx);
    cx.update(|cx| editing.update(cx, |state, cx| state.set_selected_range(5..10, cx)));
    press(cx, &opened, "alt-c");
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Range { from: 2, to: 3 } } }", "Code { gesture: Action { action: Comment } }"]);
    assert_eq!(text(cx).as_deref(), Some("one\ntwo\nthree\nfour\n"), "the chord is not typed");
}

/// EDITOR-GUTTER-4: Alt-] and Alt-[ step between changes from anywhere in the editor.
#[gpui_kit::test]
fn alt_brackets_step_between_changes(cx: &mut TestAppContext) {
    let mut page = editing(10, 1);
    for row in page.lines.iter_mut().filter(|row| row.no == 4 || row.no == 8) {
        row.added = true;
    }
    let opened = with(cx, page);
    hear(cx, &opened, vec![opened_with("src/lib.rs", &(1..=10).map(|n| format!("l{n}\n")).collect::<String>())]);
    settle(cx, &opened);
    let line = |cx: &mut TestAppContext| {
        let state = state(cx);
        cx.update(|cx| {
            let state = state.read(cx);
            state.text().offset_to_point(state.cursor()).row + 1
        })
    };
    press(cx, &opened, "alt-]");
    assert_eq!(line(cx), 4);
    press(cx, &opened, "alt-]");
    assert_eq!(line(cx), 8);
    press(cx, &opened, "alt-[");
    assert_eq!(line(cx), 4);
}
