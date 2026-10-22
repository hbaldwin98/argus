//! The buttons an agent put in a view, pressed in the blocks that hold them: what a press asks Rust
//! for, when a button waits, when it cannot be pressed, and a confirmation that survives a redraw.

use argus_desktop_core::connection::page::Call;
use argus_protocol::ViewPress;
use gpui_kit::TestAppContext;
use serde_json::{json, Value};

use super::*;
use crate::views::ctx::Kind;

fn row(confirm: Option<&str>) -> Value {
    json!([{"kind": "row", "items": [{"kind": "action", "action": {"id": "go", "label": "Go", "tone": "good", "confirm": confirm}}]}])
}

fn press(action: &str) -> ViewPress {
    ViewPress { view: "v".into(), action: action.into(), rows: vec![], form: None, answers: vec![] }
}

fn agent() -> ViewCtx {
    ViewCtx { can_press: true, ..ViewCtx::agent(7, "v") }
}

// AGENTVIEWS-12: a press tells the agent through its pane.
#[gpui_kit::test]
fn pressing_an_action_in_an_agents_view_tells_its_pane(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, row(None));
    click(cx, &rig, "7/v.0.0|go");
    assert_eq!(calls(&rig), [said(Call::PressView { pane: 7, press: press("go") })]);
}

#[gpui_kit::test]
fn pressing_an_action_in_a_saved_view_is_relayed_by_the_daemon_to_whoever_saved_it(cx: &mut TestAppContext) {
    let ctx = ViewCtx { kind: Kind::Saved, pane: 0, can_press: true, ..ViewCtx::agent(0, "v") };
    let rig = open_as(cx, 700.0, row(None), ctx);
    click(cx, &rig, "0/v.0.0|go");
    assert_eq!(calls(&rig), [said(Call::PressSavedView { id: "v".into(), press: press("go") })]);
}

#[gpui_kit::test]
fn a_button_the_daemon_cannot_take_is_disabled_and_asks_for_nothing(cx: &mut TestAppContext) {
    let rig = open_as(cx, 700.0, row(None), ViewCtx::agent(7, "v"));
    assert!(exists(cx, &rig, "7/v.0.0|go"), "it is drawn, only not pressable");
    click(cx, &rig, "7/v.0.0|go");
    assert!(calls(&rig).is_empty());
}

#[gpui_kit::test]
fn a_button_waits_while_its_press_is_unanswered_and_works_again_when_the_agent_shows_the_view(cx: &mut TestAppContext) {
    let waiting = ViewCtx { pressed: std::rc::Rc::new(vec!["go".to_owned()]), ..agent() };
    let rig = open_as(cx, 700.0, row(None), waiting);
    click(cx, &rig, "7/v.0.0|go");
    assert!(calls(&rig).is_empty(), "a second press while the first is unanswered would double the agent's work");
    show(cx, &rig, row(None), agent());
    click(cx, &rig, "7/v.0.0|go");
    assert_eq!(calls(&rig).len(), 1);
}

// FORM-3: an action with a confirm asks first.
#[gpui_kit::test]
fn an_action_with_a_confirm_asks_and_sends_only_on_yes(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, row(Some("Ship it?")));
    click(cx, &rig, "7/v.0.0|go");
    assert!(calls(&rig).is_empty(), "the question comes first");
    assert_eq!(label(cx, &rig, "7/v.0.0|go-confirm").as_deref(), Some("Ship it?"));
    click(cx, &rig, "7/v.0.0|go-no");
    assert!(calls(&rig).is_empty() && !exists(cx, &rig, "7/v.0.0|go-confirm"), "No closes it and sends nothing");
    click(cx, &rig, "7/v.0.0|go");
    click(cx, &rig, "7/v.0.0|go-yes");
    assert_eq!(calls(&rig), [said(Call::PressView { pane: 7, press: press("go") })]);
    assert!(!exists(cx, &rig, "7/v.0.0|go-confirm"), "answered, the question is gone");
}

#[gpui_kit::test]
fn a_question_being_answered_survives_the_agent_showing_the_view_again(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, row(Some("Ship it?")));
    click(cx, &rig, "7/v.0.0|go");
    show(cx, &rig, row(Some("Ship it?")), agent());
    assert!(exists(cx, &rig, "7/v.0.0|go-confirm"), "a question the person is answering must not vanish under them");
}

#[gpui_kit::test]
fn a_cards_buttons_press_in_the_card_and_every_tone_is_a_button(cx: &mut TestAppContext) {
    let actions: Vec<Value> = ["good", "active", "bad", "warn", "info", "neutral"].iter().map(|tone| json!({"id": tone, "label": tone, "tone": tone})).collect();
    let rig = open(cx, 700.0, json!([{"kind": "card", "title": "Deploy", "tone": "warn", "blocks": [], "actions": actions}]));
    for tone in ["good", "active", "bad", "warn", "info", "neutral"] {
        assert!(exists(cx, &rig, &format!("7/v.0|{tone}")), "{tone}");
    }
    click(cx, &rig, "7/v.0|bad");
    assert_eq!(calls(&rig), [said(Call::PressView { pane: 7, press: press("bad") })]);
}

// AGENTVIEWS-12: the question stays open across redraws, whatever is pending or the daemon takes.
#[gpui_kit::test]
fn an_open_question_stays_when_the_press_is_pending_or_the_daemon_cannot_take_presses(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, row(Some("Ship it?")));
    click(cx, &rig, "7/v.0.0|go");
    let waiting = ViewCtx { pressed: std::rc::Rc::new(vec!["go".to_owned()]), ..agent() };
    show(cx, &rig, row(Some("Ship it?")), waiting);
    assert!(exists(cx, &rig, "7/v.0.0|go-confirm"), "pending");
    show(cx, &rig, row(Some("Ship it?")), ViewCtx::agent(7, "v"));
    assert!(exists(cx, &rig, "7/v.0.0|go-confirm"), "a daemon without VIEW_ACTIONS");
}
