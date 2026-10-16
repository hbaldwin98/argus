//! What an edit to an open feature asks of the daemon: each gesture as the
//! one message the terminal client sends for the same edit, in the
//! protocol's words.
//!
//! A write is checked here only against the bounds `argus-protocol` sets,
//! so a refusal is said at once rather than as the daemon's; the daemon
//! checks it again. Nothing here changes what the window shows: the pushed
//! board and tasks do, so a refused edit leaves the feature as it is.
//! A new row saved blank is a form put away: it sends nothing and says
//! nothing.

use argus_protocol::flow;
use argus_protocol::{
    checked_task_body, checked_task_line, ClientMsg, DecisionBoard, DecisionChange, DecisionState,
    DecisionText, DecisionWrite, DiagramAction, DiagramList, DiagramWrite, FeatureAction, SequenceDiagram,
    TaskAction, TaskList, TaskWrite, MAX_CRITERION_BYTES, MAX_FEATURE_BODY_BYTES,
};

use crate::feature_stage::Scope;
use crate::page::FeatureEdit;

/// The feature an edit is to, and what is known of it.
pub struct Target<'a> {
    pub scope: &'a Scope,
    pub slug: &'a str,
    pub board: &'a DecisionBoard,
    pub tasks: Option<&'a TaskList>,
    pub diagrams: Option<&'a DiagramList>,
}

/// What a flow added from the desktop starts as: the terminal client's
/// skeleton, two lifelines and a request answered, to be edited.
const SKELETON: &str = "sequenceDiagram\n    participant Client\n    participant Daemon\n    Client->>Daemon: request\n    Daemon-->>Client: response";

/// The messages `edit` sends — none when it changes nothing — or why it
/// cannot be sent.
pub fn messages(edit: FeatureEdit, target: &Target) -> Result<Vec<ClientMsg>, String> {
    let task = |action| ClientMsg::Task {
        project: target.scope.project,
        checkout: target.scope.checkout,
        feature: target.slug.to_string(),
        action,
    };
    let change = |change| ClientMsg::ChangeDecision {
        project: target.scope.project,
        checkout: target.scope.checkout,
        feature: target.slug.to_string(),
        change,
    };
    let diagram = |action| ClientMsg::SequenceDiagram {
        project: target.scope.project,
        checkout: target.scope.checkout,
        feature: target.slug.to_string(),
        action,
    };
    // A drag or a binding is the flow's source rewritten and sent back
    // whole; `None` from the rewrite is a gesture that changes nothing.
    let rewritten = |id: i64, rewrite: &dyn Fn(&str) -> Option<String>| -> Result<Vec<ClientMsg>, String> {
        let flow = flow_of(target, id)?;
        Ok(match rewrite(&flow.body) {
            Some(body) => {
                let write = DiagramWrite { title: flow.title.clone(), body }.checked()?;
                vec![diagram(DiagramAction::Rewrite { id, write })]
            }
            None => Vec::new(),
        })
    };
    let added = match &edit {
        FeatureEdit::AddCriterion { text, .. } => Some(text),
        FeatureEdit::AddTask { title, .. } | FeatureEdit::AddFlow { title } => Some(title),
        FeatureEdit::AddDecision { line, .. } | FeatureEdit::Supersede { line, .. } => Some(line),
        _ => None,
    };
    if added.is_some_and(|text| text.trim().is_empty()) {
        return Ok(Vec::new());
    }
    Ok(match edit {
        FeatureEdit::Brief { body } => {
            let feature = target.board.features.iter().find(|f| f.slug == target.slug).ok_or("that feature is gone")?;
            if body.trim() == feature.body.trim() {
                return Ok(Vec::new());
            }
            if body.trim().len() > MAX_FEATURE_BODY_BYTES {
                return Err("a feature document is a brief, not a design document".into());
            }
            vec![ClientMsg::Feature {
                project: target.scope.project,
                checkout: target.scope.checkout,
                action: FeatureAction::Rewrite { slug: target.slug.to_string(), body },
                request_id: 0,
            }]
        }
        FeatureEdit::Task { id, title, body } => {
            let current = target.tasks.and_then(|list| list.tasks.iter().find(|t| t.id == id)).ok_or("that task is gone")?;
            let mut out = Vec::new();
            let title = TaskWrite { title, ..TaskWrite::default() }.checked()?.title;
            if title != current.title {
                out.push(task(TaskAction::Retitle { id, title }));
            }
            if checked_task_body(body.clone())? != current.body {
                out.push(task(TaskAction::SetBody { id, body }));
            }
            out
        }
        FeatureEdit::AddCriterion { id, text } => {
            vec![task(TaskAction::AddCriterion { id, text: checked_task_line(&text, MAX_CRITERION_BYTES)? })]
        }
        FeatureEdit::SetCriterion { id, index, done } => vec![task(TaskAction::SetCriterion { id, index, done })],
        FeatureEdit::RemoveCriterion { id, index } => vec![task(TaskAction::RemoveCriterion { id, index })],
        FeatureEdit::AddTask { parent, title } => {
            vec![task(TaskAction::Add(TaskWrite { title, external: None, parent }.checked()?))]
        }
        FeatureEdit::AddDecision { under, task, line } => {
            let (state, chose) = DecisionState::typed(&line);
            let write = DecisionWrite { chose: chose.to_string(), under, task, state, ..DecisionWrite::default() };
            vec![ClientMsg::Decide {
                project: target.scope.project,
                checkout: target.scope.checkout,
                feature: target.slug.to_string(),
                write: write.checked()?,
            }]
        }
        FeatureEdit::Take { id, option } => {
            let decision = open_decision(target, id)?;
            let answer = decision.options.get(option).cloned().ok_or("that option is gone")?;
            vec![change(DecisionChange::Resolve { id, answer, because: None })]
        }
        FeatureEdit::Answer { id, answer, because } => {
            open_decision(target, id)?;
            let answer = answer.trim().to_string();
            if answer.is_empty() {
                return Err("an answer has to say what was decided".into());
            }
            let because = because.map(|b| b.trim().to_string()).filter(|b| !b.is_empty());
            vec![change(DecisionChange::Resolve { id, answer, because })]
        }
        FeatureEdit::Rewrite { id, document } => {
            let text = DecisionText::from_markdown(&document).checked()?;
            vec![change(DecisionChange::Rewrite { id, text })]
        }
        FeatureEdit::Withdraw { id, withdrawn } => {
            vec![change(if withdrawn { DecisionChange::Withdraw { id } } else { DecisionChange::Restore { id } })]
        }
        FeatureEdit::Supersede { id, line } => {
            target.board.decisions.iter().find(|d| d.id == id).ok_or("that decision is gone")?;
            let (state, chose) = DecisionState::typed(&line);
            let write = DecisionWrite { chose: chose.to_string(), supersedes: Some(id), state, ..DecisionWrite::default() };
            vec![ClientMsg::Decide {
                project: target.scope.project,
                checkout: target.scope.checkout,
                feature: target.slug.to_string(),
                write: write.checked()?,
            }]
        }
        FeatureEdit::AddFlow { title } => {
            vec![diagram(DiagramAction::Add(DiagramWrite { title: title.trim().to_string(), body: SKELETON.into() }.checked()?))]
        }
        FeatureEdit::Flow { id, title, source } => {
            let flow = flow_of(target, id)?;
            let write = DiagramWrite { title: title.trim().to_string(), body: source }.checked()?;
            if write.title == flow.title && write.body == flow.body {
                return Ok(Vec::new());
            }
            vec![diagram(DiagramAction::Rewrite { id, write })]
        }
        FeatureEdit::BindStep { diagram: id, step, decision } => {
            target.board.decisions.iter().find(|d| d.id == decision).ok_or("that decision is gone")?;
            rewritten(id, &|body| flow::bind(body, step, decision))?
        }
        FeatureEdit::StepText { diagram: id, step, text } => {
            if text.trim().is_empty() {
                return Err("a step has to say what is asked or answered".into());
            }
            rewritten(id, &|body| flow::retext(body, step, &text))?
        }
        FeatureEdit::MoveStep { diagram: id, step, to } => rewritten(id, &|body| flow::move_step(body, step, to))?,
        FeatureEdit::MoveParticipant { diagram: id, name, to } => {
            rewritten(id, &|body| flow::move_participant(body, &name, to))?
        }
    })
}

fn flow_of<'a>(target: &Target<'a>, id: i64) -> Result<&'a SequenceDiagram, String> {
    let gone = || "that flow is gone".to_string();
    target.diagrams.and_then(|list| list.diagrams.iter().find(|d| d.id == id)).ok_or_else(gone)
}

fn open_decision<'a>(target: &Target<'a>, id: i64) -> Result<&'a argus_protocol::Decision, String> {
    let decision = target.board.decisions.iter().find(|d| d.id == id).ok_or("that decision is gone")?;
    if !decision.open() {
        return Err("only an open decision is waiting on an answer".into());
    }
    Ok(decision)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use argus_protocol::{CheckoutId, ProjectId, RepositoryId};

    fn scope() -> Scope {
        Scope {
            project: ProjectId(1),
            project_name: "src".into(),
            repository: RepositoryId(2),
            repository_name: "argus".into(),
            checkout: CheckoutId(3),
            key: "/src/main".into(),
        }
    }

    fn send(edit: FeatureEdit) -> Result<Vec<ClientMsg>, String> {
        let mut board = fixtures::board();
        board.decisions[1].options = vec!["Every standing decision".into(), "Only the binding ones".into()];
        let tasks = fixtures::tasks("trim");
        let diagrams = fixtures::diagrams("trim");
        let scope = scope();
        messages(edit, &Target { scope: &scope, slug: "trim", board: &board, tasks: Some(&tasks), diagrams: Some(&diagrams) })
    }

    /// The source a flow edit sends back for flow 11.
    fn rewritten(edit: FeatureEdit) -> String {
        let ClientMsg::SequenceDiagram { action: DiagramAction::Rewrite { id: 11, write }, .. } = one(edit) else {
            panic!("not flow 11 rewritten");
        };
        assert_eq!(write.title, "Asking for the brief", "the title stays");
        write.body
    }

    fn steps(source: &str) -> Vec<String> {
        flow::rows(source)
            .into_iter()
            .filter_map(|row| match row {
                flow::FlowRow::Step(step) => Some(step.text),
                flow::FlowRow::Branch { .. } => None,
            })
            .collect()
    }

    #[test]
    fn dragging_a_step_rewrites_the_flow_and_sends_it_back_whole() {
        let body = rewritten(FeatureEdit::MoveStep { diagram: 11, step: 1, to: 3 });
        assert_eq!(steps(&body)[..3], ["brief request", "find the pane's feature", "ask for the brief"]);
        assert!(send(FeatureEdit::MoveStep { diagram: 11, step: 2, to: 2 }).unwrap().is_empty(), "a drop where it was");
        assert!(send(FeatureEdit::MoveStep { diagram: 99, step: 1, to: 2 }).is_err());
    }

    #[test]
    fn a_step_edited_says_its_new_text_and_an_empty_one_is_refused() {
        let body = rewritten(FeatureEdit::StepText { diagram: 11, step: 2, text: "the brief, please".into() });
        assert_eq!(steps(&body)[1], "the brief, please");
        assert!(body.contains("the brief, please\n    %% decision #1"), "its binding stays: {body}");
        assert!(send(FeatureEdit::StepText { diagram: 11, step: 2, text: " ".into() }).is_err());
    }

    #[test]
    fn dragging_a_lifeline_declares_the_new_order() {
        let body = rewritten(FeatureEdit::MoveParticipant { diagram: 11, name: "argusd".into(), to: 0 });
        let order: Vec<_> = flow::participants(&body).into_iter().map(|p| p.id).collect();
        assert_eq!(order, ["argusd", "agent", "hook"]);
    }

    #[test]
    fn binding_a_step_writes_the_decision_under_it() {
        let body = rewritten(FeatureEdit::BindStep { diagram: 11, step: 1, decision: 2 });
        assert!(body.contains("agent->>hook: ask for the brief\n    %% decision #2"), "{body}");
        assert!(send(FeatureEdit::BindStep { diagram: 11, step: 2, decision: 1 }).unwrap().is_empty(), "already bound");
        assert!(send(FeatureEdit::BindStep { diagram: 11, step: 1, decision: 99 }).is_err());
    }

    #[test]
    fn a_flow_is_added_from_a_skeleton_and_edited_as_its_source() {
        let ClientMsg::SequenceDiagram { action: DiagramAction::Add(write), .. } = one(FeatureEdit::AddFlow { title: " Retry ".into() })
        else {
            panic!();
        };
        assert_eq!((write.title.as_str(), steps(&write.body).len()), ("Retry", 2));
        assert!(send(FeatureEdit::AddFlow { title: " ".into() }).unwrap().is_empty());
        let same = FeatureEdit::Flow { id: 11, title: "Asking for the brief".into(), source: fixtures::FLOW.into() };
        assert!(send(same).unwrap().is_empty());
        let body = rewritten(FeatureEdit::Flow { id: 11, title: "Asking for the brief".into(), source: "sequenceDiagram\n    a->>b: x".into() });
        assert_eq!(steps(&body), ["x"]);
    }

    #[test]
    fn superseding_records_a_decision_that_takes_the_old_ones_place() {
        let ClientMsg::Decide { write, .. } = one(FeatureEdit::Supersede { id: 1, line: "Link it instead".into() }) else {
            panic!();
        };
        assert_eq!((write.chose.as_str(), write.supersedes, write.state), ("Link it instead", Some(1), DecisionState::Accepted));
        assert!(send(FeatureEdit::Supersede { id: 99, line: "x".into() }).is_err());
    }

    fn one(edit: FeatureEdit) -> ClientMsg {
        let mut out = send(edit).unwrap();
        assert_eq!(out.len(), 1, "{out:?}");
        out.remove(0)
    }

    #[test]
    fn taking_an_option_resolves_the_decision_with_it() {
        let ClientMsg::ChangeDecision { feature, change, .. } = one(FeatureEdit::Take { id: 2, option: 1 }) else {
            panic!();
        };
        assert_eq!(feature, "trim");
        assert_eq!(change, DecisionChange::Resolve { id: 2, answer: "Only the binding ones".into(), because: None });
    }

    #[test]
    fn an_answer_in_words_resolves_it_and_only_an_open_one_takes_one() {
        let ClientMsg::ChangeDecision { change, .. } = one(FeatureEdit::Answer {
            id: 2,
            answer: " The last five ".into(),
            because: Some("  ".into()),
        }) else {
            panic!();
        };
        assert_eq!(change, DecisionChange::Resolve { id: 2, answer: "The last five".into(), because: None });
        assert!(send(FeatureEdit::Answer { id: 1, answer: "x".into(), because: None }).is_err(), "D1 is made");
        assert!(send(FeatureEdit::Answer { id: 2, answer: " ".into(), because: None }).is_err());
    }

    #[test]
    fn a_rewritten_record_is_read_back_as_the_adr_it_was_edited_as() {
        let document = "Name the feature\n\n## Context\nThe hook prints the brief.\n\n## Why\nIt is long.\n";
        let ClientMsg::ChangeDecision { change: DecisionChange::Rewrite { id, text }, .. } =
            one(FeatureEdit::Rewrite { id: 1, document: document.into() })
        else {
            panic!();
        };
        assert_eq!(id, 1);
        assert_eq!((text.context.as_deref(), text.because.as_deref()), (Some("The hook prints the brief."), Some("It is long.")));
        assert!(send(FeatureEdit::Rewrite { id: 1, document: "\n\n".into() }).is_err(), "nothing chosen");
    }

    #[test]
    fn a_decision_is_withdrawn_or_restored_never_removed() {
        let withdraw = one(FeatureEdit::Withdraw { id: 1, withdrawn: true });
        assert!(matches!(withdraw, ClientMsg::ChangeDecision { change: DecisionChange::Withdraw { id: 1 }, .. }));
        let restore = one(FeatureEdit::Withdraw { id: 1, withdrawn: false });
        assert!(matches!(restore, ClientMsg::ChangeDecision { change: DecisionChange::Restore { id: 1 }, .. }));
    }

    #[test]
    fn a_typed_decision_line_asks_a_question_or_records_one_made() {
        let ClientMsg::Decide { write, .. } =
            one(FeatureEdit::AddDecision { under: None, task: Some(4), line: "? Which route".into() })
        else {
            panic!();
        };
        assert_eq!((write.chose.as_str(), write.state, write.task), ("Which route", DecisionState::Open, Some(4)));
        let ClientMsg::Decide { write, .. } =
            one(FeatureEdit::AddDecision { under: Some(1), task: None, line: "The pane API".into() })
        else {
            panic!();
        };
        assert_eq!((write.state, write.under), (DecisionState::Accepted, Some(1)));
    }

    #[test]
    fn a_task_edit_sends_only_what_changed() {
        let same = send(FeatureEdit::Task { id: 1, title: "Serve the brief on demand".into(), body: String::new() }).unwrap();
        assert!(same.is_empty());
        let both = send(FeatureEdit::Task { id: 1, title: " Serve it ".into(), body: "Over the pane API.".into() }).unwrap();
        let actions: Vec<_> = both
            .iter()
            .map(|m| match m {
                ClientMsg::Task { action, .. } => action.clone(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            actions,
            [
                TaskAction::Retitle { id: 1, title: "Serve it".into() },
                TaskAction::SetBody { id: 1, body: "Over the pane API.".into() },
            ]
        );
        assert!(send(FeatureEdit::Task { id: 1, title: "  ".into(), body: String::new() }).is_err());
    }

    #[test]
    fn tasks_subtasks_and_criteria_are_added_by_id() {
        let ClientMsg::Task { action, .. } = one(FeatureEdit::AddTask { parent: Some(1), title: "Wire the hook".into() }) else {
            panic!();
        };
        assert_eq!(action, TaskAction::Add(TaskWrite { title: "Wire the hook".into(), external: None, parent: Some(1) }));
        let ClientMsg::Task { action, .. } = one(FeatureEdit::AddCriterion { id: 3, text: " A test covers it ".into() }) else {
            panic!();
        };
        assert_eq!(action, TaskAction::AddCriterion { id: 3, text: "A test covers it".into() });
        assert!(send(FeatureEdit::AddCriterion { id: 3, text: String::new() }).unwrap().is_empty());
    }

    #[test]
    fn a_new_row_saved_blank_sends_nothing() {
        let blank = || "  ".to_string();
        let added = [
            FeatureEdit::AddCriterion { id: 3, text: blank() },
            FeatureEdit::AddTask { parent: None, title: blank() },
            FeatureEdit::AddDecision { under: None, task: None, line: blank() },
            FeatureEdit::Supersede { id: 1, line: blank() },
            FeatureEdit::AddFlow { title: blank() },
        ];
        for edit in added {
            assert_eq!(send(edit).map(|out| out.len()), Ok(0));
        }
        assert!(send(FeatureEdit::Answer { id: 2, answer: blank(), because: None }).is_err(), "an existing row's blank is refused");
    }

    #[test]
    fn an_unchanged_brief_sends_nothing_and_a_changed_one_replaces_it() {
        assert!(send(FeatureEdit::Brief { body: "Why Trim context injection matters.".into() }).unwrap().is_empty());
        let ClientMsg::Feature { action, .. } = one(FeatureEdit::Brief { body: "Name it, don't print it.".into() }) else {
            panic!();
        };
        assert_eq!(action, FeatureAction::Rewrite { slug: "trim".into(), body: "Name it, don't print it.".into() });
    }
}
