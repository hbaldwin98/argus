use argus_desktop_core::connection::page::{DecisionDetail, FeatureView};

use super::*;
use crate::board::snapshots;

fn work() -> FeatureView {
    snapshots::open("feature-work")
}

fn record() -> FeatureView {
    snapshots::open("feature-record")
}

fn flows() -> FeatureView {
    snapshots::open("feature-flows")
}

/// What `action` comes to as the words `Debug` says, `none` for a key the lens passes on.
fn said(act: Option<Act>) -> String {
    act.map_or_else(|| "none".to_owned(), |act| format!("{act:?}"))
}

fn decision_of(view: &FeatureView) -> DecisionDetail {
    view.record.as_ref().and_then(|record| record.decision.clone()).expect("the Record lens has a decision under the cursor")
}

/// The Work lens with its cursor on a decision in `state`.
fn on_decision(state: &'static str) -> FeatureView {
    let mut view = work();
    let mut decision = decision_of(&record());
    decision.state = state;
    if state == "superseded" {
        decision.superseded_by = Some("D9".into());
    }
    view.task = None;
    view.decision = Some(decision);
    view
}

// BOARD-HEAD-6, BOARD-WORK-24: the digits are the lenses, in every lens and with the diagram expanded.
#[test]
fn the_lens_keys_are_heard_in_every_lens_and_in_the_expanded_diagram() {
    for view in [work(), record(), flows()] {
        for expanded in [false, true] {
            assert_eq!(said(outline(&view, expanded, Action::WorkLens)), "Lens(Work)");
            assert_eq!(said(outline(&view, expanded, Action::RecordLens)), "Lens(Record)");
            assert_eq!(said(outline(&view, expanded, Action::FlowsLens)), "Lens(Flows)");
            assert_eq!(said(outline(&view, expanded, Action::Brief)), "Begin(Brief)");
            assert_eq!(said(outline(&view, expanded, Action::LinkTicket)), "Call(OpenLink { map: false })");
            assert_eq!(said(outline(&view, expanded, Action::TrackerMap)), "Call(OpenLink { map: true })");
            assert_eq!(said(outline(&view, expanded, Action::Reconcile)), "Call(OpenReconcile)");
        }
    }
}

// BOARD-WORK-10: a step is the session's, over the rows the lens draws.
#[test]
fn the_cursor_keys_are_the_sessions_in_every_lens() {
    for view in [work(), record(), flows()] {
        for (action, word) in [(Action::Next, "Next"), (Action::Previous, "Previous"), (Action::First, "First"), (Action::Last, "Last")] {
            assert_eq!(said(outline(&view, false, action)), format!("Call(FeatureAction {{ action: {word} }})"));
        }
    }
}

// BOARD-WORK-14: Edit is the row's own form.
#[test]
fn edit_opens_a_task_as_its_title_a_decision_as_its_record_and_nothing_as_the_brief() {
    let view = work();
    let id = view.task.as_ref().expect("a task").id;
    assert_eq!(said(outline(&view, false, Action::Edit)), format!("Begin(Task {{ id: {id} }})"));
    let on = on_decision("accepted");
    let id = on.decision.as_ref().unwrap().id;
    assert_eq!(said(outline(&on, false, Action::Edit)), format!("Begin(Record {{ id: {id} }})"));
    let mut nothing = work();
    nothing.task = None;
    assert_eq!(said(outline(&nothing, false, Action::Edit)), "Begin(Brief)");
}

// BOARD-WORK-10, -11: a, s and d.
#[test]
fn a_new_task_a_subtask_and_a_decision_are_written_where_the_cursor_is() {
    let view = work();
    let id = view.task.as_ref().unwrap().id;
    assert_eq!(said(outline(&view, false, Action::AddTask)), "Begin(AddTask { parent: None })");
    assert_eq!(said(outline(&view, false, Action::Under)), format!("Begin(AddTask {{ parent: Some({id}) }})"), "a subtask under a task");
    assert_eq!(said(outline(&view, false, Action::Decide)), format!("Begin(AddDecision {{ under: None, task: Some({id}) }})"), "raised under the task");
    let on = on_decision("accepted");
    let decision = on.decision.as_ref().unwrap().id;
    assert_eq!(said(outline(&on, false, Action::Under)), format!("Begin(AddDecision {{ under: Some({decision}), task: None }})"), "a sub-decision under a decision");
    let mut nothing = work();
    nothing.task = None;
    assert_eq!(said(outline(&nothing, false, Action::Under)), "Nothing", "held and nothing done (BOARD-WORK-11)");
    assert_eq!(said(outline(&nothing, false, Action::Decide)), "Begin(AddDecision { under: None, task: None })", "a decision on the feature");
}

// BOARD-WORK-10: o, x, u and y act only with the cursor on a decision and fall through on a task.
#[test]
fn the_decision_keys_fall_through_on_a_task() {
    let view = work();
    for action in [Action::Answer, Action::Withdraw, Action::Supersede, Action::CopyRecord] {
        assert_eq!(said(outline(&view, false, action)), "none", "{action:?}");
    }
}

// BOARD-RECORD-10: o answers an open decision only, x toggles, u says when superseded, y copies.
#[test]
fn a_decision_is_answered_withdrawn_superseded_and_copied_by_its_keys() {
    let open = on_decision("open");
    let id = open.decision.as_ref().unwrap().id;
    assert_eq!(said(outline(&open, false, Action::Answer)), format!("Begin(Answer {{ id: {id} }})"));
    let made = on_decision("accepted");
    assert_eq!(said(outline(&made, false, Action::Answer)), "Nothing", "silently otherwise");
    assert_eq!(said(outline(&made, false, Action::Withdraw)), format!("Edit(Withdraw {{ id: {id}, withdrawn: true }})"));
    let mut withdrawn = on_decision("withdrawn");
    withdrawn.decision.as_mut().unwrap().withdrawn = true;
    assert_eq!(said(outline(&withdrawn, false, Action::Withdraw)), format!("Edit(Withdraw {{ id: {id}, withdrawn: false }})"), "a withdrawn one is restored");
    let superseded = on_decision("superseded");
    let label = superseded.decision.as_ref().unwrap().label.clone();
    assert_eq!(said(outline(&superseded, false, Action::Supersede)), format!("Say(\"{label} is superseded already.\")"));
    assert_eq!(said(outline(&made, false, Action::Supersede)), format!("Begin(Supersede {{ id: {id} }})"));
    let (adr, label) = (made.decision.as_ref().unwrap().adr.clone(), made.decision.as_ref().unwrap().label.clone());
    let copy = format!("{:?}", outline(&made, false, Action::CopyRecord).unwrap());
    assert!(copy.contains(&format!("{adr:?}")) && copy.contains(&format!("Copied {label} as ADR markdown.")) && copy.contains("The clipboard refused the record."), "{copy}");
}

// BOARD-RECORD-5, -12: e, d and s on the Record lens; a does nothing there.
#[test]
fn the_record_lens_edits_the_record_raises_decisions_and_has_no_add_task() {
    let view = record();
    let id = decision_of(&view).id;
    assert_eq!(said(outline(&view, false, Action::Edit)), format!("Begin(Record {{ id: {id} }})"));
    assert_eq!(said(outline(&view, false, Action::Decide)), "Begin(AddDecision { under: None, task: None })");
    assert_eq!(said(outline(&view, false, Action::Under)), format!("Begin(AddDecision {{ under: Some({id}), task: None }})"));
    assert_eq!(said(outline(&view, false, Action::AddTask)), "none", "a/A does nothing on the Record lens");
}

// BOARD-WORK-12: t opens the one ticket, says there is none, or lists several.
#[test]
fn the_ticket_key_opens_one_says_none_or_asks_which() {
    let mut view = work();
    let link = view.links[0].clone();
    view.links = vec![link.clone()];
    assert_eq!(said(outline(&view, false, Action::OpenTicket)), format!("Call(OpenUrl {{ url: {:?} }})", link.url));
    view.links = vec![];
    assert_eq!(said(outline(&view, false, Action::OpenTicket)), "Say(\"No ticket is linked to this feature; T links one.\")");
    view.links = vec![link.clone(), link];
    assert_eq!(said(outline(&view, false, Action::OpenTicket)), "Call(OpenLink { map: false })");
}

#[test]
fn the_agents_pane_is_gone_to_when_there_is_one_and_the_key_is_held_when_not() {
    let mut view = work();
    let pane = view.agent.as_ref().expect("an agent works on it").pane;
    assert_eq!(said(outline(&view, false, Action::GoToPane)), format!("Call(GoToPane {{ pane: {pane} }})"));
    view.agent = None;
    assert_eq!(said(outline(&view, false, Action::GoToPane)), "Nothing");
}

// BOARD-FLOWS-9: a adds a flow, l and h change flow, f expands, e edits the source, d binds.
#[test]
fn the_flows_lens_adds_flows_moves_between_them_expands_edits_and_binds() {
    let view = flows();
    let id = view.flows.as_ref().unwrap().flow.as_ref().unwrap().id;
    assert_eq!(said(outline(&view, false, Action::AddTask)), "Begin(AddFlow)", "on this lens AddTask means a flow");
    assert_eq!(said(outline(&view, false, Action::NextFlow)), "Call(FeatureAction { action: NextFlow })");
    assert_eq!(said(outline(&view, false, Action::PreviousFlow)), "Call(FeatureAction { action: PreviousFlow })");
    assert_eq!(said(outline(&view, false, Action::Expand)), "Expand");
    assert_eq!(said(outline(&view, false, Action::Choose)), "Expand");
    assert_eq!(said(outline(&view, false, Action::Edit)), format!("Begin(Flow {{ id: {id} }})"), "the flow's source, not the step");
    assert_eq!(said(outline(&view, false, Action::Decide)), "Bind { expanded: false }");
    assert_eq!(said(outline(&view, false, Action::Under)), "none");
}

#[test]
fn with_no_flow_on_screen_only_adding_one_and_the_motions_work() {
    let mut view = flows();
    view.flows.as_mut().unwrap().flow = None;
    assert_eq!(said(outline(&view, false, Action::AddTask)), "Begin(AddFlow)");
    assert_eq!(said(outline(&view, false, Action::Next)), "Call(FeatureAction { action: Next })");
    for action in [Action::Expand, Action::Edit, Action::Decide] {
        assert_eq!(said(outline(&view, false, action)), "none", "{action:?}");
    }
}

// BOARD-SEQ-11, -19: the expanded diagram's keys, and every other key of the outline swallowed.
#[test]
fn the_expanded_diagram_zooms_fits_hides_its_steps_edits_binds_and_swallows_the_rest() {
    let view = flows();
    for (action, word) in [(Action::ZoomIn, "Zoom(0.1)"), (Action::ZoomOut, "Zoom(-0.1)"), (Action::ZoomReset, "ZoomReset"), (Action::Fit, "Fit"), (Action::StepsPanel, "Panel")] {
        assert_eq!(said(diagram(&view, action)), word);
        assert_eq!(said(outline(&view, true, action)), word, "the outline layer finds them too while expanded");
    }
    let id = view.flows.as_ref().unwrap().flow.as_ref().unwrap().id;
    assert_eq!(said(outline(&view, true, Action::Edit)), format!("Begin(Flow {{ id: {id} }})"));
    assert_eq!(said(outline(&view, true, Action::Decide)), "Bind { expanded: true }");
    for action in [Action::AddTask, Action::Under, Action::Answer, Action::Withdraw, Action::Supersede, Action::CopyRecord] {
        assert_eq!(said(outline(&view, true, action)), "none", "{action:?} is swallowed");
    }
}

// BOARD-SEQ-11: K and J move the lit step, not past the first or last.
#[test]
fn a_step_moves_up_and_down_the_flow_but_not_past_its_ends() {
    let mut view = flows();
    let flow = view.flows.as_ref().unwrap().flow.clone().unwrap();
    let (id, count) = (flow.id, flow.rows.iter().filter(|row| row.number.is_some()).count());
    let at = |number: usize, view: &mut FeatureView| {
        let flow = view.flows.as_mut().unwrap().flow.as_mut().unwrap();
        flow.step.as_mut().unwrap().number = number;
    };
    at(3, &mut view);
    assert_eq!(said(diagram(&view, Action::Earlier)), format!("MoveStep {{ diagram: {id}, step: 3, to: 2 }}"));
    assert_eq!(said(diagram(&view, Action::Later)), format!("MoveStep {{ diagram: {id}, step: 3, to: 4 }}"));
    at(1, &mut view);
    assert_eq!(said(diagram(&view, Action::Earlier)), "Nothing", "not past the first");
    at(count, &mut view);
    assert_eq!(said(diagram(&view, Action::Later)), "Nothing", "not past the last");
}
