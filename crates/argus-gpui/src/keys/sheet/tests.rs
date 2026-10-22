//! The shortcuts sheet in a test window: where it opens, how a section is changed, how the find
//! field filters, and who has the keys.

use argus_desktop_core::connection::page::Event;
use argus_desktop_core::keys::keymap::{commands, groups, Action, LAYERS};
use gpui_kit::prelude::*;
use gpui_kit::test::TestQueryExt as _;
use gpui_kit::TestAppContext;

use crate::keys::root::tests::{blur, click_label, drain, has_label, hear, open, overlays, press, question_mark, type_text, Opened};
use crate::kit::overlay::kind::Overlay;

fn keymap() -> Event {
    Event::Keymap { groups: groups(), commands: commands(), layers: LAYERS }
}

/// Whether the nav's section named `title` is the current one: the button says it is selected.
fn current(cx: &mut TestAppContext, opened: &Opened, title: &str) -> Option<bool> {
    cx.update_window(opened.handle, |_, window, _| window.find_by_label(title).selected()).unwrap()
}

/// The sheet opened by a `?` the session heard in the pane that has the keys.
fn from_the_pane(cx: &mut TestAppContext) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, vec![keymap(), Event::Run { action: Action::Shortcuts }]);
    assert_eq!(overlays(cx), [Overlay::Sheet]);
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    hear(cx, &opened, Vec::new());
    opened
}

/// SHELL-SHEET-3: it opens on the section for where the person is: the pane's text area has the keys.
#[gpui_kit::test]
fn it_opens_on_the_panes_section_when_the_pane_had_the_keys_and_marks_it_here(cx: &mut TestAppContext) {
    let opened = from_the_pane(cx);
    assert_eq!(current(cx, &opened, "In a pane"), Some(true));
    assert_eq!(current(cx, &opened, "Anywhere"), Some(false));
    assert!(has_label(cx, &opened, "In a pane shortcuts"), "its rows are on the right");
    assert!(!has_label(cx, &opened, "Anywhere shortcuts"), "only the current section is drawn");
}

/// SHELL-SHEET-3: with the keys nowhere in particular it is Anywhere.
#[gpui_kit::test]
fn with_the_keys_on_the_stage_it_opens_on_anywhere(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![keymap()]);
    blur(cx, &opened);
    question_mark(cx, &opened);
    assert_eq!(overlays(cx), [Overlay::Sheet]);
    assert_eq!(current(cx, &opened, "Anywhere"), Some(true));
}

/// SHELL-SHEET-1: the keys are on the sheet's close button: a letter is held, and `?` or Escape
/// put it away.
#[gpui_kit::test]
fn the_sheet_has_the_keys_so_a_letter_goes_nowhere_and_question_mark_closes_it(cx: &mut TestAppContext) {
    let opened = from_the_pane(cx);
    drain(&opened);
    press(cx, &opened, "x");
    assert!(drain(&opened).is_empty(), "the letter did not reach the pane or the stage");
    question_mark(cx, &opened);
    assert!(overlays(cx).is_empty());
}

#[gpui_kit::test]
fn escape_closes_the_sheet(cx: &mut TestAppContext) {
    let opened = from_the_pane(cx);
    press(cx, &opened, "escape");
    assert!(overlays(cx).is_empty());
}

/// SHELL-SHEET-9: Down and Up change section and stop at the ends.
#[gpui_kit::test]
fn down_and_up_change_section_and_stop_at_the_ends(cx: &mut TestAppContext) {
    let opened = from_the_pane(cx);
    press(cx, &opened, "down");
    assert_eq!(current(cx, &opened, "Window"), Some(true));
    assert_eq!(current(cx, &opened, "In a pane"), Some(false));
    press(cx, &opened, "up");
    press(cx, &opened, "up");
    assert_eq!(current(cx, &opened, "Anywhere"), Some(true));
    press(cx, &opened, "up");
    assert_eq!(current(cx, &opened, "Anywhere"), Some(true), "no wrap past the first");
}

/// SHELL-SHEET-8: a press on a section shows it, and gives the keys back to the sheet.
#[gpui_kit::test]
fn a_press_on_a_section_shows_it_and_the_sheet_has_the_keys_again(cx: &mut TestAppContext) {
    let opened = from_the_pane(cx);
    click_label(cx, &opened, "Sidebar");
    assert_eq!(current(cx, &opened, "Sidebar"), Some(true));
    assert!(has_label(cx, &opened, "Sidebar shortcuts"));
    press(cx, &opened, "down");
    assert_eq!(current(cx, &opened, "Feature"), Some(true), "Down changes section from the list");
}

/// SHELL-SHEET-7: typing in the find field keeps the sections with a row that has every word.
#[gpui_kit::test]
fn the_find_field_keeps_the_sections_with_a_row_that_matches_every_word(cx: &mut TestAppContext) {
    let opened = from_the_pane(cx);
    click_label(cx, &opened, "Find a key or an action");
    type_text(cx, &opened, "paste");
    assert!(has_label(cx, &opened, "In a pane"), "the pane has paste");
    assert!(!has_label(cx, &opened, "Anywhere"), "a section with no row left goes");
    assert_eq!(current(cx, &opened, "In a pane"), Some(false), "while filtering none is current");
    type_text(cx, &opened, "zzzz");
    assert!(has_label(cx, &opened, "No key or action matches."));
}

/// SHELL-SHEET-13: `?` typed in the find field is typed there and does not close the sheet.
#[gpui_kit::test]
fn a_question_mark_typed_in_the_find_field_is_typed_and_closes_nothing(cx: &mut TestAppContext) {
    let opened = from_the_pane(cx);
    click_label(cx, &opened, "Find a key or an action");
    question_mark(cx, &opened);
    assert_eq!(overlays(cx), [Overlay::Sheet]);
    assert_eq!(current(cx, &opened, "In a pane"), Some(false), "the ? is in the field, so the sheet is filtering by it and none is current");
}

/// SHELL-SHEET-9: Down and Up change section from the find field too, while it is empty.
#[gpui_kit::test]
fn down_from_an_empty_find_field_changes_section(cx: &mut TestAppContext) {
    let opened = from_the_pane(cx);
    click_label(cx, &opened, "Find a key or an action");
    press(cx, &opened, "down");
    assert_eq!(current(cx, &opened, "Window"), Some(true));
}

/// SHELL-SHEET-2, 11: the sheet is generated from the keymap the session sent.
#[gpui_kit::test]
fn there_is_a_section_for_every_group_of_the_keymap(cx: &mut TestAppContext) {
    let opened = from_the_pane(cx);
    for group in groups() {
        let drawn = cx.update_window(opened.handle, |_, window, _| window.find_all_by_label(group.title).len()).unwrap();
        assert!(drawn >= 1, "{}", group.title);
    }
}

/// SHELL-SHEET-9: and Up goes back from it.
#[gpui_kit::test]
fn up_from_an_empty_find_field_goes_back_a_section(cx: &mut TestAppContext) {
    let opened = from_the_pane(cx);
    click_label(cx, &opened, "Find a key or an action");
    press(cx, &opened, "down");
    press(cx, &opened, "up");
    assert_eq!(current(cx, &opened, "In a pane"), Some(true));
}
