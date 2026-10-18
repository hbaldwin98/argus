//! Requests about a feature board: features and what they hold — tasks,
//! decisions, diagrams — and the tracker map and reconcile around them.

use argus_protocol::{
    Capture, CapturedDecision, CapturedTask, ClientMsg, Consequence, DecisionChange, DecisionState,
    DecisionText, DecisionWrite, DiagramAction, DiagramWrite, FeatureAction, FeatureWrite,
    LinkWrite, MapChange, ReconcileAction, TaskAction, TaskPlace, TaskState, TaskWrite, Ticked,
};
use proptest::prelude::*;

use super::values::{checkout, maybe_text, pane, project, request_id, row, slug, text};
use super::Known;

fn on_board(k: &Known) -> impl Strategy<Value = (argus_protocol::ProjectId, argus_protocol::CheckoutId, String)> {
    (project(k), checkout(k), slug(k))
}

fn state() -> BoxedStrategy<DecisionState> {
    prop_oneof![Just(DecisionState::Accepted), Just(DecisionState::Open), Just(DecisionState::Rejected)].boxed()
}

fn consequences() -> BoxedStrategy<Vec<Consequence>> {
    proptest::collection::vec((any::<bool>(), text()).prop_map(|(gain, text)| Consequence { gain, text }), 0..3).boxed()
}

fn options() -> BoxedStrategy<Vec<String>> {
    proptest::collection::vec(text(), 0..3).boxed()
}

fn capture() -> BoxedStrategy<Capture> {
    let decision = (text(), any::<bool>(), options(), maybe_text())
        .prop_map(|(chose, open, options, because)| CapturedDecision { chose, open, options, because });
    let task = (text(), proptest::collection::vec(decision.clone(), 0..2))
        .prop_map(|(title, decisions)| CapturedTask { title, subtasks: Vec::new(), decisions });
    (text(), maybe_text(), proptest::collection::vec(task, 0..3), proptest::collection::vec(decision, 0..2))
        .prop_map(|(title, body, tasks, decisions)| Capture { title, body, tasks, decisions, unread: Vec::new() })
        .boxed()
}

fn feature_action(k: &Known) -> BoxedStrategy<FeatureAction> {
    let slugs = slug(k);
    let link = (text(), maybe_text(), maybe_text(), maybe_text())
        .prop_map(|(url, tracker, key, role)| LinkWrite { url, tracker, key, role });
    prop_oneof![
        (text(), maybe_text()).prop_map(|(title, body)| FeatureAction::Open(FeatureWrite { title, body })),
        slugs.clone().prop_map(|slug| FeatureAction::Select { slug }),
        text().prop_map(|text| FeatureAction::Append { text }),
        (slugs.clone(), maybe_text()).prop_map(|(slug, detail)| FeatureAction::Done { slug, detail }),
        (slugs.clone(), maybe_text()).prop_map(|(slug, detail)| FeatureAction::Reopen { slug, detail }),
        (slugs.clone(), text()).prop_map(|(slug, title)| FeatureAction::Retitle { slug, title }),
        (slugs.clone(), text()).prop_map(|(slug, body)| FeatureAction::Rewrite { slug, body }),
        slugs.clone().prop_map(|slug| FeatureAction::Drop { slug }),
        (slugs.clone(), text()).prop_map(|(slug, reason)| FeatureAction::Hold { slug, reason }),
        slugs.clone().prop_map(|slug| FeatureAction::Unhold { slug }),
        (slugs.clone(), text()).prop_map(|(slug, on)| FeatureAction::Wait { slug, on }),
        (slugs.clone(), text()).prop_map(|(slug, on)| FeatureAction::Unwait { slug, on }),
        (slugs.clone(), link).prop_map(|(slug, link)| FeatureAction::Link { slug, link }),
        (slugs.clone(), text()).prop_map(|(slug, key)| FeatureAction::Unlink { slug, key }),
        (slugs, checkout(k)).prop_map(|(slug, to)| FeatureAction::Transfer { slug, to }),
        (proptest::option::of(slug(k)), capture())
            .prop_map(|(into, capture)| FeatureAction::Capture { into, capture: Box::new(capture) }),
    ]
    .boxed()
}

fn task_action(k: &Known) -> BoxedStrategy<TaskAction> {
    let id = row(k.task);
    let task_state = prop_oneof![Just(TaskState::Todo), Just(TaskState::Doing), Just(TaskState::Done)];
    let place = prop_oneof![
        row(k.task).prop_map(TaskPlace::Under),
        Just(TaskPlace::Top),
        row(k.task).prop_map(TaskPlace::Before),
        row(k.task).prop_map(TaskPlace::After),
        slug(k).prop_map(TaskPlace::Feature),
    ];
    let index = prop_oneof![0..3usize, any::<usize>()];
    prop_oneof![
        Just(TaskAction::List),
        (text(), maybe_text(), proptest::option::of(row(k.task)))
            .prop_map(|(title, external, parent)| TaskAction::Add(TaskWrite { title, external, parent })),
        (id.clone(), task_state).prop_map(|(id, state)| TaskAction::Move { id, state }),
        (id.clone(), text()).prop_map(|(id, title)| TaskAction::Retitle { id, title }),
        (id.clone(), text()).prop_map(|(id, body)| TaskAction::SetBody { id, body }),
        id.clone().prop_map(|id| TaskAction::Remove { id }),
        (id.clone(), row(k.task)).prop_map(|(id, to)| TaskAction::Reorder { id, to }),
        (id.clone(), place).prop_map(|(id, place)| TaskAction::Place { id, place }),
        (id.clone(), text()).prop_map(|(id, text)| TaskAction::AddCriterion { id, text }),
        (id.clone(), index.clone(), any::<bool>()).prop_map(|(id, index, done)| TaskAction::SetCriterion { id, index, done }),
        (id.clone(), index).prop_map(|(id, index)| TaskAction::RemoveCriterion { id, index }),
        (id, text()).prop_map(|(id, text)| TaskAction::Note { id, text }),
    ]
    .boxed()
}

fn decision_write(k: &Known) -> BoxedStrategy<DecisionWrite> {
    let parents = (proptest::option::of(row(k.decision)), proptest::option::of(row(k.decision)), proptest::option::of(row(k.task)));
    let prose = (maybe_text(), maybe_text(), maybe_text(), maybe_text());
    (text(), state(), options(), consequences(), parents, prose)
        .prop_map(|(chose, state, options, consequences, (under, supersedes, task), (over, because, context, detail))| DecisionWrite {
            chose,
            over,
            because,
            under,
            supersedes,
            state,
            task,
            context,
            detail,
            consequences,
            options,
        })
        .boxed()
}

fn decision_change(k: &Known) -> BoxedStrategy<DecisionChange> {
    let id = row(k.decision);
    let text_of = (text(), maybe_text(), maybe_text(), maybe_text(), maybe_text(), consequences(), options())
        .prop_map(|(chose, context, detail, because, over, consequences, options)| DecisionText {
            chose,
            context,
            detail,
            because,
            over,
            consequences,
            options,
        });
    prop_oneof![
        id.clone().prop_map(|id| DecisionChange::Withdraw { id }),
        id.clone().prop_map(|id| DecisionChange::Restore { id }),
        (id.clone(), text(), maybe_text()).prop_map(|(id, answer, because)| DecisionChange::Resolve { id, answer, because }),
        (id, text_of).prop_map(|(id, text)| DecisionChange::Rewrite { id, text }),
    ]
    .boxed()
}

pub fn get_decisions(k: &Known) -> BoxedStrategy<ClientMsg> {
    (project(k), checkout(k))
        .prop_map(|(project, checkout)| ClientMsg::GetDecisions { project, checkout })
        .boxed()
}

pub fn feature(k: &Known) -> BoxedStrategy<ClientMsg> {
    (project(k), checkout(k), feature_action(k), request_id())
        .prop_map(|(project, checkout, action, request_id)| ClientMsg::Feature { project, checkout, action, request_id })
        .boxed()
}

pub fn tracker_map(k: &Known) -> BoxedStrategy<ClientMsg> {
    let change = prop_oneof![text().prop_map(MapChange::Write), text().prop_map(MapChange::Approve), Just(MapChange::Discard)];
    (project(k), checkout(k), change)
        .prop_map(|(project, checkout, change)| ClientMsg::TrackerMap { project, checkout, change })
        .boxed()
}

pub fn reconcile(k: &Known) -> BoxedStrategy<ClientMsg> {
    let ticked = proptest::collection::vec((0..8usize, text()).prop_map(|(line, text)| Ticked { line, text }), 0..3);
    let action = prop_oneof![
        (pane(k), text()).prop_map(|(pane, ask)| ReconcileAction::Ask { pane, ask }),
        ticked.prop_map(|ticked| ReconcileAction::CarryOut { ticked }),
        Just(ReconcileAction::Discard),
    ];
    (on_board(k), action)
        .prop_map(|((project, checkout, slug), action)| ClientMsg::Reconcile { project, checkout, slug, action })
        .boxed()
}

pub fn task(k: &Known) -> BoxedStrategy<ClientMsg> {
    (on_board(k), task_action(k))
        .prop_map(|((project, checkout, feature), action)| ClientMsg::Task { project, checkout, feature, action })
        .boxed()
}

pub fn decide(k: &Known) -> BoxedStrategy<ClientMsg> {
    (on_board(k), decision_write(k))
        .prop_map(|((project, checkout, feature), write)| ClientMsg::Decide { project, checkout, feature, write })
        .boxed()
}

pub fn change_decision(k: &Known) -> BoxedStrategy<ClientMsg> {
    (on_board(k), decision_change(k))
        .prop_map(|((project, checkout, feature), change)| ClientMsg::ChangeDecision { project, checkout, feature, change })
        .boxed()
}

pub fn sequence_diagram(k: &Known) -> BoxedStrategy<ClientMsg> {
    let write = (text(), prop_oneof![Just("sequenceDiagram\n  A->>B: hi".to_owned()), text()])
        .prop_map(|(title, body)| DiagramWrite { title, body });
    let action = prop_oneof![
        Just(DiagramAction::List),
        write.clone().prop_map(DiagramAction::Add),
        (row(1), write).prop_map(|(id, write)| DiagramAction::Rewrite { id, write }),
        row(1).prop_map(|id| DiagramAction::Remove { id }),
    ];
    (on_board(k), action)
        .prop_map(|((project, checkout, feature), action)| ClientMsg::SequenceDiagram { project, checkout, feature, action })
        .boxed()
}
