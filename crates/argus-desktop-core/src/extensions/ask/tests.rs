use super::*;

fn dialog(kind: ExtDialogKind) -> ExtDialog {
    ExtDialog { title: "Stop api-web".into(), kind, answers: None }
}

fn confirm(yes: Option<&str>, danger: bool) -> ExtDialog {
    dialog(ExtDialogKind::Confirm { message: "Sure?".into(), yes: yes.map(str::to_owned), danger })
}

fn items() -> Vec<ExtItem> {
    ["a", "b", "c"].iter().map(|key| ExtItem { key: (*key).into(), label: key.to_uppercase() }).collect()
}

#[test]
fn the_asker_line_says_how_many_more_wait_only_when_some_do() {
    assert_eq!(asker("docker", 0), "Asked by the docker extension");
    assert_eq!(asker("docker", 2), "Asked by the docker extension \u{b7} 2 more waiting");
}

#[test]
fn the_answering_button_says_the_confirms_word_or_the_kinds() {
    assert_eq!(yes_words(&confirm(None, false)), "Yes");
    assert_eq!(yes_words(&confirm(Some("Stop both"), true)), "Stop both");
    assert_eq!(yes_words(&dialog(ExtDialogKind::Pick { message: "m".into(), items: items() })), "Choose");
    assert_eq!(yes_words(&dialog(ExtDialogKind::Prompt { message: "m".into(), initial: String::new() })), "OK");
    assert_eq!(yes_words(&dialog(ExtDialogKind::Form { fields: Vec::new() })), "OK");
}

#[test]
fn only_a_dangerous_confirm_is_a_danger_button() {
    assert!(is_danger(&confirm(None, true)));
    assert!(!is_danger(&confirm(None, false)));
    assert!(!is_danger(&dialog(ExtDialogKind::Prompt { message: "m".into(), initial: String::new() })));
}

#[test]
fn a_form_answers_with_each_field_by_its_name() {
    let fields = vec![
        ExtField { name: "host".into(), label: "Host".into(), initial: String::new() },
        ExtField { name: "port".into(), label: "Port".into(), initial: "22".into() },
    ];
    let ExtAnswer::Form(given) = form_answer(&fields, &["box".into(), "2222".into()]) else { panic!("a form") };
    assert_eq!(given.get("host").map(String::as_str), Some("box"));
    assert_eq!(given.get("port").map(String::as_str), Some("2222"));
}

#[test]
fn the_lit_choice_is_held_at_both_ends_and_picking_it_answers_with_its_key() {
    assert_eq!(step(0, 3, -1), 0);
    assert_eq!(step(0, 3, 1), 1);
    assert_eq!(step(2, 3, 1), 2);
    assert_eq!(step(0, 0, 1), 0, "an empty list");
    assert_eq!(picked(&items(), 1), Some(ExtAnswer::Picked("b".into())));
    assert_eq!(picked(&[], 0), None);
}

#[test]
fn escape_cancels_and_enter_or_ctrl_enter_submits_but_enter_on_a_button_is_the_buttons() {
    assert_eq!(pressed(Some(Action::CloseLayer), false, false), Pressed::Cancel);
    assert_eq!(pressed(Some(Action::CloseLayer), true, false), Pressed::Cancel, "even on a button");
    assert_eq!(pressed(Some(Action::Choose), false, false), Pressed::Submit);
    assert_eq!(pressed(Some(Action::Choose), true, false), Pressed::Pass);
    assert_eq!(pressed(Some(Action::Save), true, false), Pressed::Submit, "ctrl-Enter is never a click");
}

#[test]
fn the_arrows_move_the_lit_choice_only_in_a_pick() {
    assert_eq!(pressed(Some(Action::Next), false, true), Pressed::Move(1));
    assert_eq!(pressed(Some(Action::Previous), false, true), Pressed::Move(-1));
    assert_eq!(pressed(Some(Action::Next), false, false), Pressed::Pass);
    assert_eq!(pressed(None, false, true), Pressed::Pass);
}
