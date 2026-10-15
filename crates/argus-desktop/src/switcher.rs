//! What the project switcher offers beyond the tree's projects — the
//! daemon's workspaces — and the project waiting on a yes to be removed.
//!
//! Both act only through the daemon: a workspace opened or a project
//! removed shows when the daemon's tree and workspaces say so, never
//! before, so a refusal leaves the switcher showing what is true.

use argus_protocol::{ClientMsg, ProjectId, ProjectInfo, WorkspaceId, WorkspaceInfo};

use crate::page::{Removing, WorkspaceChoice};

#[derive(Default)]
pub struct Switcher {
    workspaces: Vec<WorkspaceInfo>,
    removing: Option<ProjectId>,
}

impl Switcher {
    pub fn receive_workspaces(&mut self, workspaces: Vec<WorkspaceInfo>) {
        self.workspaces = workspaces;
    }

    /// Opens a workspace the daemon listed; one already open, or gone,
    /// asks nothing.
    pub fn open_workspace(&self, workspace: WorkspaceId) -> Option<ClientMsg> {
        self.workspaces
            .iter()
            .any(|w| w.id == workspace && !w.open)
            .then_some(ClientMsg::OpenWorkspace { workspace })
    }

    /// Asks before removing a project the tree holds.
    pub fn ask_remove(&mut self, project: ProjectId, tree: &[ProjectInfo]) {
        if tree.iter().any(|p| p.id == project) {
            self.removing = Some(project);
        }
    }

    /// The answer: a yes asks the daemon, which leaves the folders alone
    /// and refuses while any of the project's panes runs.
    pub fn answer(&mut self, yes: bool) -> Option<ClientMsg> {
        let project = self.removing.take()?;
        yes.then_some(ClientMsg::RemoveProject { project })
    }

    pub fn workspaces(&self) -> Vec<WorkspaceChoice> {
        self.workspaces
            .iter()
            .map(|w| WorkspaceChoice {
                id: w.id.0,
                name: w.name.clone(),
                open: w.open,
                projects: w.projects,
                panes: w.panes,
            })
            .collect()
    }

    /// The question as the page draws it, while its project is in the tree.
    pub fn removing(&self, tree: &[ProjectInfo]) -> Option<Removing> {
        let project = tree.iter().find(|p| Some(p.id) == self.removing)?;
        Some(Removing {
            project: project.id.0,
            name: project.name.clone(),
            repositories: project.repositories.len(),
            panes: project
                .repositories
                .iter()
                .flat_map(|r| &r.checkouts)
                .map(|c| c.panes.len())
                .sum(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane, project, repository};
    use argus_protocol::{PaneKind, PaneStatus};

    fn workspace(id: u64, name: &str, open: bool) -> WorkspaceInfo {
        WorkspaceInfo { id: WorkspaceId(id), name: name.into(), projects: 1, panes: 0, open }
    }

    #[test]
    fn only_a_workspace_not_open_is_asked_for() {
        let mut switcher = Switcher::default();
        switcher.receive_workspaces(vec![workspace(1, "personal", true), workspace(2, "work", false)]);
        assert!(switcher.open_workspace(WorkspaceId(1)).is_none(), "already open");
        assert!(switcher.open_workspace(WorkspaceId(9)).is_none(), "no such workspace");
        assert!(matches!(
            switcher.open_workspace(WorkspaceId(2)),
            Some(ClientMsg::OpenWorkspace { workspace: WorkspaceId(2) })
        ));
        assert_eq!(switcher.workspaces()[0].name, "personal");
    }

    #[test]
    fn removing_a_project_asks_first_and_only_a_yes_sends() {
        let tree = vec![project(1, "src", vec![repository(10, "argus", None, vec![checkout(
            100,
            "main",
            true,
            vec![pane(1, PaneKind::Shell, "zsh", PaneStatus::Idle)],
        )])])];
        let mut switcher = Switcher::default();
        switcher.ask_remove(ProjectId(9), &tree);
        assert!(switcher.removing(&tree).is_none(), "no such project");
        switcher.ask_remove(ProjectId(1), &tree);
        let asked = switcher.removing(&tree).unwrap();
        assert_eq!((asked.name.as_str(), asked.repositories, asked.panes), ("src", 1, 1));

        assert!(switcher.answer(false).is_none());
        assert!(switcher.removing(&tree).is_none(), "a no puts the question away");

        switcher.ask_remove(ProjectId(1), &tree);
        assert!(matches!(switcher.answer(true), Some(ClientMsg::RemoveProject { project: ProjectId(1) })));
        assert!(switcher.answer(true).is_none(), "asked once");
    }
}
