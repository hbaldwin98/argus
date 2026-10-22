//! The Flows lens, its steps, the bind menu and the diagram (BOARD-FLOWS-1 to -14, BOARD-SEQ-1 to -20).

use argus_desktop_core::connection::page::FlowDetail;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, MouseButton, TestAppContext};

use super::{on, push, said, selected, view, width};
use crate::checkouts::stage::tests::{click_id, label, present};
use crate::keys::root::tests::{blur, click_label, drain, has_label, press, type_text, Opened};

fn flow(cx: &mut TestAppContext, opened: &Opened) -> FlowDetail {
    view(opened, cx).flows.and_then(|flows| flows.flow).expect("a flow is on screen")
}

fn centre(cx: &mut TestAppContext, opened: &Opened, id: (&'static str, usize)) -> gpui_kit::Point<gpui_kit::Pixels> {
    cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().center()).unwrap()
}

// BOARD-FLOWS-1: a row for each flow, the one read selected, with what it holds.
#[gpui_kit::test]
fn each_flow_is_a_row_with_its_summary_and_the_one_read_is_selected(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let flows = view(&opened, cx).flows.expect("the Flows lens");
    assert!(flows.flows.len() >= 2);
    for (ix, item) in flows.flows.iter().enumerate() {
        assert_eq!(label(cx, &opened, ("frow", ix)).as_deref(), Some(format!("{} {}", item.title, item.summary).as_str()), "flow {ix}");
        assert_eq!(selected(cx, &opened, ("frow", ix)), Some(item.on), "flow {ix}");
    }
}

#[gpui_kit::test]
fn a_click_on_a_flow_asks_for_it_keeping_the_step_when_it_is_the_same_flow(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let flows = view(&opened, cx).flows.expect("the Flows lens");
    let other = flows.flows.iter().position(|item| !item.on).expect("another flow");
    click_id(cx, &opened, ("frow", other));
    assert_eq!(drain(&opened), [format!("SelectStep {{ diagram: {}, step: None }}", flows.flows[other].id)]);
}

// BOARD-FLOWS-3: a numbered row for each step; each branch a row that is not pressed.
#[gpui_kit::test]
fn each_step_is_a_row_with_its_route_and_words_and_a_branch_is_not_pressed(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let flow = flow(cx, &opened);
    for (ix, row) in flow.rows.iter().enumerate() {
        let expected = match row.number {
            Some(number) => format!("{number} {} {}", row.route, row.text),
            None => format!("{} {}", row.route, row.text),
        };
        assert_eq!(label(cx, &opened, ("steprow", ix)).as_deref(), Some(expected.as_str()), "row {ix}");
    }
    let branch = flow.rows.iter().position(|row| row.number.is_none()).expect("a branch");
    click_id(cx, &opened, ("steprow", branch));
    assert!(drain(&opened).is_empty(), "a branch is not a step");
    let lit = flow.rows.iter().position(|row| row.on).expect("a lit step");
    assert_eq!(selected(cx, &opened, ("steprow", lit)), Some(true));
}

#[gpui_kit::test]
fn a_click_on_a_step_puts_the_cursor_on_it_counted_from_zero(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let flow = flow(cx, &opened);
    let ix = flow.rows.iter().position(|row| row.number == Some(1)).expect("step 1");
    click_id(cx, &opened, ("steprow", ix));
    assert_eq!(drain(&opened), [format!("SelectStep {{ diagram: {}, step: Some(0) }}", flow.id)]);
}

// BOARD-FLOWS-2: flows arrive after the board.
#[gpui_kit::test]
fn flows_not_yet_arrived_say_so_and_no_flow_says_to_choose_one(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let mut feature = view(&opened, cx);
    let flows = feature.flows.as_mut().expect("the Flows lens");
    flows.loaded = false;
    flows.flows.clear();
    flows.flow = None;
    push(cx, &opened, feature.clone());
    assert!(has_label(cx, &opened, "Reading its flows\u{2026}"));
    assert!(has_label(cx, &opened, "Choose a flow to draw it here."));
    feature.flows.as_mut().unwrap().loaded = true;
    push(cx, &opened, feature);
    assert!(!has_label(cx, &opened, "Reading its flows\u{2026}"));
}

// BOARD-FLOWS-4: what stands behind the step under the cursor.
#[gpui_kit::test]
fn a_step_nothing_binds_says_so_and_a_bound_one_names_what_binds_and_carries_it(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let flow = flow(cx, &opened);
    let step = flow.step.clone().expect("a lit step");
    assert!(step.bound_by.is_empty() && has_label(cx, &opened, &format!("Nothing binds step {} yet.", step.number)));
    let mut feature = view(&opened, cx);
    let detail = feature.flows.as_mut().and_then(|flows| flows.flow.as_mut()).and_then(|flow| flow.step.as_mut()).expect("a step");
    detail.bound_by = vec![argus_desktop_core::connection::page::Line { label: "D1".into(), text: "Name it".into(), aside: None }];
    detail.carried_out_by = vec![argus_desktop_core::connection::page::Line { label: "1.1".into(), text: "Do it".into(), aside: None }];
    push(cx, &opened, feature);
    assert!(has_label(cx, &opened, &format!("Step {} is bound by", step.number)) && has_label(cx, &opened, "Carried out by"));
    assert!(!has_label(cx, &opened, &format!("Nothing binds step {} yet.", step.number)));
}

// BOARD-FLOWS-6: the step's words as a form.
#[gpui_kit::test]
fn editing_a_step_opens_its_words_and_saving_sends_them(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let flow = flow(cx, &opened);
    let step = flow.step.clone().expect("a lit step");
    click_id(cx, &opened, "step-edit");
    assert!(has_label(cx, &opened, &format!("Step {} says", step.number)));
    assert_eq!(said(cx, &opened, "what is asked or answered").as_deref(), Some(step.text.as_str()));
    type_text(cx, &opened, "!");
    drain(&opened);
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: StepText {{ diagram: {}, step: {}, text: {:?} }} }}", flow.id, step.number, format!("{}!", step.text))]);
}

// BOARD-FLOWS-7, BOARD-FORM-14: the flow's source and title.
#[gpui_kit::test]
fn editing_a_flows_source_opens_its_title_and_mermaid_and_saving_sends_both(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let flow = flow(cx, &opened);
    blur(cx, &opened);
    press(cx, &opened, "e");
    assert!(present(cx, &opened, "flow-form") && has_label(cx, &opened, "Mermaid source"));
    assert_eq!(said(cx, &opened, "What the flow shows").as_deref(), Some(flow.title.as_str()));
    assert_eq!(said(cx, &opened, "sequenceDiagram").as_deref(), Some(flow.source.as_str()));
    drain(&opened);
    press(cx, &opened, "ctrl-enter");
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: Flow {{ id: {}, title: {:?}, source: {:?} }} }}", flow.id, flow.title, flow.source)]);
}

// BOARD-FLOWS-5: + Flow adds one from a title.
#[gpui_kit::test]
fn a_flow_is_added_from_a_title(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flows-add");
    assert!(has_label(cx, &opened, "New flow"));
    type_text(cx, &opened, "Closing");
    drain(&opened);
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["EditFeature { edit: AddFlow { title: \"Closing\" } }"]);
    blur(cx, &opened);
    press(cx, &opened, "a");
    assert!(has_label(cx, &opened, "New flow"), "on this lens `a` adds a flow, not a task");
}

// BOARD-FLOWS-9: l and h change flow.
#[gpui_kit::test]
fn the_flow_keys_step_the_flow_and_the_step(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    blur(cx, &opened);
    for (key, action) in [("l", "NextFlow"), ("right", "NextFlow"), ("h", "PreviousFlow"), ("left", "PreviousFlow"), ("j", "Next"), ("k", "Previous")] {
        press(cx, &opened, key);
        assert_eq!(drain(&opened), [format!("FeatureAction {{ action: {action} }}")], "{key}");
    }
}

// BOARD-FLOWS-8: the bind menu, one choice for each decision, held off when already bound.
#[gpui_kit::test]
fn the_bind_menu_binds_the_step_to_a_decision_by_its_id_and_holds_off_one_bound_already(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let mut feature = view(&opened, cx);
    let flows = feature.flows.as_mut().expect("the Flows lens");
    assert!(flows.choices.len() >= 2, "decisions to bind to");
    flows.choices[0].bound = true;
    let (first, second) = (flows.choices[0].clone(), flows.choices[1].clone());
    let (diagram, number) = {
        let flow = flows.flow.as_ref().expect("a flow");
        (flow.id, flow.step.as_ref().expect("a step").number)
    };
    push(cx, &opened, feature);
    click_id(cx, &opened, "step-bind");
    click_label(cx, &opened, &format!("{}  {}", first.label, first.text));
    assert!(drain(&opened).is_empty(), "bound already: held off");
    click_id(cx, &opened, "step-bind");
    click_label(cx, &opened, &format!("{}  {}", second.label, second.text));
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: BindStep {{ diagram: {diagram}, step: {number}, decision: {} }} }}", second.id)]);
}

#[gpui_kit::test]
fn the_bind_key_opens_the_menu_under_the_lit_step_and_with_no_decision_says_so(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    blur(cx, &opened);
    press(cx, &opened, "d");
    let choices = view(&opened, cx).flows.expect("the Flows lens").choices;
    let first = &choices[0];
    assert!(has_label(cx, &opened, &format!("{}  {}", first.label, first.text)), "the menu is open");
    press(cx, &opened, "escape");
    assert!(!has_label(cx, &opened, &format!("{}  {}", first.label, first.text)), "Escape puts it away");
    let mut feature = view(&opened, cx);
    feature.flows.as_mut().unwrap().choices.clear();
    push(cx, &opened, feature);
    press(cx, &opened, "d");
    assert!(has_label(cx, &opened, "No decision to bind it to yet."));
}

// BOARD-SEQ-3, -5: every step is an arrow named by its route and words, and a click selects it.
#[gpui_kit::test]
fn each_step_is_an_arrow_that_a_click_selects(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let flow = flow(cx, &opened);
    let steps = flow.rows.iter().filter(|row| row.number.is_some()).count();
    for n in 1..=steps {
        assert!(present(cx, &opened, ("seq-step", n)), "step {n}");
    }
    assert!(label(cx, &opened, ("seq-step", 3usize)).is_some_and(|words| words.starts_with("Step 3: ")));
    click_id(cx, &opened, ("seq-step", 2usize));
    assert_eq!(drain(&opened), [format!("SelectStep {{ diagram: {}, step: Some(1) }}", flow.id)]);
}

// BOARD-FLOWS-12: the diagram is scaled to fit the box, not below 70% nor above its size.
#[gpui_kit::test]
fn the_inline_diagram_is_scaled_to_fit_and_never_below_seventy_percent(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    let flow = flow(cx, &opened);
    let natural = argus_desktop_core::board::lenses::picture::layout(&flow.diagram).width;
    let zoom = cx.update_window(opened.handle, |_, window, _| crate::theme::scale::zoom(window.rem_size())).unwrap();
    let drawn = width(cx, &opened, "seq") / zoom;
    let scale = drawn / natural;
    assert!((0.699..=1.001).contains(&scale), "{scale}");
}

// BOARD-FLOWS-11, BOARD-SEQ-7: Expand opens the diagram over the whole window.
#[gpui_kit::test]
fn expand_opens_the_diagram_over_the_window_and_escape_puts_it_away_before_the_feature(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    assert!(!present(cx, &opened, "expanded"));
    click_id(cx, &opened, "flow-expand");
    assert!(present(cx, &opened, "expanded") && present(cx, &opened, "seq-bar") && present(cx, &opened, "seq-panel"));
    let size = cx.update_window(opened.handle, |_, window, _| (window.bounds().size, window.find("expanded").bounds().size)).unwrap();
    assert_eq!(size.0, size.1, "over the whole window");
    blur(cx, &opened);
    press(cx, &opened, "escape");
    assert!(!present(cx, &opened, "expanded") && drain(&opened).is_empty(), "the diagram first, and the feature stays");
    press(cx, &opened, "f");
    assert!(present(cx, &opened, "expanded"), "f expands it");
    press(cx, &opened, "escape");
    press(cx, &opened, "escape");
    assert_eq!(drain(&opened), ["OpenFeature { slug: None }"], "then the feature");
}

// BOARD-SEQ-8, -9: zoom, fit and the steps panel.
#[gpui_kit::test]
fn the_toolbar_zooms_fits_and_shows_or_hides_the_steps(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    let percent = |cx: &mut TestAppContext| label(cx, &opened, "seq-zoom-pct").expect("the percent");
    let fitted = percent(cx);
    click_id(cx, &opened, "seq-zoom-in");
    assert_ne!(percent(cx), fitted, "zooming by hand moves it");
    click_id(cx, &opened, "seq-zoom-pct");
    assert_eq!(percent(cx), "100%", "the percent button gives 100% back");
    click_id(cx, &opened, "seq-fit");
    assert_eq!(percent(cx), fitted, "Fit goes back to the fit");
    click_id(cx, &opened, "seq-steps");
    assert!(!present(cx, &opened, "seq-panel") && has_label(cx, &opened, "Show steps"));
    click_id(cx, &opened, "seq-steps");
    assert!(present(cx, &opened, "seq-panel") && has_label(cx, &opened, "Hide steps"));
}

#[gpui_kit::test]
fn the_diagram_keys_zoom_fit_and_hide_the_steps_and_the_percent_follows(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    blur(cx, &opened);
    press(cx, &opened, "0");
    assert_eq!(label(cx, &opened, "seq-zoom-pct").as_deref(), Some("100%"));
    press(cx, &opened, "+");
    assert_eq!(label(cx, &opened, "seq-zoom-pct").as_deref(), Some("110%"));
    press(cx, &opened, "-");
    press(cx, &opened, "-");
    assert_eq!(label(cx, &opened, "seq-zoom-pct").as_deref(), Some("90%"));
    press(cx, &opened, "p");
    assert!(!present(cx, &opened, "seq-panel"));
    press(cx, &opened, "e");
    assert!(present(cx, &opened, "seq-panel") && present(cx, &opened, "flow-form"), "editing the source shows the steps it is edited in");
}

// BOARD-SEQ-8: the crumbs, and the flow switcher.
#[gpui_kit::test]
fn the_crumbs_collapse_or_leave_and_the_switcher_chooses_a_flow_from_its_first_step(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    click_id(cx, &opened, "seq-crumb-flows");
    assert!(!present(cx, &opened, "expanded"), "the Flows crumb collapses");
    click_id(cx, &opened, "flow-expand");
    click_id(cx, &opened, "seq-crumb-features");
    assert!(!present(cx, &opened, "expanded"));
    assert_eq!(drain(&opened), ["OpenFeature { slug: None }"], "the first crumb also leaves the feature");
    click_id(cx, &opened, "flow-expand");
    let flows = view(&opened, cx).flows.expect("the Flows lens");
    let other = flows.flows.iter().find(|item| !item.on).expect("another flow").clone();
    click_id(cx, &opened, "seq-flow");
    click_label(cx, &opened, &other.title);
    assert_eq!(drain(&opened), [format!("SelectStep {{ diagram: {}, step: Some(0) }}", other.id)]);
    click_id(cx, &opened, "seq-collapse");
    assert!(!present(cx, &opened, "expanded"));
}

// BOARD-SEQ-16: a participant moves a place, held at the ends.
#[gpui_kit::test]
fn a_participant_moves_a_place_and_is_held_at_the_ends(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    let flow = flow(cx, &opened);
    let step = flow.step.clone().expect("a lit step");
    let at = flow.diagram.lifelines.iter().position(|l| l.label == step.participants[0]).expect("a lifeline");
    let name = flow.diagram.lifelines[at].name.clone();
    click_id(cx, &opened, ("seq-move-left", 0usize));
    if at > 0 {
        assert_eq!(drain(&opened), [format!("EditFeature {{ edit: MoveParticipant {{ diagram: {}, name: {name:?}, to: {} }} }}", flow.id, at - 1)]);
    } else {
        assert!(drain(&opened).is_empty(), "the first cannot go left");
    }
    click_id(cx, &opened, ("seq-move-right", 0usize));
    if at + 1 < flow.diagram.lifelines.len() {
        assert_eq!(drain(&opened), [format!("EditFeature {{ edit: MoveParticipant {{ diagram: {}, name: {name:?}, to: {} }} }}", flow.id, at + 1)]);
    } else {
        assert!(drain(&opened).is_empty(), "the last cannot go right");
    }
}

// BOARD-SEQ-13: dragging an arrow sends the move and keeps the cursor on the step.
#[gpui_kit::test]
fn dragging_an_arrow_to_another_sends_the_move_and_keeps_the_cursor_on_it(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    let flow = flow(cx, &opened);
    let (from, to) = (centre(cx, &opened, ("seq-big-step", 1)), centre(cx, &opened, ("seq-big-step", 4)));
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(from, MouseButton::Left, cx);
        window.pointer_move(to, Some(MouseButton::Left), cx);
    })
    .unwrap();
    assert!(drain(&opened).is_empty(), "a drag only says where it would land");
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(to, MouseButton::Left, cx)).unwrap();
    assert_eq!(
        drain(&opened),
        [format!("EditFeature {{ edit: MoveStep {{ diagram: {}, step: 1, to: 4 }} }}", flow.id), format!("SelectStep {{ diagram: {}, step: Some(3) }}", flow.id)]
    );
}

#[gpui_kit::test]
fn a_press_that_never_travels_is_a_click_on_its_arrow_even_in_the_expanded_view(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    let flow = flow(cx, &opened);
    let at = centre(cx, &opened, ("seq-big-step", 2));
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    assert_eq!(drain(&opened), [format!("SelectStep {{ diagram: {}, step: Some(1) }}", flow.id)], "a click selects, and moves nothing");
}

#[gpui_kit::test]
fn escape_puts_a_drag_back_with_nothing_sent_and_the_diagram_stays(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    let (from, to) = (centre(cx, &opened, ("seq-big-step", 1)), centre(cx, &opened, ("seq-big-step", 4)));
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(from, MouseButton::Left, cx);
        window.pointer_move(to, Some(MouseButton::Left), cx);
    })
    .unwrap();
    press(cx, &opened, "escape");
    cx.update_window(opened.handle, |_, window, cx| window.pointer_up(to, MouseButton::Left, cx)).unwrap();
    assert!(drain(&opened).is_empty(), "nothing was dropped");
    assert!(present(cx, &opened, "expanded"), "the diagram is still expanded: Escape put the drag back");
}

// BOARD-SEQ-14: dragging a lifeline's head sends the participant's new place.
#[gpui_kit::test]
fn dragging_a_lifelines_head_sideways_sends_its_new_place(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    let flow = flow(cx, &opened);
    let last = flow.diagram.lifelines.len() - 1;
    let (from, to) = (centre(cx, &opened, ("seq-big-head", 0)), centre(cx, &opened, ("seq-big-head", last)));
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(from, MouseButton::Left, cx);
        window.pointer_move(to, Some(MouseButton::Left), cx);
        window.pointer_up(to, MouseButton::Left, cx);
    })
    .unwrap();
    assert_eq!(drain(&opened), [format!("EditFeature {{ edit: MoveParticipant {{ diagram: {}, name: {:?}, to: {last} }} }}", flow.id, flow.diagram.lifelines[0].name)]);
}

// BOARD-SEQ-6: inline, a head is not dragged.
#[gpui_kit::test]
fn inline_the_diagram_has_no_heads_to_drag(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    assert!(!present(cx, &opened, ("seq-head", 0usize)));
}

// BOARD-SEQ-19: while it is open the other keys of the outline do nothing.
#[gpui_kit::test]
fn while_the_diagram_is_open_the_other_outline_keys_are_swallowed(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    blur(cx, &opened);
    for key in ["a", "s", "o", "x", "u", "y"] {
        press(cx, &opened, key);
        assert!(drain(&opened).is_empty() && !present(cx, &opened, "flow-add"), "{key}");
    }
}

// BOARD-SEQ-11: K moves the lit step up, not past the first.
#[gpui_kit::test]
fn the_move_keys_move_the_lit_step_and_hold_at_the_ends(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    blur(cx, &opened);
    let flow = flow(cx, &opened);
    let number = flow.step.clone().expect("a lit step").number;
    assert!(number > 1, "the scene lights step 3");
    press(cx, &opened, "K");
    assert_eq!(
        drain(&opened),
        [format!("EditFeature {{ edit: MoveStep {{ diagram: {}, step: {number}, to: {} }} }}", flow.id, number - 1), format!("SelectStep {{ diagram: {}, step: Some({}) }}", flow.id, number - 2)]
    );
}

// BOARD-SEQ-7: the diagram goes when its flow does, and stays while flows are still coming.
#[gpui_kit::test]
fn the_diagram_goes_when_its_flow_goes_but_stays_while_flows_are_still_coming(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-flows");
    click_id(cx, &opened, "flow-expand");
    let whole = view(&opened, cx);
    let mut loading = whole.clone();
    loading.flows.as_mut().unwrap().loaded = false;
    loading.flows.as_mut().unwrap().flow = None;
    push(cx, &opened, loading);
    assert!(!present(cx, &opened, "expanded"), "nothing to draw yet");
    push(cx, &opened, whole.clone());
    assert!(present(cx, &opened, "expanded"), "it was only waiting");
    let mut gone = whole.clone();
    gone.flows.as_mut().unwrap().flow = None;
    push(cx, &opened, gone);
    push(cx, &opened, whole);
    assert!(!present(cx, &opened, "expanded"), "its flow went, and so did it");
}
