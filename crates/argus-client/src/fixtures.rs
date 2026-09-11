//! Tree builders every test module shares, so a field added to the wire
//! tree is filled in here rather than in every test that builds one.

use argus_protocol::{
    CheckoutId, CheckoutInfo, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId, ProjectInfo,
    RepositoryId, RepositoryInfo,
};

pub(crate) fn pane_info(id: u64, kind: PaneKind, title: &str, status: PaneStatus) -> PaneInfo {
    PaneInfo {
        id: PaneId(id),
        kind,
        title: title.to_string(),
        status,
        note: None,
        template: None,
        children: Vec::new(),
    }
}

/// An editor pane, which the panel never lists among the panes.
pub(crate) fn editor(id: u64, title: &str) -> PaneInfo {
    pane_info(id, PaneKind::Editor, title, PaneStatus::Idle)
}

pub(crate) fn checkout(id: u64, name: &str, primary: bool, panes: Vec<PaneInfo>) -> CheckoutInfo {
    CheckoutInfo {
        id: CheckoutId(id),
        name: name.to_string(),
        path: format!("/repo/{name}"),
        primary,
        git: None,
        panes,
    }
}

pub(crate) fn repository(id: u64, name: &str, checkouts: Vec<CheckoutInfo>) -> RepositoryInfo {
    RepositoryInfo {
        id: RepositoryId(id),
        name: name.to_string(),
        checkouts,
        branches: Vec::new(),
        default_branch: None,
        remote_branches: Vec::new(),
    }
}

pub(crate) fn project(id: u64, name: &str, repositories: Vec<RepositoryInfo>) -> ProjectInfo {
    ProjectInfo {
        id: ProjectId(id),
        name: name.to_string(),
        repositories,
    }
}
