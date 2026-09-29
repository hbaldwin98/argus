//! A client's copy of the tree: every pane in it, an agent's telemetry
//! applied to it between trees, and which panes changed state from the
//! copy before.
//!
//! Here rather than in each client because the terminal and the web server
//! both hold a copy, and two readings of what changed would ring a bell for
//! a pane the phone was never told of.

use std::collections::HashMap;

use crate::ids::PaneId;
use crate::tree::{AgentTelemetry, PaneInfo, PaneState, PaneStatus, ProjectInfo, WorkspaceTree};

/// A tree a client holds: the open workspace's projects, or every
/// workspace's (`WIDE_TREE`).
pub trait HeldTree {
    /// Every pane, editors included: a pane is a pane to be found whether
    /// or not the checkout lists it.
    fn panes(&self) -> impl Iterator<Item = &PaneInfo>;

    fn panes_mut(&mut self) -> impl Iterator<Item = &mut PaneInfo>;

    /// Applies `ServerMsg::PaneTelemetry`. Returns whether the tree
    /// changed; a pane not in it yet is skipped, since the tree that brings
    /// it carries the same record.
    fn apply_telemetry(&mut self, pane: PaneId, telemetry: AgentTelemetry) -> bool {
        let Some(info) = self.panes_mut().find(|info| info.id == pane) else {
            return false;
        };
        if info.telemetry == telemetry {
            return false;
        }
        info.telemetry = telemetry;
        true
    }
}

impl HeldTree for [ProjectInfo] {
    fn panes(&self) -> impl Iterator<Item = &PaneInfo> {
        self.iter()
            .flat_map(|project| project.repositories.iter())
            .flat_map(|repository| repository.checkouts.iter())
            .flat_map(|checkout| checkout.panes.iter())
    }

    fn panes_mut(&mut self) -> impl Iterator<Item = &mut PaneInfo> {
        self.iter_mut()
            .flat_map(|project| project.repositories.iter_mut())
            .flat_map(|repository| repository.checkouts.iter_mut())
            .flat_map(|checkout| checkout.panes.iter_mut())
    }
}

impl HeldTree for [WorkspaceTree] {
    fn panes(&self) -> impl Iterator<Item = &PaneInfo> {
        self.iter().flat_map(|workspace| workspace.projects.panes())
    }

    fn panes_mut(&mut self) -> impl Iterator<Item = &mut PaneInfo> {
        self.iter_mut()
            .flat_map(|workspace| workspace.projects.panes_mut())
    }
}

/// A pane whose loudest state is not the one it had in the last tree.
#[derive(Debug, Clone, Copy)]
pub struct Transition<'a> {
    pub pane: &'a PaneInfo,
    pub before: PaneStatus,
    /// The loudest state now, with the child it came from and its note.
    pub after: PaneState<'a>,
}

/// The panes of `next` whose loudest state differs from `previous`'s, in
/// `next`'s order. Which of them are worth a bell or a push is each
/// client's own call.
///
/// A pane `previous` does not hold is not a transition: it is new, and
/// what it started as is not news. So the first tree a client holds,
/// compared with the empty one it started from, is quiet.
pub fn transitions<'a, T: HeldTree + ?Sized>(previous: &T, next: &'a T) -> Vec<Transition<'a>> {
    let before: HashMap<PaneId, PaneStatus> = previous
        .panes()
        .map(|pane| (pane.id, pane.loudest_state().status))
        .collect();
    next.panes()
        .filter_map(|pane| {
            let before = *before.get(&pane.id)?;
            let after = pane.loudest_state();
            (before != after.status).then_some(Transition { pane, before, after })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{CheckoutId, ProjectId, RepositoryId, WorkspaceId};
    use crate::tree::{CheckoutInfo, ChildAgentInfo, PaneKind, RepositoryInfo};

    fn pane(id: u64, status: PaneStatus) -> PaneInfo {
        PaneInfo {
            id: PaneId(id),
            kind: PaneKind::Agent,
            title: format!("agent {id}"),
            status,
            note: None,
            template: None,
            children: Vec::new(),
            telemetry: AgentTelemetry::default(),
            has_transcript: false,
            since: None,
            queued: Vec::new(),
            live: false,
        }
    }

    /// One project per group of panes, each in a checkout of its own.
    fn projects(groups: Vec<Vec<PaneInfo>>) -> Vec<ProjectInfo> {
        groups
            .into_iter()
            .enumerate()
            .map(|(i, panes)| ProjectInfo {
                id: ProjectId(i as u64),
                name: format!("p{i}"),
                root: None,
                repositories: vec![RepositoryInfo {
                    id: RepositoryId(i as u64),
                    name: format!("r{i}"),
                    branches: Vec::new(),
                    default_branch: None,
                    remote_branches: Vec::new(),
                    checkouts: vec![CheckoutInfo {
                        id: CheckoutId(i as u64),
                        name: "main".into(),
                        path: format!("/r{i}"),
                        panes,
                        git: None,
                        primary: true,
                    }],
                }],
            })
            .collect()
    }

    fn workspaces(groups: Vec<Vec<PaneInfo>>) -> Vec<WorkspaceTree> {
        groups
            .into_iter()
            .enumerate()
            .map(|(i, panes)| WorkspaceTree {
                id: WorkspaceId(i as u64),
                name: format!("w{i}"),
                open: i == 0,
                projects: projects(vec![panes]),
            })
            .collect()
    }

    fn ids(tree: &(impl HeldTree + ?Sized)) -> Vec<u64> {
        tree.panes().map(|pane| pane.id.0).collect()
    }

    #[test]
    fn every_pane_is_walked_in_every_workspace_and_project() {
        let mut editor = pane(3, PaneStatus::Idle);
        editor.kind = PaneKind::Editor;
        let tree = projects(vec![
            vec![pane(1, PaneStatus::Idle), pane(2, PaneStatus::Idle)],
            vec![editor],
        ]);
        assert_eq!(ids(tree.as_slice()), [1, 2, 3], "editors are panes too");

        let wide = workspaces(vec![vec![pane(1, PaneStatus::Idle)], vec![pane(4, PaneStatus::Idle)]]);
        assert_eq!(ids(wide.as_slice()), [1, 4]);
    }

    #[test]
    fn telemetry_lands_on_its_pane_and_says_whether_anything_changed() {
        let mut tree = workspaces(vec![vec![pane(1, PaneStatus::Working)], vec![pane(2, PaneStatus::Working)]]);
        let report = AgentTelemetry {
            model: Some("opus".into()),
            ..Default::default()
        };

        assert!(tree.apply_telemetry(PaneId(2), report.clone()));
        assert_eq!(tree.panes().nth(1).unwrap().telemetry, report);
        assert!(tree.panes().next().unwrap().telemetry.is_empty(), "only its own pane");

        assert!(!tree.apply_telemetry(PaneId(2), report.clone()), "the same report again");
        assert!(!tree.apply_telemetry(PaneId(9), report), "a pane no tree has brought yet");
    }

    #[test]
    fn the_first_tree_is_quiet() {
        let next = projects(vec![vec![pane(1, PaneStatus::Waiting)]]);
        assert!(transitions(&[][..], next.as_slice()).is_empty());
    }

    #[test]
    fn a_pane_new_to_this_tree_is_not_a_transition() {
        let previous = projects(vec![vec![pane(1, PaneStatus::Working)]]);
        let next = projects(vec![vec![pane(1, PaneStatus::Working), pane(2, PaneStatus::Waiting)]]);
        assert!(transitions(previous.as_slice(), next.as_slice()).is_empty());
    }

    #[test]
    fn a_pane_whose_loudest_state_changed_is_a_transition() {
        let previous = workspaces(vec![
            vec![pane(1, PaneStatus::Working), pane(2, PaneStatus::Working)],
            vec![pane(3, PaneStatus::Working)],
        ]);
        let mut child_waits = pane(1, PaneStatus::Working);
        child_waits.children.push(ChildAgentInfo {
            label: "reviewer".into(),
            status: PaneStatus::Waiting,
            note: Some("approve rm".into()),
        });
        let next = workspaces(vec![
            vec![child_waits, pane(2, PaneStatus::Working)],
            vec![pane(3, PaneStatus::Idle)],
        ]);

        let found = transitions(previous.as_slice(), next.as_slice());
        let seen: Vec<_> = found
            .iter()
            .map(|t| (t.pane.id.0, t.before, t.after.status, t.after.child))
            .collect();
        assert_eq!(
            seen,
            [
                (1, PaneStatus::Working, PaneStatus::Waiting, Some("reviewer")),
                (3, PaneStatus::Working, PaneStatus::Idle, None),
            ],
            "a child waiting is its parent's row changing; an unchanged pane is not there"
        );
        assert_eq!(found[0].after.note, Some("approve rm"));
    }
}
