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
