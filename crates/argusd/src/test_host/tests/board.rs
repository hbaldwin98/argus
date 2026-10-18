//! A feature worked through on its board from a client: opened, its tasks
//! added and moved, its decisions made, asked, answered and read back.

use argus_protocol::{
    ClientMsg, DecisionChange, DecisionState, DecisionWrite, FeatureAction, FeatureWrite,
    ServerMsg, TaskAction, TaskState, TaskWrite,
};

use crate::test_host::transcript::Transcript;
use crate::test_host::Host;

/// The slug the board files "Bound the queue" under.
const FEATURE: &str = "bound-the-queue";

#[tokio::test]
async fn a_feature_worked_through() {
    let host = Host::start().await;
    let (project, checkout) = (host.project(), host.checkout());
    let mut t = Transcript::greeted(&host).await;
    let decisions = |m: &ServerMsg| matches!(m, ServerMsg::Decisions(_));
    let tasks = |m: &ServerMsg| matches!(m, ServerMsg::Tasks(_));

    let open = |title: &str, body: Option<&str>| ClientMsg::Feature {
        project,
        checkout,
        action: FeatureAction::Open(FeatureWrite { title: title.into(), body: body.map(Into::into) }),
        request_id: 1,
    };
    t.ask(open("Bound the queue", Some("Refuse when full.")), decisions).await;
    // A second feature by the same title is opened beside the first.
    t.ask(open("Bound the queue", None), decisions).await;

    let task = |action| ClientMsg::Task { project, checkout, feature: FEATURE.into(), action };
    let add = TaskAction::Add(TaskWrite { title: "Measure the backlog".into(), external: None, parent: None });
    t.ask(task(add), tasks).await;
    t.ask(task(TaskAction::Move { id: 1, state: TaskState::Doing }), tasks).await;
    t.ask(task(TaskAction::List), tasks).await;

    let decide = |write| ClientMsg::Decide { project, checkout, feature: FEATURE.into(), write };
    let made = DecisionWrite {
        chose: "refuse rather than drop".into(),
        because: Some("a dropped write is silent".into()),
        task: Some(1),
        ..Default::default()
    };
    t.ask(decide(made), decisions).await;
    let asked = DecisionWrite { chose: "which bound".into(), state: DecisionState::Open, ..Default::default() };
    t.ask(decide(asked), decisions).await;

    let resolve = |answer: &str| ClientMsg::ChangeDecision {
        project,
        checkout,
        feature: FEATURE.into(),
        change: DecisionChange::Resolve { id: 2, answer: answer.into(), because: None },
    };
    t.ask(resolve("a count"), decisions).await;
    // Answering it again is refused: it is no longer open.
    t.ask(resolve("a byte size"), |m| matches!(m, ServerMsg::Error { .. })).await;

    t.ask(ClientMsg::GetDecisions { project, checkout }, decisions).await;
    insta::assert_snapshot!(t.finish());
    host.stop().await;
}
