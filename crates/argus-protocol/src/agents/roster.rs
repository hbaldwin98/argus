//! Which agents Argus is running and where each one is, as an agent asks
//! for them: the pane id is what a view's `pane` value links by, and an
//! agent has no other way to learn another's, least of all in another
//! project.

use serde::{Deserialize, Serialize};

use crate::{PaneId, PaneKind, PaneStatus, ProjectInfo};

/// One agent pane, with the project, repository and checkout holding it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentListing {
    pub pane: PaneId,
    pub title: String,
    #[serde(default)]
    pub template: Option<String>,
    pub status: PaneStatus,
    #[serde(default)]
    pub note: Option<String>,
    pub project: String,
    pub repository: String,
    pub checkout: String,
    /// In the same project as the agent that asked.
    pub here: bool,
}

/// Every agent pane in `tree` but `asker` itself, those of its own project
/// first, each half in tree order, so the agents an asker most likely
/// means lead the list.
pub fn roster(tree: &[ProjectInfo], asker: PaneId) -> Vec<AgentListing> {
    let home = tree.iter().position(|project| {
        project
            .repositories
            .iter()
            .flat_map(|r| &r.checkouts)
            .any(|c| c.panes.iter().any(|p| p.id == asker))
    });
    let mut all = Vec::new();
    for (index, project) in tree.iter().enumerate() {
        for repository in &project.repositories {
            for checkout in &repository.checkouts {
                for pane in &checkout.panes {
                    if pane.kind != PaneKind::Agent || pane.id == asker {
                        continue;
                    }
                    all.push(AgentListing {
                        pane: pane.id,
                        title: pane.title.clone(),
                        template: pane.template.clone(),
                        status: pane.status,
                        note: pane.note.clone(),
                        project: project.name.clone(),
                        repository: repository.name.clone(),
                        checkout: checkout.name.clone(),
                        here: Some(index) == home,
                    });
                }
            }
        }
    }
    // Stable, so each half keeps the tree's order.
    all.sort_by_key(|agent| !agent.here);
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CheckoutId, CheckoutInfo, PaneInfo, ProjectId, RepositoryId, RepositoryInfo};

    fn pane(id: u64, kind: PaneKind, title: &str) -> PaneInfo {
        PaneInfo {
            id: PaneId(id),
            kind,
            title: title.into(),
            status: PaneStatus::Working,
            note: None,
            template: Some("claude".into()),
            children: Vec::new(),
            telemetry: Default::default(),
            background: false,
            since: None,
            live: false,
        }
    }

    fn project(id: u64, name: &str, panes: Vec<PaneInfo>) -> ProjectInfo {
        ProjectInfo {
            id: ProjectId(id),
            name: name.into(),
            root: None,
            repositories: vec![RepositoryInfo {
                id: RepositoryId(id),
                name: format!("{name}-repo"),
                checkouts: vec![CheckoutInfo {
                    id: CheckoutId(id),
                    name: "main".into(),
                    path: format!("/src/{name}"),
                    panes,
                    git: None,
                    primary: true,
                }],
                branches: Vec::new(),
                default_branch: None,
                remote_branches: Vec::new(),
                unmerged: Default::default(),
                last_used: None,
            }],
        }
    }

    // The case that asked for it: a view in one project naming an agent
    // working in another, which it could not link.
    #[test]
    fn lists_agents_of_every_project_with_the_askers_own_first() {
        let tree = vec![
            project(1, "argus", vec![pane(7, PaneKind::Agent, "argus-s2-ba")]),
            project(2, "reliab", vec![
                pane(3, PaneKind::Agent, "asker"),
                pane(4, PaneKind::Shell, "zsh"),
                pane(5, PaneKind::Agent, "reliab-adapter"),
            ]),
        ];
        let listed = roster(&tree, PaneId(3));
        let names: Vec<_> = listed.iter().map(|a| (a.pane.0, a.title.as_str(), a.here)).collect();
        assert_eq!(names, vec![(5, "reliab-adapter", true), (7, "argus-s2-ba", false)]);
        assert_eq!(listed[1].project, "argus");
        assert_eq!(listed[1].repository, "argus-repo");
        assert_eq!(listed[1].checkout, "main");
    }

    #[test]
    fn an_asker_the_tree_does_not_hold_is_at_home_nowhere() {
        let tree = vec![project(1, "argus", vec![pane(7, PaneKind::Agent, "a")])];
        let listed = roster(&tree, PaneId(99));
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].here);
    }
}
