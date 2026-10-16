//! The builders every test module shares — the tree, and a pane's view as
//! the daemon's messages leave it — so a field added to the wire is filled
//! in here rather than in every test that builds one.

use argus_protocol::pane_view::PaneView;
use argus_protocol::{
    Cell, CheckoutId, CheckoutInfo, Cursor, MouseTracking, PaneId, PaneInfo, PaneKind, PaneStatus,
    ProjectId, ProjectInfo, RepositoryId, RepositoryInfo, ServerMsg,
};

/// What a snapshot of a pane says.
pub(crate) struct Shown {
    pub cells: Vec<Vec<Cell>>,
    pub cursor: Cursor,
    pub mouse: MouseTracking,
    pub alternate_screen: bool,
}

/// A view of `pane` showing `cells`, as the daemon's snapshot leaves it.
pub(crate) fn pane_view(pane: PaneId, cells: Vec<Vec<Cell>>) -> PaneView {
    let mut view = PaneView::new(pane);
    reshow(&mut view, |shown| shown.cells = cells);
    view
}

/// Changes what `view` shows the way the daemon would, with a snapshot:
/// whatever `change` leaves alone stays as it was.
pub(crate) fn reshow(view: &mut PaneView, change: impl FnOnce(&mut Shown)) {
    let mut shown = Shown {
        cells: view.live_rows().to_vec(),
        cursor: view.cursor(),
        mouse: view.mouse(),
        alternate_screen: view.alternate_screen(),
    };
    change(&mut shown);
    let _ = view.receive(&ServerMsg::PaneSnapshot {
        pane: view.pane(),
        rows: shown.cells.len() as u16,
        cols: shown.cells.first().map_or(0, Vec::len) as u16,
        cells: shown.cells,
        cursor: shown.cursor,
        mouse: shown.mouse,
        alternate_screen: shown.alternate_screen,
        keyboard: Default::default(),
    });
}

pub(crate) fn pane_info(id: u64, kind: PaneKind, title: &str, status: PaneStatus) -> PaneInfo {
    PaneInfo {
        id: PaneId(id),
        kind,
        title: title.to_string(),
        status,
        note: None,
        template: None,
        children: Vec::new(),
        telemetry: Default::default(),
        background: false,
        since: None,
        live: false,
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
        unmerged: Default::default(),
        last_used: None,
    }
}

pub(crate) fn project(id: u64, name: &str, repositories: Vec<RepositoryInfo>) -> ProjectInfo {
    ProjectInfo {
        id: ProjectId(id),
        name: name.to_string(),
        root: None,
        repositories,
    }
}
