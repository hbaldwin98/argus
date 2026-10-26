//! The tree builders the test modules share, and the commits and diffs
//! the daemon answers about it.

use argus_protocol::{
    AgentTelemetry, ChangeKind, CheckoutId, CheckoutInfo, CommitFile, CommitInfo, DiffLine,
    FileDiff, GitStatus, Hunk, LineKind, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId,
    ProjectInfo, RepositoryId, RepositoryInfo, Review, ReviewBase,
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
            changes: 0,
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
                links: vec![TrackerLink { tracker: "Linear".into(), key: "ENG-88".into(), url: "https://linear.app/x/ENG-88".into(), role: "the work".into() }],
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

/// [`board`] with `trim` opened up: its open question raised under task 4
/// and its made decision under task 1, the question offering two answers
/// and the decision costing a request.
pub fn open_board() -> argus_protocol::DecisionBoard {
    use argus_protocol::{Consequence, DecisionState};
    let mut board = board();
    for decision in &mut board.decisions {
        decision.task = Some(if decision.state == DecisionState::Open { 4 } else { 1 });
    }
    board.decisions[1].options = vec!["Every standing decision".into(), "Only the binding ones".into()];
    board.decisions[0].consequences = vec![Consequence { gain: false, text: "one more request".into() }];
    board
}

/// [`tasks`] of `trim`, task 3 with one criterion checked off and a note
/// an agent left.
pub fn open_tasks() -> argus_protocol::TaskList {
    use argus_protocol::{Criterion, TaskNote};
    let mut tasks = tasks("trim");
    tasks.tasks[2].criteria = vec![
        Criterion { text: "The route answers".into(), done: true },
        Criterion { text: "A test covers it".into(), done: false },
    ];
    tasks.tasks[2].notes = vec![TaskNote { text: "wiring it now".into(), session: Some("s".into()), ..TaskNote::default() }];
    tasks
}

/// The source of `trim`'s first flow: the hook asks the daemon for a
/// brief, which answers it or refuses, its second step bound to decision 1.
pub const FLOW: &str = "sequenceDiagram
    participant agent
    participant hook as argus-hook
    agent->>hook: ask for the brief
    hook->>argusd: brief request
    %% decision #1
    argusd->>argusd: find the pane's feature
    alt the pane has a feature
        argusd-->>hook: the brief
    else it has none
        argusd-->>hook: refused
    end";

/// A feature's flows: [`FLOW`], and a short second.
pub fn diagrams(feature: &str) -> argus_protocol::DiagramList {
    use argus_protocol::SequenceDiagram;
    let diagram = |id: i64, title: &str, body: &str| SequenceDiagram {
        id,
        feature: feature.into(),
        title: title.into(),
        body: body.into(),
        at: id,
        session: None,
    };
    argus_protocol::DiagramList {
        repositories: Vec::new(),
        project_name: "src".into(),
        feature: Some(feature.into()),
        diagrams: vec![diagram(11, "Asking for the brief", FLOW), diagram(12, "Capping", "sequenceDiagram\n    a->>b: cap")],
    }
}

/// A commit as a history lists it.
pub fn commit(oid: &str, summary: &str) -> CommitInfo {
    CommitInfo { oid: oid.into(), short: oid[..4].into(), summary: summary.into(), author: "t".into(), time: 0 }
}

/// A file a commit changed by one added line.
pub fn commit_file(path: &str) -> CommitFile {
    CommitFile { path: path.into(), old_path: None, kind: ChangeKind::Modified, added: 1, removed: 0 }
}

/// A commit's diff on checkout 100, answering `request_id`, that added a
/// line to `path`.
pub fn diff_of(request_id: u64, path: &str) -> Review {
    Review {
        request_id,
        checkout: CheckoutId(100),
        base: ReviewBase::Commit,
        files: vec![FileDiff {
            path: path.into(),
            old_path: None,
            kind: ChangeKind::Modified,
            hunks: vec![Hunk {
                header: "@@ -1 +1,2 @@".into(),
                lines: vec![DiffLine { kind: LineKind::Added, old_lineno: None, new_lineno: Some(2), text: "more".into(), spans: Vec::new() }],
            }],
            note: None,
            new_blob: None,
            new_lines: None,
            pending: false,
        }],
        commit: None,
    }
}
