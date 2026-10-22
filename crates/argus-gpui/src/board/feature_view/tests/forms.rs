//! The one form an open feature has open (BOARD-FORM-1 to -9).

use argus_desktop_core::connection::page::WorkAt;
use gpui_kit::test::TestQueryExt as _;
use gpui_kit::{AppContext as _, TestAppContext};

use super::{on, push, said, view};
use crate::checkouts::stage::tests::{click_id, present};
use crate::keys::root::tests::{blur, click_label, drain, has_label, press, type_text};

fn focused(cx: &mut TestAppContext, opened: &crate::keys::root::tests::Opened, name: &str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.find_by_label(name).focused().unwrap_or(false)).unwrap()
}

// BOARD-FORM-2, -8: a pushed redraw never loses what is typed nor who has the keys.
#[gpui_kit::test]
fn a_push_keeps_the_text_being_typed_and_the_field_that_has_the_keys(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    blur(cx, &opened);
    press(cx, &opened, "e");
    let title = feature.task.as_ref().expect("a task").title.clone();
    assert_eq!(said(cx, &opened, "The task, in a line").as_deref(), Some(title.as_str()));
    assert!(focused(cx, &opened, "The task, in a line"), "the title has the keys when the form opens");
    type_text(cx, &opened, " now");
    push(cx, &opened, feature.clone());
    push(cx, &opened, feature);
    assert_eq!(said(cx, &opened, "The task, in a line"), Some(format!("{title} now")), "the caret was at the end, and a push is not an edit");
    assert!(focused(cx, &opened, "The task, in a line"), "and the keys stayed");
}

// BOARD-FORM-4, -5: saving sends exactly one edit and puts the form away.
#[gpui_kit::test]
fn saving_sends_one_edit_puts_the_form_away_and_draws_nothing_new(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    let task = feature.task.clone().expect("a task");
    blur(cx, &opened);
    press(cx, &opened, "e");
    type_text(cx, &opened, "!");
    drain(&opened);
    press(cx, &opened, "ctrl-enter");
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: Task {{ id: {}, title: {:?}, body: \"\" }} }}", task.id, format!("{}!", task.title))]);
    assert!(!present(cx, &opened, "task-form") && present(cx, &opened, "task-detail"), "the detail reads as it did until the board says otherwise");
}

// BOARD-FORM-1: opening another form replaces the first.
#[gpui_kit::test]
fn opening_another_form_puts_the_first_away(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    click_id(cx, &opened, "work-add-task");
    assert!(present(cx, &opened, "work-add"));
    click_id(cx, &opened, "criterion-add");
    assert!(!present(cx, &opened, "work-add"), "one form at a time");
    assert!(!present(cx, &opened, "criterion-add"), "the criterion's field is up");
}

// BOARD-FORM-3: a plain Enter is a newline in a multi-line field and Ctrl Enter saves it.
#[gpui_kit::test]
fn a_plain_enter_is_a_newline_in_a_multi_line_field_and_ctrl_enter_saves_it(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    click_id(cx, &opened, "feature-brief");
    type_text(cx, &opened, "a");
    press(cx, &opened, "enter");
    type_text(cx, &opened, "b");
    assert!(drain(&opened).is_empty(), "typing in a form sends nothing: the form is the window's until it is saved");
    press(cx, &opened, "ctrl-enter");
    let sent = drain(&opened);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].starts_with("EditFeature { edit: Brief { body: ") && sent[0].contains("a\\nb"), "the newline was typed, not a save: {sent:?}");
}

// BOARD-FORM-7: a form is dropped when its row is no longer on screen; the brief and a new task stay.
#[gpui_kit::test]
fn a_form_on_a_row_is_dropped_when_its_row_goes_but_a_new_task_stays(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let whole = view(&opened, cx);
    let mut feature = whole.clone();
    let task = feature.task.clone().expect("a task");
    blur(cx, &opened);
    press(cx, &opened, "e");
    assert!(present(cx, &opened, "task-form"));
    feature.rows.retain(|row| !matches!(row.at, WorkAt::Task { id } if id == task.id));
    feature.task = None;
    push(cx, &opened, feature.clone());
    assert!(!present(cx, &opened, "task-form"), "the row is gone, so is its form");
    push(cx, &opened, whole);
    assert!(!present(cx, &opened, "task-form"), "and it does not come back with the row: the form was dropped, not hidden");
    click_id(cx, &opened, "work-add-task");
    push(cx, &opened, feature);
    assert!(present(cx, &opened, "work-add"), "a new task is about no row");
}

// BOARD-FORM-3: Escape puts the form away, unsent, and hands the keys back.
#[gpui_kit::test]
fn escape_cancels_the_form_and_the_keys_return_to_the_feature(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    click_id(cx, &opened, "work-add-task");
    type_text(cx, &opened, "never sent");
    drain(&opened);
    press(cx, &opened, "escape");
    assert!(!present(cx, &opened, "work-add") && drain(&opened).is_empty());
    press(cx, &opened, "j");
    assert_eq!(drain(&opened), ["FeatureAction { action: Next }"], "the outline hears its keys again");
}

// BOARD-FORM-9: the primary button says what it does and sends the same edit as the key.
#[gpui_kit::test]
fn the_primary_button_saves_as_the_key_does_and_cancel_is_a_button_too(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    click_id(cx, &opened, "work-add-task");
    type_text(cx, &opened, "By button");
    drain(&opened);
    click_id(cx, &opened, "form-save");
    assert_eq!(drain(&opened), ["EditFeature { edit: AddTask { parent: None, title: \"By button\" } }"]);
    click_id(cx, &opened, "work-add-task");
    click_id(cx, &opened, "form-cancel");
    assert!(!present(cx, &opened, "work-add") && drain(&opened).is_empty());
    click_id(cx, &opened, "feature-brief");
    assert!(has_label(cx, &opened, "Save the brief"));
    click_label(cx, &opened, "Cancel");
    assert!(!has_label(cx, &opened, "Save the brief"));
}

// BOARD-HEAD-5: choosing a lens drops a form being written.
#[gpui_kit::test]
fn choosing_a_lens_drops_the_form_being_written(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    click_id(cx, &opened, "work-add-task");
    click_id(cx, &opened, "lens-2");
    assert!(!present(cx, &opened, "work-add"));
    assert_eq!(drain(&opened), ["SetLens { lens: Record }"]);
}
