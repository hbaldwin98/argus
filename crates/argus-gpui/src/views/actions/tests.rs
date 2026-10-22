use super::*;
use crate::views::ctx::Kind;

fn action() -> ViewAction {
    ViewAction { id: "deploy".into(), label: "Deploy".into(), tone: Tone::Good, confirm: None }
}

#[test]
fn a_press_carries_the_rows_the_form_and_the_answers_read_at_the_moment_of_the_press() {
    let with = PressWith { rows: vec!["a".into(), "b".into()], form: Some("ship".into()), answers: vec![] };
    let call = press_call(&ViewCtx::agent(3, "ci"), &action(), with);
    let Call::PressView { pane, press } = call else { panic!("an agent's view is pressed through its pane: {call:?}") };
    assert_eq!(pane, 3);
    assert_eq!((press.view.as_str(), press.action.as_str(), press.rows, press.form), ("ci", "deploy", vec!["a".to_owned(), "b".to_owned()], Some("ship".to_owned())));
}

#[test]
fn a_saved_views_press_names_the_view_since_it_has_no_pane() {
    let ctx = ViewCtx { kind: Kind::Saved, pane: 0, ..ViewCtx::agent(0, "release") };
    assert!(matches!(press_call(&ctx, &action(), PressWith::default()), Call::PressSavedView { id, .. } if id == "release"));
}

// The page draws `.btn.vtone-*` after `.btn[disabled]`, so a button that waits or cannot be pressed
// keeps its tone.
#[test]
fn a_button_that_cannot_be_pressed_keeps_the_look_of_its_tone() {
    use gpui_kit::prelude::Styled as _;
    let tokens = crate::theme::themes::ledger::tokens();
    let fill = |tone, live| toned("a".into(), "Ship", tone, live, &tokens).style().background.clone();
    assert_eq!(fill(Tone::Good, false), fill(Tone::Good, true), "a waiting Ship is still the primary button");
    assert_eq!(fill(Tone::Good, false), Some(tokens.accent.into()));
    let edge = |tone, live| toned("a".into(), "Bad", tone, live, &tokens).style().border_color;
    assert_eq!(edge(Tone::Bad, false), Some(tokens.danger));
    assert_ne!(edge(Tone::Bad, false), edge(Tone::Neutral, false), "a pending Bad keeps its danger edge");
}
