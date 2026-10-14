//! The tree builders the test modules share.

use argus_protocol::{
    AgentTelemetry, CheckoutId, CheckoutInfo, GitStatus, PaneId, PaneInfo, PaneKind, PaneStatus,
    ProjectId, ProjectInfo, RepositoryId, RepositoryInfo,
};

pub fn project(id: u64, name: &str, repositories: Vec<RepositoryInfo>) -> ProjectInfo {
    ProjectInfo { id: ProjectId(id), name: name.into(), root: None, repositories }
}

pub fn repository(id: u64, name: &str, last_used: Option<u64>, checkouts: Vec<CheckoutInfo>) -> RepositoryInfo {
    RepositoryInfo {
        id: RepositoryId(id),
        name: name.into(),
        checkouts,
        branches: Vec::new(),
        default_branch: None,
        remote_branches: Vec::new(),
        unmerged: Default::default(),
        last_used,
    }
}

pub fn checkout(id: u64, name: &str, primary: bool, panes: Vec<PaneInfo>) -> CheckoutInfo {
    CheckoutInfo {
        id: CheckoutId(id),
        name: name.into(),
        path: format!("/src/{name}"),
        panes,
        git: Some(GitStatus {
            branch: Some(name.into()),
            dirty: false,
            changed_files: 0,
            staged: 0,
            unstaged: 0,
            ahead: 0,
            behind: 0,
            upstream: None,
            last_commit: None,
        }),
        primary,
    }
}

pub fn pane(id: u64, kind: PaneKind, title: &str, status: PaneStatus) -> PaneInfo {
    PaneInfo {
        id: PaneId(id),
        kind,
        title: title.into(),
        status,
        note: None,
        template: None,
        children: Vec::new(),
        background: false,
        telemetry: AgentTelemetry::default(),
        since: None,
        live: false,
    }
}

/// A board for the repository in [`feature_tree`]: one feature in each
/// place, oldest first as the daemon lists them.
pub fn board() -> argus_protocol::DecisionBoard {
    use argus_protocol::{Feature, FeatureState, TaskCounts, TrackerLink};
    let feature = |number: i64, slug: &str, title: &str| Feature {
        number,
        slug: slug.into(),
        title: title.into(),
        body: format!("Why {title} matters."),
        at: number,
        ..Feature::default()
    };
    let counts = |todo, doing, done| TaskCounts { todo, doing, done };
    argus_protocol::DecisionBoard {
        repositories: Vec::new(),
        project: Some(ProjectId(1)),
        name: "src".into(),
        features: vec![
            Feature { state: FeatureState::Done, tasks: counts(0, 0, 2), ..feature(4, "old", "Old work") },
            Feature { held: Some("until the bump lands".into()), tasks: counts(2, 0, 0), ..feature(5, "mouse", "Mouse selection") },
            Feature { tasks: counts(0, 0, 4), ..feature(6, "ctrlc", "Ctrl-C on Windows") },
            Feature {
                tasks: counts(3, 1, 3),
                checkouts: vec!["/src/main".into()],
                links: vec![TrackerLink { tracker: "Linear".into(), key: "ENG-88".into(), url: "https://linear.app/x/ENG-88".into() }],
                ..feature(7, "trim", "Trim context injection")
            },
            Feature { waits_on: vec!["trim".into()], tasks: counts(5, 0, 0), ..feature(8, "bump", "The ratatui bump") },
            Feature { body: String::new(), ..feature(10, "desktop", "Desktop client") },
        ],
        decisions: vec![decision(1, "trim", argus_protocol::DecisionState::Accepted, "Name the feature"), decision(2, "trim", argus_protocol::DecisionState::Open, "How much history?")],
        tracker_map: Default::default(),
    }
}

fn decision(id: i64, feature: &str, state: argus_protocol::DecisionState, chose: &str) -> argus_protocol::Decision {
    argus_protocol::Decision {
        id,
        parent: None,
        at: id * 10,
        session: None,
        checkout: None,
        feature: Some(feature.into()),
        chose: chose.into(),
        over: None,
        because: None,
        superseded_by: None,
        withdrawn_at: None,
        state,
        task: None,
        context: None,
        detail: None,
        consequences: Vec::new(),
        options: Vec::new(),
    }
}

/// The tree [`board`] is read against: `trim` is worked on `main`, where a
/// working agent is.
pub fn feature_tree() -> Vec<ProjectInfo> {
    vec![project(1, "src", vec![repository(2, "argus", None, vec![checkout(
        3,
        "main",
        true,
        vec![pane(1, PaneKind::Agent, "claude", PaneStatus::Working)],
    )])])]
}

/// `trim`'s tasks: 1 under way with 1.1 done and 1.2 being done by the
/// agent, and 2 not started.
pub fn tasks(feature: &str) -> argus_protocol::TaskList {
    use argus_protocol::{Task, TaskState};
    let task = |id: i64, parent: Option<i64>, position: i64, state: TaskState, title: &str| Task {
        id,
        feature: feature.into(),
        parent,
        title: title.into(),
        body: None,
        state,
        claimed_by: None,
        external: None,
        position,
        at: id,
        session: None,
        claimed_pane: None,
        criteria: Vec::new(),
        notes: Vec::new(),
    };
    argus_protocol::TaskList {
        repositories: Vec::new(),
        project_name: "src".into(),
        feature: Some(feature.into()),
        tasks: vec![
            task(1, None, 0, TaskState::Doing, "Serve the brief on demand"),
            task(2, Some(1), 0, TaskState::Done, "Add a brief route"),
            Task { claimed_pane: Some(PaneId(1)), ..task(3, Some(1), 1, TaskState::Doing, "The hook reads the brief") },
            task(4, None, 1, TaskState::Todo, "Cap the history"),
        ],
    }
}
