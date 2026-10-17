//! Each declared call's body and reply, written and read back through its forms.

use super::*;
use crate::{
    AgentLaunch, AgentLaunched, AgentTelemetry, Decision, DecisionBoard, DecisionChange, DecisionWrite,
    DiagramAction, DiagramList, Feature, FeatureAction, FeatureBoard, PaneId, ReconcileChange,
    RepositoryId, SavedViewChange, TaskAction, TaskList, TaskState, ViewChange,
};

fn reads_back<C: Call>(body: C::Body, reply: C::Reply)
where
    C::Body: PartialEq + std::fmt::Debug,
    C::Reply: PartialEq + std::fmt::Debug,
{
    let written = C::BodyForm::write(&body).unwrap();
    assert_eq!(C::BodyForm::read(&written).unwrap(), body, "{:?}'s body", C::ENDPOINT);
    let written = C::ReplyForm::write(&reply).unwrap();
    assert_eq!(C::ReplyForm::read(&written).unwrap(), reply, "{:?}'s reply", C::ENDPOINT);
}

fn tasks() -> TaskList {
    TaskList {
        project_name: "argus".into(),
        feature: Some("pushes".into()),
        tasks: Vec::new(),
        repositories: vec![RepositoryId(4)],
    }
}

fn board() -> FeatureBoard {
    FeatureBoard {
        project: None,
        project_name: "argus".into(),
        features: vec![Feature { slug: "pushes".into(), ..Default::default() }],
        current: Some("pushes".into()),
        decisions: vec![Decision { id: 2, ..Default::default() }],
        unfiled: 1,
    }
}

#[test]
fn every_json_call_reads_back_what_it_wrote() {
    reads_back::<Checkout>("/work/ünïcode".into(), ());
    reads_back::<Comments>((), Vec::new());
    reads_back::<Decisions>((), DecisionBoard::default());
    reads_back::<Decide>(
        DecisionWrite { chose: "one channel".into(), ..Default::default() },
        Decision { id: 7, ..Default::default() },
    );
    reads_back::<ChangeDecision>(DecisionChange::Withdraw { id: 7 }, DecisionBoard::default());
    reads_back::<Features>((), board());
    reads_back::<ChangeFeature>(FeatureAction::Select { slug: "pushes".into() }, board());
    reads_back::<Tasks>(TaskAction::Move { id: 3, state: TaskState::Doing }, tasks());
    reads_back::<Diagrams>(
        DiagramAction::Remove { id: 1 },
        DiagramList {
            project_name: "argus".into(),
            feature: None,
            diagrams: Vec::new(),
            repositories: vec![RepositoryId(4)],
        },
    );
    reads_back::<TrackerMap>("ENG -> linear".into(), crate::TrackerMap::default());
    reads_back::<Reconcile>(ReconcileChange::Read, Feature { slug: "pushes".into(), ..Default::default() });
    reads_back::<Telemetry>(AgentTelemetry::default(), ());
    reads_back::<ChangeView>(ViewChange::Close { id: "deps".into() }, ());
    reads_back::<SavedView>(SavedViewChange::Get { id: "deps".into() }, serde_json::json!({"id": "deps"}));
    reads_back::<Launch>(
        AgentLaunch::default(),
        AgentLaunched { pane: PaneId(3), template: "claude".into(), checkout: "/work".into() },
    );
}

#[test]
fn a_body_that_is_not_json_is_refused_in_serdes_words() {
    let why = <Json as Form<TaskAction>>::read(br#"{"Juggle":{}}"#).unwrap_err();
    assert!(why.contains("Juggle"), "{why}");
}

#[test]
fn text_is_read_as_sent_and_nothing_reads_anything() {
    let sent = "naïve — 日本".to_string();
    assert_eq!(Text::read(&Text::write(&sent).unwrap()).unwrap(), sent);
    assert_eq!(Empty::read(b"junk"), Ok(()));
    assert!(Empty::write(&()).unwrap().is_empty());
}

#[test]
fn no_two_calls_name_one_endpoint() {
    let all = [
        Checkout::ENDPOINT,
        Comments::ENDPOINT,
        Decisions::ENDPOINT,
        Decide::ENDPOINT,
        ChangeDecision::ENDPOINT,
        Features::ENDPOINT,
        ChangeFeature::ENDPOINT,
        Tasks::ENDPOINT,
        Diagrams::ENDPOINT,
        TrackerMap::ENDPOINT,
        Reconcile::ENDPOINT,
        Telemetry::ENDPOINT,
        ChangeView::ENDPOINT,
        SavedView::ENDPOINT,
        Launch::ENDPOINT,
    ];
    for (i, endpoint) in all.iter().enumerate() {
        assert!(!all[i + 1..].contains(endpoint), "{endpoint:?} is declared twice");
    }
}
