use argus_desktop_core::connection::page::FeatureView;

use super::*;
use crate::board::snapshots;

fn work() -> FeatureView {
    snapshots::open("feature-work")
}

// BOARD-FORM-9: the primary button says what it does.
#[test]
fn each_form_says_what_its_primary_button_does() {
    let words = [
        (Editing::Brief, "Save the brief"),
        (Editing::Task { id: 1 }, "Save the task"),
        (Editing::Record { id: 1 }, "Save the record"),
        (Editing::Answer { id: 1 }, "Answer it"),
        (Editing::Supersede { id: 1 }, "Supersede"),
        (Editing::Step { diagram: 1, step: 1 }, "Save the step"),
        (Editing::Flow { id: 1 }, "Save the flow"),
        (Editing::Criterion { id: 1 }, "Add"),
        (Editing::AddTask { parent: None }, "Add"),
        (Editing::AddDecision { under: None, task: None }, "Add"),
        (Editing::AddFlow, "Add"),
    ];
    for (editing, said) in words {
        assert_eq!(save_label(editing), said, "{editing:?}");
    }
}

// BOARD-WORK-16, BOARD-HEAD-4: a task opens as its title and description, the brief as the brief, and
// the title is the field that takes the keys.
#[test]
fn a_task_opens_as_its_title_and_description_with_the_title_typed_in_first() {
    let view = work();
    let task = view.task.clone().expect("the cursor is on a task");
    let filled = fill(Editing::Task { id: task.id }, &view);
    assert_eq!((filled.line.as_str(), filled.area.as_str()), (task.title.as_str(), task.body.as_str()));
    assert_eq!((filled.line_hint, filled.area_hint), ("The task, in a line", "What it is, and anything the one doing it should know"));
    assert_eq!(filled.focus, Focus::Line);
    let brief = fill(Editing::Brief, &view);
    assert_eq!((brief.area.as_str(), brief.area_hint, brief.focus), (view.brief.as_str(), "What is being built, and why", Focus::Area));
}

#[test]
fn a_form_for_a_task_that_is_not_under_the_cursor_opens_empty() {
    let view = work();
    let filled = fill(Editing::Task { id: 999 }, &view);
    assert_eq!((filled.line.as_str(), filled.area.as_str()), ("", ""));
}

// BOARD-RECORD-8, BOARD-FORM-13: a record opens as the ADR document; an answer has a because that
// is not typed in first.
#[test]
fn a_record_opens_as_its_document_and_an_answer_as_two_empty_fields() {
    let view = snapshots::open("feature-record");
    let decision = view.record.as_ref().and_then(|record| record.decision.clone()).expect("a decision is under the cursor");
    let record = fill(Editing::Record { id: decision.id }, &view);
    assert_eq!((record.area.as_str(), record.focus), (decision.document.as_str(), Focus::Area));
    let answer = fill(Editing::Answer { id: decision.id }, &view);
    assert_eq!((answer.area_hint, answer.line_hint, answer.focus), ("What was decided", "Why, if it needs saying", Focus::Area));
    assert_eq!((answer.area.as_str(), answer.line.as_str()), ("", ""));
}

// BOARD-FORM-12, -14, BOARD-FLOWS-6, -7: the flow opens as title and source, a step as its text.
#[test]
fn a_flow_opens_as_its_title_and_source_and_a_step_as_its_text() {
    let view = snapshots::open("feature-flows");
    let flow = view.flows.as_ref().and_then(|flows| flows.flow.clone()).expect("a flow is on screen");
    let step = flow.step.clone().expect("a step is lit");
    let opened = fill(Editing::Flow { id: flow.id }, &view);
    assert_eq!((opened.line.as_str(), opened.area.as_str(), opened.area_hint), (flow.title.as_str(), flow.source.as_str(), "sequenceDiagram"));
    assert_eq!(opened.focus, Focus::Line, "the source does not take the keys first");
    let step_form = fill(Editing::Step { diagram: flow.id, step: step.number }, &view);
    assert_eq!((step_form.line.as_str(), step_form.line_hint), (step.text.as_str(), "what is asked or answered"));
    let elsewhere = fill(Editing::Step { diagram: flow.id, step: step.number + 1 }, &view);
    assert_eq!(elsewhere.line, "", "a step that is not the lit one has nothing to say yet");
}

#[test]
fn a_new_row_opens_empty_with_the_hint_that_says_how_to_write_it() {
    let view = work();
    for (editing, hint) in [
        (Editing::AddTask { parent: None }, "What is to be done, in a line"),
        (Editing::Criterion { id: 3 }, "One checkable line"),
        (Editing::AddDecision { under: None, task: Some(3) }, DECISION_LINE),
        (Editing::Supersede { id: 1 }, DECISION_LINE),
        (Editing::AddFlow, "What it shows"),
    ] {
        let filled = fill(editing, &view);
        assert_eq!((filled.line.as_str(), filled.line_hint, filled.focus), ("", hint, Focus::Line), "{editing:?}");
    }
}

// BOARD-FORM-5: which edit each form sends.
#[test]
fn each_form_sends_the_one_edit_it_is_for_with_what_its_fields_say() {
    let sent = |editing, line: &str, area: &str| format!("{:?}", edit(editing, line, area));
    assert_eq!(sent(Editing::Brief, "", "why"), "Brief { body: \"why\" }");
    assert_eq!(sent(Editing::Task { id: 3 }, "t", "b"), "Task { id: 3, title: \"t\", body: \"b\" }");
    assert_eq!(sent(Editing::Record { id: 1 }, "", "doc"), "Rewrite { id: 1, document: \"doc\" }");
    assert_eq!(sent(Editing::Answer { id: 2 }, "because", "yes"), "Answer { id: 2, answer: \"yes\", because: Some(\"because\") }");
    assert_eq!(sent(Editing::Criterion { id: 3 }, "ok", ""), "AddCriterion { id: 3, text: \"ok\" }");
    assert_eq!(sent(Editing::AddTask { parent: Some(1) }, "sub", ""), "AddTask { parent: Some(1), title: \"sub\" }");
    assert_eq!(sent(Editing::AddDecision { under: None, task: Some(3) }, "? q", ""), "AddDecision { under: None, task: Some(3), line: \"? q\" }");
    assert_eq!(sent(Editing::Supersede { id: 1 }, "new", ""), "Supersede { id: 1, line: \"new\" }");
    assert_eq!(sent(Editing::AddFlow, "flow", ""), "AddFlow { title: \"flow\" }");
    assert_eq!(sent(Editing::Flow { id: 11 }, "t", "src"), "Flow { id: 11, title: \"t\", source: \"src\" }");
    assert_eq!(sent(Editing::Step { diagram: 11, step: 2 }, "says", ""), "StepText { diagram: 11, step: 2, text: \"says\" }");
}

// BOARD-FORM-7: a form is dropped when what it is about is no longer on screen.
#[test]
fn a_form_on_a_row_is_dropped_when_its_row_leaves_the_outline_and_the_brief_never_is() {
    use argus_desktop_core::connection::page::WorkAt;
    let mut view = work();
    let task = view.rows.iter().rev().find_map(|row| match row.at {
        WorkAt::Task { id } => Some(id),
        WorkAt::Decision { .. } => None,
    });
    let decision = view.rows.iter().rev().find_map(|row| match row.at {
        WorkAt::Decision { id } => Some(id),
        WorkAt::Task { .. } => None,
    });
    let (task, decision) = (task.expect("the outline has a task"), decision.expect("and a decision"));
    let about = |task: i64, decision: i64| {
        [
            Editing::Task { id: task },
            Editing::Criterion { id: task },
            Editing::Record { id: decision },
            Editing::Answer { id: decision },
            Editing::Supersede { id: decision },
            Editing::AddDecision { under: Some(decision), task: None },
            Editing::AddDecision { under: None, task: Some(task) },
        ]
    };
    for editing in about(task, decision) {
        assert!(still_there(editing, &view), "{editing:?} while its row is there");
    }
    view.rows.retain(|row| row.at != WorkAt::Task { id: task } && row.at != WorkAt::Decision { id: decision });
    for editing in about(task, decision) {
        assert!(!still_there(editing, &view), "{editing:?} once its row is gone");
    }
    for editing in [Editing::Brief, Editing::AddTask { parent: Some(task) }, Editing::AddFlow, Editing::AddDecision { under: None, task: None }] {
        assert!(still_there(editing, &view), "{editing:?} stays whatever is pushed");
    }
}

// A task and a decision are numbered apart: a decision that has the number a task's form is about is not its row.
#[test]
fn a_task_is_not_a_decision_that_happens_to_have_its_number() {
    use argus_desktop_core::connection::page::WorkAt;
    let mut view = work();
    let task = view.rows.iter().find_map(|row| match row.at {
        WorkAt::Task { id } => Some(id),
        WorkAt::Decision { .. } => None,
    });
    let task = task.expect("a task");
    view.rows.retain(|row| row.at != WorkAt::Task { id: task });
    view.rows[0].at = WorkAt::Decision { id: task };
    assert!(!still_there(Editing::Task { id: task }, &view), "a decision numbered {task} is not task {task}");
    assert!(still_there(Editing::Answer { id: task }, &view), "though it is the one an answer is about");
}

#[test]
fn a_form_on_a_flow_or_a_step_is_dropped_with_its_flow() {
    let mut view = snapshots::open("feature-flows");
    let id = view.flows.as_ref().and_then(|flows| flows.flow.as_ref()).map(|flow| flow.id).expect("a flow is on screen");
    assert!(still_there(Editing::Flow { id }, &view) && still_there(Editing::Step { diagram: id, step: 1 }, &view));
    assert!(!still_there(Editing::Flow { id: id + 100 }, &view) && !still_there(Editing::Step { diagram: id + 100, step: 1 }, &view));
    view.flows = None;
    assert!(!still_there(Editing::Flow { id }, &view), "no flows at all is no flow");
}
