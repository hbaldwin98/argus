//! The Work lens (BOARD-WORK-1 to -27).

use argus_desktop_core::connection::page::Event;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, MouseButton, TestAppContext};

use super::{double_click, left, on, push, said, selected, view, width};
use crate::checkouts::stage::tests::{click_id, label, present};
use crate::keys::root::tests::{blur, drain, has_label, hear, press, type_text};

// BOARD-WORK-3: a row for each task and decision, named by its label and words, the cursor's selected.
#[gpui_kit::test]
fn each_task_and_decision_is_a_row_named_by_its_label_and_words_and_the_cursors_is_selected(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    assert!(feature.rows.len() >= 8, "a feature with a tree");
    for (ix, row) in feature.rows.iter().enumerate() {
        assert_eq!(label(cx, &opened, ("wrow", ix)).as_deref(), Some(format!("{} {}", row.label, row.text).as_str()), "row {ix}");
        assert_eq!(selected(cx, &opened, ("wrow", ix)), Some(row.on), "row {ix}");
    }
}

// BOARD-WORK-9: a click puts the cursor on the row, by its id.
#[gpui_kit::test]
fn a_click_on_a_row_puts_the_cursor_on_it_by_its_id_and_changes_nothing_else(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    let other = feature.rows.iter().position(|row| !row.on).expect("a row the cursor is not on");
    click_id(cx, &opened, ("wrow", other));
    let at = feature.rows[other].at;
    assert_eq!(drain(&opened), [format!("SelectWork {{ at: {at:?} }}")]);
    assert_eq!(selected(cx, &opened, ("wrow", other)), Some(false), "the row is lit when the pushed board says so (the client never predicts)");
}

// BOARD-WORK-9, -14: a double-click opens what is under the cursor to be edited.
#[gpui_kit::test]
fn a_double_click_opens_the_row_under_the_cursor_to_be_edited(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    let cursor = feature.rows.iter().position(|row| row.on).expect("the cursor is on a row");
    double_click(cx, &opened, ("wrow", cursor));
    let task = feature.task.expect("a task is under the cursor");
    assert!(present(cx, &opened, "task-form"), "the task's form opens");
    assert_eq!(said(cx, &opened, "The task, in a line").as_deref(), Some(task.title.as_str()), "on its title");
}

// BOARD-WORK-3, -5: what stands at a row's end.
#[gpui_kit::test]
fn an_open_decision_says_waiting_a_held_task_says_what_it_waits_on_and_a_parent_its_subtasks(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    let open = feature.rows.iter().position(|row| row.state == "open").expect("an open decision");
    assert!(present(cx, &opened, ("wrow-waiting", open)), "an open decision waits on a person");
    let held = feature.rows.iter().position(|row| row.waits_on.is_some()).expect("a task held up by a decision");
    let waits = feature.rows[held].waits_on.clone().unwrap();
    assert_eq!(label(cx, &opened, ("wrow-waits", held)).as_deref(), Some(format!("waits on {waits}").as_str()));
    let parent = feature.rows.iter().position(|row| row.subtasks.is_some()).expect("a task with subtasks");
    let (done, total) = feature.rows[parent].subtasks.unwrap();
    assert_eq!(label(cx, &opened, ("wrow-sub", parent)).as_deref(), Some(format!("{done}/{total}").as_str()));
    let rejected = feature.rows.iter().position(|row| row.state == "rejected").expect("a rejected decision");
    assert_eq!(label(cx, &opened, ("wrow-state", rejected)).as_deref(), Some("rejected"));
}

// BOARD-WORK-7: a dot on every row says nothing.
#[gpui_kit::test]
fn a_changed_dot_is_drawn_only_where_some_rows_changed_and_some_did_not(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let mut feature = view(&opened, cx);
    for row in &mut feature.rows {
        row.changed = false;
    }
    feature.rows[1].changed = true;
    push(cx, &opened, feature.clone());
    assert!(present(cx, &opened, ("wrow-changed", 1usize)) && !present(cx, &opened, ("wrow-changed", 0usize)));
    for row in &mut feature.rows {
        row.changed = true;
    }
    push(cx, &opened, feature);
    assert!(!present(cx, &opened, ("wrow-changed", 1usize)), "all of them changed says nothing");
}

// BOARD-WORK-2: until the tasks arrive, the outline says so; with none it says what to do.
#[gpui_kit::test]
fn the_outline_says_it_is_reading_its_tasks_and_what_to_do_when_there_are_none(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let mut feature = view(&opened, cx);
    feature.loaded = false;
    push(cx, &opened, feature.clone());
    assert!(has_label(cx, &opened, "Reading its tasks\u{2026}"));
    feature.loaded = true;
    feature.rows.clear();
    feature.task = None;
    feature.decision = None;
    push(cx, &opened, feature);
    assert!(has_label(cx, &opened, "Nothing written under it yet: add a task, or raise a decision."));
    assert!(!has_label(cx, &opened, "Reading its tasks\u{2026}"));
}

// BOARD-WORK-18: with nothing under the cursor the detail says what to do.
#[gpui_kit::test]
fn with_nothing_under_the_cursor_the_detail_says_to_choose(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let mut feature = view(&opened, cx);
    feature.task = None;
    feature.decision = None;
    push(cx, &opened, feature);
    assert!(has_label(cx, &opened, "Choose a task or a decision to read it here."));
}

// BOARD-WORK-1: the outline takes the share the page's flex gives it, less what the detail's padding has over its own.
#[gpui_kit::test]
fn the_outline_takes_a_share_of_the_split_before_it_is_dragged(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let (outline, detail) = (width(cx, &opened, "work-outline"), width(cx, &opened, "task-detail"));
    let share = outline / (outline + detail);
    assert!((0.53..0.58).contains(&share), "1.25 to 1: {share} ({outline} and {detail})");
}

// BOARD-WORK-20, -25: a width Rust kept is the outline's, whatever the share would be.
#[gpui_kit::test]
fn a_width_rust_kept_is_the_outlines(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let mut rail = cx.update(|cx| opened.heard.read(cx).rail.clone());
    rail.outline_width = Some(400);
    hear(cx, &opened, vec![Event::Rail(Box::new(rail))]);
    let zoom = cx.update_window(opened.handle, |_, window, _| crate::theme::scale::zoom(window.rem_size())).unwrap();
    let outline = width(cx, &opened, "work-outline") / zoom;
    assert!((outline - 400.0).abs() <= 26.0, "400 wide in the page's pixels, its padding and edge in it: {outline}");
}

// BOARD-WORK-20: dragging the sash reports the width once, on release; a double-click gives the usual back.
#[gpui_kit::test]
fn dragging_the_sash_reports_its_width_once_and_a_double_click_gives_the_usual_one_back(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let at = cx.update_window(opened.handle, |_, window, _| window.find("work-sash").bounds().center()).unwrap();
    let to = gpui_kit::point(at.x + gpui_kit::px(80.0), at.y);
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_move(to, Some(MouseButton::Left), cx);
    })
    .unwrap();
    assert!(drain(&opened).is_empty(), "nothing is said while it is dragged");
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(to, MouseButton::Left, cx)).unwrap();
    let sent = drain(&opened);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].starts_with("OutlineWidth { width: Some("), "{sent:?}");
    double_click(cx, &opened, "work-sash");
    assert_eq!(drain(&opened), ["OutlineWidth { width: None }"]);
}

// BOARD-WORK-10: j, k, Home and End step over the rows drawn.
#[gpui_kit::test]
fn the_cursor_keys_step_the_session_and_nothing_is_predicted(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    blur(cx, &opened);
    for (key, action) in [("j", "Next"), ("k", "Previous"), ("down", "Next"), ("up", "Previous"), ("home", "First"), ("end", "Last")] {
        press(cx, &opened, key);
        assert_eq!(drain(&opened), [format!("FeatureAction {{ action: {action} }}")], "{key}");
    }
}

// BOARD-WORK-13: the footer's buttons open the root forms and a form adds what is written.
#[gpui_kit::test]
fn the_footer_buttons_open_the_root_forms_and_a_form_adds_what_is_written(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    click_id(cx, &opened, "work-add-task");
    assert!(present(cx, &opened, "work-add") && has_label(cx, &opened, "New task"));
    type_text(cx, &opened, "Write the docs");
    drain(&opened);
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["EditFeature { edit: AddTask { parent: None, title: \"Write the docs\" } }"], "a plain Enter saves a one-line field");
    assert!(!present(cx, &opened, "work-add"), "the form is put away; the outline changes when the board does");
    click_id(cx, &opened, "work-add-decision");
    assert!(has_label(cx, &opened, "Decision on the feature"));
    type_text(cx, &opened, "? which one");
    drain(&opened);
    press(cx, &opened, "ctrl-enter");
    assert_eq!(drain(&opened), ["EditFeature { edit: AddDecision { under: None, task: None, line: \"? which one\" } }"]);
}

// BOARD-WORK-13: the form says what it is under, and Escape puts it away unsent.
#[gpui_kit::test]
fn a_new_row_form_says_what_it_is_under_and_escape_puts_it_away_unsent(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    blur(cx, &opened);
    press(cx, &opened, "s");
    let task = feature.task.expect("a task is under the cursor");
    assert!(has_label(cx, &opened, &format!("New subtask under {}", task.label)));
    press(cx, &opened, "escape");
    assert!(!present(cx, &opened, "work-add"), "Escape puts the form away first");
    assert!(drain(&opened).is_empty(), "and sends nothing");
}

// BOARD-WORK-8, -22: the legend is asked for and Escape puts it away before the feature.
#[gpui_kit::test]
fn the_legend_is_asked_for_with_the_question_mark_and_escape_puts_it_away_before_the_feature(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    assert!(!present(cx, &opened, "legend-pop"));
    click_id(cx, &opened, "legend-ask");
    assert!(present(cx, &opened, "legend-pop"), "it hangs under the header");
    for (ix, words) in ["to do", "in progress", "done", "decided", "open", "rejected"].into_iter().enumerate() {
        assert_eq!(label(cx, &opened, ("legend-item", ix)).as_deref(), Some(words), "the mark of the {words} state is in it");
    }
    blur(cx, &opened);
    press(cx, &opened, "escape");
    assert!(!present(cx, &opened, "legend-pop") && drain(&opened).is_empty(), "the legend first, and the feature stays");
    press(cx, &opened, "escape");
    assert_eq!(drain(&opened), ["OpenFeature { slug: None }"], "then the feature");
}

// BOARD-WORK-7: the legend names the dot only when one is drawn.
#[gpui_kit::test]
fn the_legend_names_the_changed_dot_only_when_one_is_drawn(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let mut feature = view(&opened, cx);
    for row in &mut feature.rows {
        row.changed = false;
    }
    push(cx, &opened, feature.clone());
    click_id(cx, &opened, "legend-ask");
    assert!(!has_label(cx, &opened, "changed since reconcile"));
    feature.rows[0].changed = true;
    push(cx, &opened, feature);
    assert!(has_label(cx, &opened, "changed since reconcile"));
}

// BOARD-WORK-15: a criterion is a checkbox that sends the edit, and a cross that removes it.
#[gpui_kit::test]
fn a_criterion_is_a_checkbox_that_sends_its_edit_and_a_cross_that_removes_it(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let task = view(&opened, cx).task.expect("a task is under the cursor");
    assert!(task.criteria.len() >= 2, "the scene's task has criteria");
    let open = task.criteria.iter().position(|c| !c.done).expect("an unchecked criterion");
    click_id(cx, &opened, ("criterion-check", open));
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: SetCriterion {{ id: {}, index: {open}, done: true }} }}", task.id)]);
    click_id(cx, &opened, ("criterion-remove", open));
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: RemoveCriterion {{ id: {}, index: {open} }} }}", task.id)]);
}

// BOARD-WORK-15, -26: a criterion is added from its own one-line field, which takes the button's place.
#[gpui_kit::test]
fn a_criterion_is_added_from_its_own_one_line_field(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let task = view(&opened, cx).task.expect("a task is under the cursor");
    click_id(cx, &opened, "criterion-add");
    assert!(!present(cx, &opened, "criterion-add"), "the button gives way to the field while it is written");
    type_text(cx, &opened, "It is fast");
    drain(&opened);
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: AddCriterion {{ id: {}, text: \"It is fast\" }} }}", task.id)]);
}

// BOARD-WORK-15: a task with no description offers to write one; its foot goes to the agent's pane.
#[gpui_kit::test]
fn a_task_without_a_description_offers_to_add_one_and_its_foot_goes_to_its_agents_pane(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    let task = feature.task.clone().expect("a task is under the cursor");
    assert!(task.body.is_empty() && present(cx, &opened, "task-add-description"));
    click_id(cx, &opened, "task-add-description");
    assert!(present(cx, &opened, "task-form"), "it opens the edit form");
    press(cx, &opened, "escape");
    click_id(cx, &opened, "task-agent");
    let pane = feature.agent.expect("an agent works on it").pane;
    assert_eq!(drain(&opened), [format!("GoToPane {{ pane: {pane} }}")]);
}

// BOARD-WORK-15: what the task is built on and the notes under it.
#[gpui_kit::test]
fn the_task_names_what_binds_it_and_its_notes(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let task = view(&opened, cx).task.expect("a task is under the cursor");
    assert!(!task.bound_by.is_empty() && !task.notes.is_empty());
    for words in ["Bound by", "Notes", "Description", "Done when"] {
        assert!(has_label(cx, &opened, words), "{words}");
    }
    assert!(left(cx, &opened, "task-detail") > left(cx, &opened, "work-outline"));
}
