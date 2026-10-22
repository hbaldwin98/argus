use super::*;
use crate::board::snapshots;
use crate::kit::menu::item::lines;

fn flow_and_choices() -> (FlowDetail, Vec<BindChoice>) {
    let flows = snapshots::open("feature-flows").flows.expect("the Flows lens");
    (flows.flow.expect("a flow"), flows.choices)
}

// BOARD-FLOWS-8: the heading says which step, one choice for each decision.
#[test]
fn the_menu_names_the_step_and_offers_each_decision_it_can_be_bound_to() {
    let (flow, choices) = flow_and_choices();
    let items = items(&flow, &choices).expect("a step is lit");
    let step = flow.step.as_ref().unwrap().number;
    let said = lines(&items);
    assert_eq!(said[0], format!("# Bind step {step} to"));
    assert_eq!(said.len(), 1 + choices.len());
    assert_eq!(said[1], format!("{}  {}", choices[0].label, choices[0].text));
}

#[test]
fn a_decision_the_step_is_bound_to_is_held_off_with_the_word_bound() {
    let (flow, mut choices) = flow_and_choices();
    choices[0].bound = true;
    let items = items(&flow, &choices).unwrap();
    assert_eq!(items[1].as_choice().unwrap().reason.as_deref(), Some("bound"));
    assert!(items[2].enabled(), "another is still offered");
}

#[test]
fn choosing_one_binds_by_its_id_not_by_its_label() {
    let (flow, choices) = flow_and_choices();
    let items = items(&flow, &choices).unwrap();
    let binds = items[2].as_choice().unwrap().does.clone();
    assert_eq!(binds, Binds { diagram: flow.id, step: flow.step.as_ref().unwrap().number, decision: choices[1].id });
    assert_eq!(format!("{:?}", binds.edit()), format!("BindStep {{ diagram: {}, step: {}, decision: {} }}", flow.id, binds.step, choices[1].id));
}

// BOARD-FLOWS-8: with none, it says so and offers nothing.
#[test]
fn with_no_decision_the_menu_says_so_and_nothing_can_be_chosen() {
    let (flow, _) = flow_and_choices();
    let items = items(&flow, &[]).unwrap();
    assert_eq!(lines(&items)[1], "No decision to bind it to yet. ()");
    assert!(!items[1].enabled());
}

#[test]
fn no_lit_step_no_menu() {
    let (mut flow, choices) = flow_and_choices();
    flow.step = None;
    assert!(items(&flow, &choices).is_none());
}
