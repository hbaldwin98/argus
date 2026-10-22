//! The Record lens and a decision read as its record (BOARD-RECORD-1 to -12, BOARD-FORM-12, -13).

use argus_desktop_core::connection::page::DecisionDetail;
use gpui_kit::TestAppContext;

use super::{double_click, on, push, said, selected, view};
use crate::checkouts::stage::tests::{click_id, label, present};
use crate::keys::root::tests::{blur, click_label, drain, has_label, press, type_text};

fn decision(cx: &mut TestAppContext, opened: &crate::keys::root::tests::Opened) -> DecisionDetail {
    view(opened, cx).record.and_then(|record| record.decision).expect("a decision is under the cursor")
}

/// The Record lens with its decision open and offering `options`.
fn open_with(cx: &mut TestAppContext, opened: &crate::keys::root::tests::Opened, options: &[&str]) -> DecisionDetail {
    let mut feature = view(opened, cx);
    let record = feature.record.as_mut().expect("the Record lens");
    let decision = record.decision.as_mut().expect("a decision");
    decision.state = "open";
    decision.options = options.iter().map(|option| (*option).to_owned()).collect();
    let open = decision.clone();
    push(cx, opened, feature);
    open
}

// BOARD-RECORD-1: a row for each decision, named by its label and words, the cursor's selected.
#[gpui_kit::test]
fn each_decision_is_a_row_in_the_order_made_and_the_cursors_is_selected(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    let rows = view(&opened, cx).record.expect("the Record lens").rows;
    assert!(rows.len() >= 3);
    for (ix, row) in rows.iter().enumerate() {
        assert_eq!(label(cx, &opened, ("rrow", ix)).as_deref(), Some(format!("{} {}", row.label, row.text).as_str()), "row {ix}");
        assert_eq!(selected(cx, &opened, ("rrow", ix)), Some(row.on), "row {ix}");
    }
}

// BOARD-RECORD-4: a click puts the cursor on a decision; a double-click opens its record.
#[gpui_kit::test]
fn a_click_selects_a_decision_and_a_double_click_opens_the_one_under_the_cursor_as_its_record(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    let record = view(&opened, cx).record.expect("the Record lens");
    let other = record.rows.iter().position(|row| !row.on).expect("a row the cursor is not on");
    click_id(cx, &opened, ("rrow", other));
    assert_eq!(drain(&opened), [format!("SelectDecision {{ id: {} }}", record.rows[other].id)]);
    let cursor = record.rows.iter().position(|row| row.on).expect("the cursor row");
    double_click(cx, &opened, ("rrow", cursor));
    let document = decision(cx, &opened).document;
    assert_eq!(said(cx, &opened, "The decision as a record").as_deref(), Some(document.as_str()), "the record is edited as its document");
    assert!(has_label(cx, &opened, "Save the record"));
}

// BOARD-RECORD-6: the decision's sections, in order.
#[gpui_kit::test]
fn the_decision_reads_as_its_record(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    let decision = decision(cx, &opened);
    for section in ["Context", "Why", "Consequences", "Considered", "Carried out by"] {
        assert!(has_label(cx, &opened, section), "{section}");
    }
    assert!(!decision.consequences.is_empty() && decision.consequences.iter().any(|line| line.starts_with('+')) && decision.consequences.iter().any(|line| line.starts_with('-')));
    assert!(!has_label(cx, &opened, "Options"), "a decision made has no options to take");
}

// BOARD-RECORD-7, -9: an open decision offers its options as cards; Take waits on one.
#[gpui_kit::test]
fn an_open_decision_offers_options_and_take_waits_until_one_is_chosen(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    let open = open_with(cx, &opened, &["Three", "Five"]);
    assert!(has_label(cx, &opened, "Options") && !has_label(cx, &opened, "Why"));
    assert_eq!(label(cx, &opened, ("option", 1usize)).as_deref(), Some("Option 2: Five"));
    assert!(has_label(cx, &opened, "Take an option"), "disabled until one is chosen");
    click_label(cx, &opened, "Take an option");
    assert!(drain(&opened).is_empty(), "a disabled button sends nothing");
    click_id(cx, &opened, ("option", 1usize));
    assert!(drain(&opened).is_empty(), "choosing only lights the card: it is the window's until Take");
    assert_eq!(selected(cx, &opened, ("option", 1usize)), Some(true));
    assert_eq!(selected(cx, &opened, ("option", 0usize)), Some(false));
    click_label(cx, &opened, "Take option 2");
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: Take {{ id: {}, option: 1 }} }}", open.id)]);
}

#[gpui_kit::test]
fn an_open_decision_with_no_options_says_to_answer_in_words(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    open_with(cx, &opened, &[]);
    assert!(present(cx, &opened, "options") || has_label(cx, &opened, "Options"));
}

// BOARD-RECORD-9, BOARD-FORM-13: answering in words, with a because that does not take the keys first.
#[gpui_kit::test]
fn answering_in_words_sends_the_answer_and_why_and_leaves_the_options_to_rust(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    let open = open_with(cx, &opened, &["Three", "Five"]);
    click_label(cx, &opened, "Answer in words\u{2026}");
    assert!(has_label(cx, &opened, "Answer it") && has_label(cx, &opened, "The options not taken stay listed as rejected."));
    type_text(cx, &opened, "Four");
    drain(&opened);
    press(cx, &opened, "ctrl-enter");
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: Answer {{ id: {}, answer: \"Four\", because: Some(\"\") }} }}", open.id)]);
}

// BOARD-RECORD-9: the foot's overflow: sub-decision, supersede, copy, withdraw or restore.
#[gpui_kit::test]
fn the_overflow_copies_withdraws_supersedes_and_raises_a_sub_decision(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    let made = decision(cx, &opened);
    click_id(cx, &opened, "decision-more-button");
    click_label(cx, &opened, "Copy as ADR");
    let copied = drain(&opened);
    assert_eq!(copied.len(), 1);
    assert!(copied[0].contains(&format!("{:?}", made.adr)) && copied[0].contains(&format!("Copied {} as ADR markdown.", made.label)), "{copied:?}");
    click_id(cx, &opened, "decision-more-button");
    click_label(cx, &opened, "Withdraw");
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: Withdraw {{ id: {}, withdrawn: true }} }}", made.id)]);
    click_id(cx, &opened, "decision-more-button");
    click_label(cx, &opened, "Sub-decision");
    assert!(has_label(cx, &opened, "Sub-decision") && present(cx, &opened, "record-add"), "its line opens under the list");
    press(cx, &opened, "escape");
    click_id(cx, &opened, "decision-more-button");
    click_label(cx, &opened, "Supersede\u{2026}");
    assert!(has_label(cx, &opened, &format!("What takes {}'s place", made.label)), "{}", made.label);
    assert!(has_label(cx, &opened, &format!("{} stays on the record, dimmed, with what replaced it.", made.label)));
}

#[gpui_kit::test]
fn a_withdrawn_decision_is_restored_and_a_superseded_one_cannot_be_superseded_again(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    let mut feature = view(&opened, cx);
    let decision = feature.record.as_mut().and_then(|record| record.decision.as_mut()).expect("a decision");
    decision.withdrawn = true;
    decision.superseded_by = Some("D9".into());
    push(cx, &opened, feature);
    assert!(has_label(cx, &opened, "Withdrawn: kept on the record, and no longer shown to agents."));
    assert!(has_label(cx, &opened, "Superseded by D9: kept on the record."));
    click_id(cx, &opened, "decision-more-button");
    assert!(has_label(cx, &opened, "Restore") && !has_label(cx, &opened, "Supersede\u{2026}"), "a superseded one is not superseded again, and a withdrawn one is restored");
}

// BOARD-RECORD-10: y copies, u on a superseded decision says so.
#[gpui_kit::test]
fn the_keys_copy_the_record_and_say_when_it_is_superseded_already(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    let made = decision(cx, &opened);
    blur(cx, &opened);
    press(cx, &opened, "y");
    let copied = drain(&opened);
    assert!(copied.len() == 1 && copied[0].contains("Copied") && copied[0].contains("The clipboard refused the record."), "{copied:?}");
    press(cx, &opened, "o");
    assert!(drain(&opened).is_empty(), "o answers only an open decision, silently otherwise");
    press(cx, &opened, "x");
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: Withdraw {{ id: {}, withdrawn: true }} }}", made.id)]);
    press(cx, &opened, "u");
    assert!(present(cx, &opened, "decision-form"), "u opens the supersede form");
}

#[gpui_kit::test]
fn u_on_a_superseded_decision_raises_the_notice_and_no_form(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    let mut feature = view(&opened, cx);
    let label_of = {
        let decision = feature.record.as_mut().and_then(|record| record.decision.as_mut()).expect("a decision");
        decision.superseded_by = Some("D9".into());
        decision.label.clone()
    };
    push(cx, &opened, feature);
    blur(cx, &opened);
    press(cx, &opened, "u");
    assert!(!present(cx, &opened, "decision-form"));
    let said = cx.update(|cx| opened.heard.read(cx).notice.as_ref().map(|notice| notice.text.clone()));
    assert_eq!(said, Some(format!("{label_of} is superseded already.")));
}

// BOARD-RECORD-3, -4: an empty record, and nothing under the cursor.
#[gpui_kit::test]
fn an_empty_record_says_what_to_do_and_its_foot_raises_a_decision(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    let mut feature = view(&opened, cx);
    feature.record.as_mut().expect("the Record lens").rows.clear();
    feature.record.as_mut().unwrap().decision = None;
    push(cx, &opened, feature);
    assert!(has_label(cx, &opened, "Nothing decided yet: raise a decision, made already or still open."));
    assert!(has_label(cx, &opened, "Choose a decision to read it as a record."));
    click_id(cx, &opened, "record-add-decision");
    assert!(has_label(cx, &opened, "Decision on the feature"));
    type_text(cx, &opened, "~ no");
    drain(&opened);
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["EditFeature { edit: AddDecision { under: None, task: None, line: \"~ no\" } }"]);
}

// BOARD-RECORD-12: a does nothing on the Record lens.
#[gpui_kit::test]
fn a_does_nothing_on_the_record_lens_and_d_raises_a_decision_on_the_feature(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-record");
    blur(cx, &opened);
    press(cx, &opened, "a");
    assert!(!present(cx, &opened, "record-add") && drain(&opened).is_empty());
    press(cx, &opened, "d");
    assert!(has_label(cx, &opened, "Decision on the feature"));
}
