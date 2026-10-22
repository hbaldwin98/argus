//! The form block: fields kept across redraws, the answers a press sends, and what a press leaves
//! behind (FORM-1 to FORM-4).

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::views::blocks::form::Typed;
use argus_protocol::{Answer, AnswerValue, ViewPress};
use gpui_kit::TestAppContext;
use serde_json::{json, Value};

use super::*;

const F: &str = "7/v.0-form";

fn form(options: &[&str]) -> Value {
    json!([{"kind": "form", "id": "deploy", "submit": {"id": "go", "label": "Deploy", "tone": "good"}, "fields": [
        {"name": "branch", "label": "Branch", "input": {"kind": "text", "default": "main"}},
        {"name": "count", "label": "Count", "input": {"kind": "number", "min": 1, "max": 9}},
        {"name": "env", "label": "Environment", "input": {"kind": "choice", "options": options}},
        {"name": "loud", "label": "Loud", "input": {"kind": "toggle", "default": false}},
        {"name": "when", "label": "When", "input": {"kind": "date"}},
        {"name": "note", "label": "Note", "input": {"kind": "text", "multiline": true}}
    ]}])
}

fn agent() -> ViewCtx {
    ViewCtx { can_press: true, ..ViewCtx::agent(7, "v") }
}

fn text(name: &str, value: &str) -> Answer {
    Answer { name: name.into(), value: AnswerValue::Text(value.into()) }
}

/// What a press of Deploy asked for, as a `Call` written out.
fn pressed(answers: Vec<Answer>) -> String {
    said(Call::PressView { pane: 7, press: ViewPress { view: "v".into(), action: "go".into(), rows: vec![], form: Some("deploy".into()), answers } })
}

fn typed(cx: &mut TestAppContext, rig: &Rig, field: usize, words: &str) {
    click(cx, rig, &format!("{F}-field{field}-box"));
    cx.update_window(rig.handle, |_, window, cx| window.input(words, cx)).unwrap();
    settle(cx, rig);
}

fn draft(rig: &Rig, name: &str) -> Option<Typed> {
    rig.memory.borrow().draft(&format!("7/v.0|{name}")).cloned()
}

/// Opens the select of field `field` and takes the option `down` rows below the one it shows.
fn choose(cx: &mut TestAppContext, rig: &Rig, field: usize, down: usize) {
    click(cx, rig, &format!("{F}-field{field}-select"));
    for _ in 0..down {
        cx.update_window(rig.handle, |_, window, cx| window.press("down", cx)).unwrap();
    }
    cx.update_window(rig.handle, |_, window, cx| window.press("enter", cx)).unwrap();
    settle(cx, rig);
}

// FORM-1, FORM-3
#[gpui_kit::test]
fn a_press_sends_what_the_form_shows_with_the_defaults_for_what_nobody_touched(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, form(&["dev", "prod"]));
    click(cx, &rig, "7/v.0|go");
    assert_eq!(
        calls(&rig),
        [pressed(vec![
            text("branch", "main"),
            // The number was left empty, so it is left out; a choice shows its first option.
            text("env", "dev"),
            Answer { name: "loud".into(), value: AnswerValue::Bool(false) },
            text("when", ""),
            text("note", ""),
        ])]
    );
}

// FORM-1: each field has the control its kind asks for.
#[gpui_kit::test]
fn every_kind_of_field_is_drawn_with_its_own_control(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, form(&["dev", "prod"]));
    for k in [0, 1, 2, 4, 5] {
        assert!(exists(cx, &rig, &format!("{F}-field{k}-box")), "field {k} has a box to type in");
    }
    // The page's select, date input with its picker and number input with its arrows.
    assert!(exists(cx, &rig, &format!("{F}-field2-select")), "a choice is a select, not a row of pills");
    assert!(!exists(cx, &rig, &format!("{F}-field2-option0")));
    assert!(exists(cx, &rig, "increment") && exists(cx, &rig, "decrement"), "a number has its step buttons");
    assert!(exists(cx, &rig, &format!("{F}-field3-toggle")));
}

// FORM-2: typed text is kept; the view shown again does not put the default back.
#[gpui_kit::test]
fn what_is_typed_survives_the_view_being_shown_again_and_is_what_a_press_sends(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, form(&["dev", "prod"]));
    typed(cx, &rig, 5, "careful");
    assert_eq!(draft(&rig, "note"), Some(Typed::Text("careful".into())));
    show(cx, &rig, form(&["dev", "prod"]), agent());
    assert_eq!(draft(&rig, "note"), Some(Typed::Text("careful".into())), "a republish is not a reset");
    click(cx, &rig, "7/v.0|go");
    let said = calls(&rig);
    assert!(said[0].contains("careful"), "{said:?}");
}

#[gpui_kit::test]
fn a_toggle_and_a_choice_are_kept_and_a_stored_choice_no_longer_offered_is_not_sent(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, form(&["dev", "prod"]));
    click(cx, &rig, &format!("{F}-field3-toggle"));
    choose(cx, &rig, 2, 1);
    assert_eq!((draft(&rig, "loud"), draft(&rig, "env")), (Some(Typed::Flag(true)), Some(Typed::Text("prod".into()))));
    show(cx, &rig, form(&["dev", "staging"]), agent());
    click(cx, &rig, "7/v.0|go");
    let said = calls(&rig);
    assert!(said[0].contains("Bool(true)"), "the toggle stayed on: {said:?}");
    assert!(said[0].contains("Text(\"dev\")") && !said[0].contains("prod"), "the pick the agent took away is not sent: {said:?}");
}

#[gpui_kit::test]
fn a_number_is_sent_as_one_and_the_arrows_step_it_within_the_bounds(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, form(&["dev"]));
    click(cx, &rig, &format!("{F}-field1-box"));
    for _ in 0..12 {
        cx.update_window(rig.handle, |_, window, cx| window.press("up", cx)).unwrap();
    }
    settle(cx, &rig);
    assert_eq!(draft(&rig, "count"), Some(Typed::Text("9".into())), "held to the form's own bound");
    click(cx, &rig, "7/v.0|go");
    assert!(calls(&rig)[0].contains("Number(9.0)"), "{:?}", calls(&rig));
}

// FORM-1: the number box's buttons step by core's rule, within the bounds.
#[gpui_kit::test]
fn the_number_buttons_step_within_the_bounds(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, form(&["dev"]));
    click(cx, &rig, "decrement");
    assert_eq!(draft(&rig, "count"), Some(Typed::Text("1".into())), "an empty box steps down to the least it allows");
    click(cx, &rig, "increment");
    assert_eq!(draft(&rig, "count"), Some(Typed::Text("2".into())));
}

// FORM-1: a date picked is the YYYY-MM-DD text, which is what is sent.
#[gpui_kit::test]
fn a_date_picked_is_kept_and_sent_as_iso_text(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, form(&["dev"]));
    let state = cx.update(|cx| rig.view.read(cx).pickers().date_at(&format!("{F}-field4#0"))).expect("the date field was drawn");
    // Picking a day in the calendar says so as this event.
    let day = chrono::NaiveDate::from_ymd_opt(2026, 3, 9).unwrap().and_hms_opt(0, 0, 0).unwrap();
    cx.update(|cx| state.update(cx, |_, cx| cx.emit(gpui_kit::component::date_picker::DatePickerEvent::Change(day.into()))));
    settle(cx, &rig);
    assert_eq!(draft(&rig, "when"), Some(Typed::Text("2026-03-09".into())));
    click(cx, &rig, "7/v.0|go");
    assert!(calls(&rig)[0].contains("Text(\"2026-03-09\")"), "{:?}", calls(&rig));
}

// FORM-2: the drafts of a form are dropped when it is sent, and what it shows starts clean.
#[gpui_kit::test]
fn sending_a_form_drops_what_was_typed_so_a_second_press_sends_the_defaults(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, form(&["dev"]));
    typed(cx, &rig, 5, "once");
    click(cx, &rig, "7/v.0|go");
    assert_eq!(draft(&rig, "note"), None);
    assert_eq!(rig.memory.borrow().sent("7/v.0|"), 1);
    // The agent shows the view again, as it does when it has read the press.
    show(cx, &rig, form(&["dev"]), agent());
    click(cx, &rig, "7/v.0|go");
    let said = calls(&rig);
    assert!(said[0].contains("once") && !said[1].contains("once"), "{said:?}");
}

// Enter in a field does not submit (FORM-1).
#[gpui_kit::test]
fn enter_in_a_field_does_not_send_the_form(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, form(&["dev"]));
    click(cx, &rig, &format!("{F}-field0-box"));
    cx.update_window(rig.handle, |_, window, cx| window.press("enter", cx)).unwrap();
    settle(cx, &rig);
    assert!(calls(&rig).is_empty());
}

// FORM-3: the submit button is the action button, so the daemon that cannot take presses disables it.
#[gpui_kit::test]
fn the_submit_button_waits_when_the_daemon_cannot_take_presses(cx: &mut TestAppContext) {
    let rig = open_as(cx, 700.0, form(&["dev"]), ViewCtx::agent(7, "v"));
    click(cx, &rig, "7/v.0|go");
    assert!(calls(&rig).is_empty());
}
