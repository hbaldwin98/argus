//! What the person at Argus is working on, and where: the view a client
//! reports, and the focus the daemon makes of it for anyone who asks.
//!
//! Selection is the client's, so the daemon only learns it by being told.
//! A client says which ids it is showing ([`View`]); the daemon reads them
//! against its own tree, so what a watcher is sent ([`Focus`]) names what
//! is true now rather than what the client last saw. The same record is
//! printed as JSON by `argus focus`, which makes its field names a
//! contract with scripts as well as with the other binaries.

use serde::{Deserialize, Serialize};

use crate::held_tree::HeldTree;
use crate::ids::{CheckoutId, PaneId, ProjectId, RepositoryId};
use crate::tree::{CheckoutInfo, PaneInfo, PaneKind, ProjectInfo};

/// What one client is showing, by the daemon's ids. Each is the selection
/// at its depth; a client on the project heading has no checkout.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub project: Option<ProjectId>,
    pub checkout: Option<CheckoutId>,
    pub pane: Option<PaneId>,
    /// Whether the person is at this client: its terminal has focus, or
    /// cannot say. Only an active view takes over from another client's;
    /// an inactive one keeps this client's record current without it, so
    /// a tree reshuffling a background client's selection does not move
    /// the focus.
    pub active: bool,
}

/// Where the person is working, as the daemon's tree has it. Every part is
/// `None` when the view names nothing there, or names what has since gone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Focus {
    /// The open workspace, by name.
    pub workspace: String,
    pub project: Option<FocusProject>,
    pub repository: Option<FocusRepository>,
    pub checkout: Option<FocusCheckout>,
    pub pane: Option<FocusPane>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusProject {
    pub id: ProjectId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusRepository {
    pub id: RepositoryId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusCheckout {
    pub id: CheckoutId,
    pub path: String,
    /// `None` for a detached HEAD, or a directory that is not a repository.
    pub branch: Option<String>,
    /// The repository's own working directory rather than a linked
    /// worktree.
    pub primary: bool,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FocusPane {
    pub id: PaneId,
    /// `shell`, `agent` or `editor`.
    pub kind: String,
    pub title: String,
    /// The agent template the pane runs, for an agent pane.
    pub agent: Option<String>,
    /// What the pane's row says: the pane API's word for its loudest state,
    /// so an agent idle at its prompt with a workflow running reads
    /// `working`, as it does on the rail.
    pub status: String,
    pub note: Option<String>,
    /// When the status last changed, in seconds since the Unix epoch.
    pub since: Option<u64>,
    pub model: Option<String>,
}

impl Focus {
    /// What `view` names in `tree`. A pane is placed by where it is now
    /// rather than by the checkout the view names beside it — an agent can
    /// move to another checkout, and the client's ids follow a tree later.
    pub fn of(view: &View, workspace: &str, tree: &[ProjectInfo]) -> Focus {
        let at_pane = view.pane.and_then(|id| tree.lineage_of_pane(id)).map(|(lineage, pane)| {
            (lineage.project, Some(lineage.repository), Some(lineage.checkout), Some(pane))
        });
        let at_checkout = || {
            let lineage = tree.lineage_of_checkout(view.checkout?)?;
            Some((lineage.project, Some(lineage.repository), Some(lineage.checkout), None))
        };
        let at_project = || {
            let id = view.project?;
            let project = tree.iter().find(|p| p.id == id)?;
            Some((project, None, None, None))
        };
        let found = at_pane.or_else(at_checkout).or_else(at_project);

        let Some((project, repository, checkout, pane)) = found else {
            return Focus {
                workspace: workspace.to_string(),
                project: None,
                repository: None,
                checkout: None,
                pane: None,
            };
        };
        Focus {
            workspace: workspace.to_string(),
            project: Some(FocusProject {
                id: project.id,
                name: project.name.clone(),
            }),
            repository: repository.map(|r| FocusRepository {
                id: r.id,
                name: r.name.clone(),
            }),
            checkout: checkout.map(focus_checkout),
            pane: pane.map(focus_pane),
        }
    }
}

fn focus_checkout(checkout: &CheckoutInfo) -> FocusCheckout {
    let git = checkout.git.as_ref();
    FocusCheckout {
        id: checkout.id,
        path: checkout.path.clone(),
        branch: git.and_then(|g| g.branch.clone()),
        primary: checkout.primary,
        dirty: git.is_some_and(|g| g.dirty),
    }
}

fn focus_pane(pane: &PaneInfo) -> FocusPane {
    let state = pane.loudest_state();
    let kind = match pane.kind {
        PaneKind::Shell => "shell",
        PaneKind::Agent => "agent",
        PaneKind::Editor => "editor",
    };
    FocusPane {
        id: pane.id,
        kind: kind.to_string(),
        title: pane.title.clone(),
        agent: pane.template.clone(),
        status: state.status.word().to_string(),
        note: state.note.map(str::to_string),
        since: pane.since,
        model: pane.telemetry.model.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{AgentTelemetry, GitStatus, PaneStatus, RepositoryInfo};

    fn pane(id: u64, status: PaneStatus) -> PaneInfo {
        PaneInfo {
            id: PaneId(id),
            kind: PaneKind::Agent,
            title: "fixing restore".into(),
            status,
            note: None,
            template: Some("claude".into()),
            children: Vec::new(),
            telemetry: AgentTelemetry {
                model: Some("opus".into()),
                ..Default::default()
            },
            background: false,
            since: Some(1_700_000_000),
            live: false,
        }
    }

    fn checkout(id: u64, branch: &str, panes: Vec<PaneInfo>) -> CheckoutInfo {
        CheckoutInfo {
            id: CheckoutId(id),
            name: branch.into(),
            path: format!("/src/argus/{branch}"),
            panes,
            git: Some(GitStatus {
                branch: Some(branch.into()),
                dirty: true,
                changed_files: 1,
                staged: 0,
                unstaged: 1,
                ahead: 0,
                behind: 0,
                upstream: None,
                last_commit: None,
            }),
            primary: branch == "main",
        }
    }

    /// One project, one repository, `main` with nothing running and
    /// `restore` with pane 7 in it.
    fn tree() -> Vec<ProjectInfo> {
        vec![ProjectInfo {
            id: ProjectId(1),
            name: "work".into(),
            root: None,
            repositories: vec![RepositoryInfo {
                id: RepositoryId(2),
                name: "argus".into(),
                checkouts: vec![
                    checkout(3, "main", Vec::new()),
                    checkout(4, "restore", vec![pane(7, PaneStatus::Waiting)]),
                ],
                branches: Vec::new(),
                default_branch: None,
                remote_branches: Vec::new(),
                unmerged: Default::default(),
                last_used: None,
            }],
        }]
    }

    #[test]
    fn a_pane_names_everything_above_it() {
        let view = View {
            pane: Some(PaneId(7)),
            ..View::default()
        };
        let focus = Focus::of(&view, "default", &tree());

        assert_eq!(focus.workspace, "default");
        assert_eq!(focus.project.unwrap().name, "work");
        assert_eq!(focus.repository.unwrap().name, "argus");
        let checkout = focus.checkout.unwrap();
        assert_eq!(checkout.branch.as_deref(), Some("restore"));
        assert!(!checkout.primary);
        assert!(checkout.dirty);
        let pane = focus.pane.unwrap();
        assert_eq!(pane.kind, "agent");
        assert_eq!(pane.agent.as_deref(), Some("claude"));
        assert_eq!(pane.status, "waiting");
        assert_eq!(pane.model.as_deref(), Some("opus"));
    }

    #[test]
    fn a_pane_is_placed_where_it_is_not_where_the_view_says() {
        // The agent moved to `restore`; the client's next view has not
        // caught up with the tree yet.
        let view = View {
            checkout: Some(CheckoutId(3)),
            pane: Some(PaneId(7)),
            ..View::default()
        };
        let focus = Focus::of(&view, "default", &tree());
        assert_eq!(focus.checkout.unwrap().id, CheckoutId(4));
    }

    #[test]
    fn a_checkout_with_nothing_selected_in_it_has_no_pane() {
        let view = View {
            checkout: Some(CheckoutId(3)),
            ..View::default()
        };
        let focus = Focus::of(&view, "default", &tree());
        assert_eq!(focus.checkout.unwrap().branch.as_deref(), Some("main"));
        assert!(focus.pane.is_none());
    }

    #[test]
    fn a_pane_that_has_gone_falls_back_to_its_checkout() {
        let view = View {
            checkout: Some(CheckoutId(4)),
            pane: Some(PaneId(99)),
            ..View::default()
        };
        let focus = Focus::of(&view, "default", &tree());
        assert_eq!(focus.checkout.unwrap().id, CheckoutId(4));
        assert!(focus.pane.is_none());
    }

    #[test]
    fn a_view_of_nothing_still_names_the_workspace() {
        let focus = Focus::of(&View::default(), "default", &tree());
        assert_eq!(focus.workspace, "default");
        assert!(focus.project.is_none() && focus.checkout.is_none());
    }

    #[test]
    fn an_agent_whose_turn_left_work_running_reads_working() {
        let mut tree = tree();
        let pane = &mut tree[0].repositories[0].checkouts[1].panes[0];
        pane.status = PaneStatus::Idle;
        pane.background = true;
        let view = View {
            pane: Some(PaneId(7)),
            ..View::default()
        };
        assert_eq!(Focus::of(&view, "default", &tree).pane.unwrap().status, "working");
    }

    #[test]
    fn the_json_scripts_read_names_its_fields_plainly() {
        // `argus focus` prints this; a renamed field breaks somebody's
        // status line.
        let view = View {
            pane: Some(PaneId(7)),
            ..View::default()
        };
        let json = serde_json::to_value(Focus::of(&view, "default", &tree())).unwrap();
        assert_eq!(json["workspace"], "default");
        assert_eq!(json["project"]["name"], "work");
        assert_eq!(json["repository"]["name"], "argus");
        assert_eq!(json["checkout"]["path"], "/src/argus/restore");
        assert_eq!(json["checkout"]["branch"], "restore");
        assert_eq!(json["pane"]["id"], 7);
        assert_eq!(json["pane"]["agent"], "claude");
        assert_eq!(json["pane"]["status"], "waiting");
        assert_eq!(json["pane"]["note"], serde_json::Value::Null);
    }
}
