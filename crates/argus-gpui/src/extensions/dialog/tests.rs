//! The extension's question in a window: raised on its own it leaves the keys where they were, asked
//! of a command just run it takes them, and each kind sends the answer the page sends.

use argus_desktop_core::connection::page::Event;
use argus_desktop_core::extensions::dialog::ExtDialogPage;
use argus_protocol::{ExtDialog, ExtDialogKind, ExtField, ExtItem};
use gpui_kit::{AppContext as _, TestAppContext};

use crate::keys::root::tests::{asked, click_label, drain, has_label, hear, open, overlays, press, type_text, Opened};
use crate::kit::overlay::kind::Overlay;

fn page(id: u64, title: &str, kind: ExtDialogKind, prompted: bool, waiting: usize) -> Event {
    let dialog = ExtDialog { title: title.into(), kind, answers: None };
    Event::ExtensionDialog(Box::new(ExtDialogPage { open: true, extension: "docker".into(), id, dialog: Some(dialog), prompted, waiting }))
}

fn confirm(id: u64, prompted: bool) -> Event {
    page(id, "Stop api-web", ExtDialogKind::Confirm { message: "Stop it?".into(), yes: Some("Stop both".into()), danger: true }, prompted, 0)
}

fn pick() -> ExtDialogKind {
    let items = ["a", "b", "c"].iter().map(|key| ExtItem { key: (*key).into(), label: format!("Item {key}") }).collect();
    ExtDialogKind::Pick { message: "Which?".into(), items }
}

fn up(cx: &mut TestAppContext) -> bool {
    overlays(cx).contains(&Overlay::Extension)
}

fn sent(opened: &Opened) -> Vec<String> {
    asked(opened).into_iter().filter(|call| call.starts_with("AnswerDialog")).collect()
}

/// EXTDIALOG-7: a question raised on its own leaves the keys with the terminal being typed in, so a
/// key goes to the pane and none is the card's.
#[gpui_kit::test]
fn a_question_raised_on_its_own_leaves_the_keys_with_the_terminal(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![confirm(9, false)]);
    assert!(up(cx), "it is drawn over the window");
    drain(&opened);
    press(cx, &opened, "y");
    press(cx, &opened, "escape");
    let said = asked(&opened);
    assert!(said.iter().all(|call| call.starts_with("Key {")), "the pane has every key: {said:?}");
    assert!(sent(&opened).is_empty() && up(cx), "Escape was the pane's, not an answer");
}

#[gpui_kit::test]
fn a_question_that_answers_a_command_takes_the_keys_and_escape_cancels_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![confirm(9, true)]);
    drain(&opened);
    press(cx, &opened, "escape");
    assert_eq!(sent(&opened), ["AnswerDialog { dialog: 9, answer: None }"]);
    assert!(!up(cx));
}

#[gpui_kit::test]
fn the_buttons_answer_yes_and_no_and_the_question_is_not_put_up_again_by_the_same_page(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![confirm(9, false)]);
    click_label(cx, &opened, "Stop both");
    assert_eq!(sent(&opened), ["AnswerDialog { dialog: 9, answer: Some(Yes) }"]);
    assert!(!up(cx));
    // The session has not yet dropped it: a page heard again must not bring it back.
    hear(cx, &opened, vec![confirm(9, false)]);
    assert!(!up(cx));
    hear(cx, &opened, vec![confirm(10, false)]);
    click_label(cx, &opened, "Cancel");
    assert!(sent(&opened).contains(&"AnswerDialog { dialog: 10, answer: None }".to_owned()), "{:?}", sent(&opened));
}

#[gpui_kit::test]
fn a_page_that_changed_only_the_count_waiting_updates_the_line_and_a_new_title_is_not_drawn(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![confirm(9, false)]);
    assert!(has_label(cx, &opened, "Asked by the docker extension"));
    let kind = ExtDialogKind::Confirm { message: "Stop it?".into(), yes: None, danger: false };
    hear(cx, &opened, vec![page(9, "Another title", kind, false, 2)]);
    assert!(has_label(cx, &opened, "Asked by the docker extension \u{b7} 2 more waiting"), "the count follows in place");
    assert!(has_label(cx, &opened, "Stop api-web") && !has_label(cx, &opened, "Another title"), "a same-id replacement is not drawn");
}

#[gpui_kit::test]
fn a_question_the_session_no_longer_holds_is_taken_away(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![confirm(9, false)]);
    assert!(up(cx));
    hear(cx, &opened, vec![Event::ExtensionDialog(Box::default())]);
    assert!(!up(cx));
}

#[gpui_kit::test]
fn a_prompt_takes_the_keys_in_its_field_and_enter_answers_with_the_text(cx: &mut TestAppContext) {
    let opened = open(cx);
    let kind = ExtDialogKind::Prompt { message: "Name?".into(), initial: String::new() };
    hear(cx, &opened, vec![page(3, "Name it", kind, true, 0)]);
    drain(&opened);
    type_text(cx, &opened, "ab");
    press(cx, &opened, "enter");
    assert_eq!(sent(&opened), ["AnswerDialog { dialog: 3, answer: Some(Text(\"ab\")) }"]);
    assert!(asked(&opened).iter().all(|call| !call.starts_with("Key {")), "none of it reached the pane");
}

#[gpui_kit::test]
fn a_pick_moves_its_lit_choice_with_the_arrows_and_enter_picks_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![page(4, "Pick one", pick(), true, 0)]);
    drain(&opened);
    press(cx, &opened, "down");
    press(cx, &opened, "down");
    press(cx, &opened, "down");
    press(cx, &opened, "up");
    press(cx, &opened, "enter");
    assert_eq!(sent(&opened), ["AnswerDialog { dialog: 4, answer: Some(Picked(\"b\")) }"], "held at the end, then back one");
}

#[gpui_kit::test]
fn a_click_on_a_choice_answers_at_once(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![page(4, "Pick one", pick(), false, 0)]);
    click_label(cx, &opened, "Item c");
    assert_eq!(sent(&opened), ["AnswerDialog { dialog: 4, answer: Some(Picked(\"c\")) }"]);
}

#[gpui_kit::test]
fn a_form_answers_with_each_field_by_name_and_its_initial_text(cx: &mut TestAppContext) {
    let opened = open(cx);
    let fields = vec![
        ExtField { name: "host".into(), label: "Host".into(), initial: "box".into() },
        ExtField { name: "port".into(), label: "Port".into(), initial: "22".into() },
    ];
    hear(cx, &opened, vec![page(5, "Connect", ExtDialogKind::Form { fields }, false, 0)]);
    click_label(cx, &opened, "OK");
    let said = sent(&opened);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("\"host\": \"box\"") && said[0].contains("\"port\": \"22\""), "{said:?}");
}

/// EXTDIALOG-5: Enter with the keys on Cancel is a press on Cancel, and answers nothing.
#[gpui_kit::test]
fn enter_on_cancel_cancels_the_question(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![confirm(9, true)]);
    drain(&opened);
    cx.update_window(opened.handle, |_, window, cx| {
        let card = cx.global::<super::Asking>().card.clone().expect("the question is up");
        let cancel = card.read(cx).cancel.clone();
        window.focus(&cancel, cx);
    })
    .unwrap();
    press(cx, &opened, "enter");
    let said = sent(&opened);
    assert_eq!(said, ["AnswerDialog { dialog: 9, answer: None }"]);
    assert!(!up(cx));
}

/// EXTDIALOG-5: Ctrl-Enter answers a prompt with its text, as Enter does.
#[gpui_kit::test]
fn ctrl_enter_answers_a_prompt_with_its_text(cx: &mut TestAppContext) {
    let opened = open(cx);
    let kind = ExtDialogKind::Prompt { message: "Name?".into(), initial: String::new() };
    hear(cx, &opened, vec![page(3, "Name it", kind, true, 0)]);
    drain(&opened);
    type_text(cx, &opened, "ab");
    press(cx, &opened, "ctrl-enter");
    assert_eq!(sent(&opened), ["AnswerDialog { dialog: 3, answer: Some(Text(\"ab\")) }"]);
}
