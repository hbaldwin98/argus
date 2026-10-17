//! A client's copy of the tree, and finding things in it: every pane and
//! checkout, where a pane or checkout sits, an agent's telemetry applied
//! between trees, and which panes changed state from the copy before.
//!
//! Every query here counts editors: a pane is found by searching its
//! checkout's panes, whether or not the checkout lists it as a row.
//!
//! Here beside the tree rather than in the terminal client, so that any
//! client holding a copy reads what changed as the terminal does: two
//! readings would ring a bell in one for a pane the other never mentions.

use std::collections::HashMap;

use crate::tree::ids::{CheckoutId, PaneId};
use crate::tree::{AgentTelemetry, CheckoutInfo, PaneInfo, PaneState, PaneStatus, ProjectInfo, RepositoryInfo};

/// Where a checkout sits: everything above it, so a caller that found it
/// by id need not walk the tree again for its names.
#[derive(Debug, Clone, Copy)]
pub struct Lineage<'a> {
    pub project: &'a ProjectInfo,
    pub repository: &'a RepositoryInfo,
    pub checkout: &'a CheckoutInfo,
}

/// A client's copy of the tree (the open workspace's projects), and finding
/// things in it.
pub trait HeldTree {
    fn checkouts(&self) -> impl Iterator<Item = &CheckoutInfo>;

    fn panes(&self) -> impl Iterator<Item = &PaneInfo> {
        self.checkouts().flat_map(|checkout| checkout.panes.iter())
    }

    fn panes_mut(&mut self) -> impl Iterator<Item = &mut PaneInfo>;

    fn checkout(&self, id: CheckoutId) -> Option<&CheckoutInfo> {
        self.checkouts().find(|checkout| checkout.id == id)
    }

    fn lineage_of_checkout(&self, id: CheckoutId) -> Option<Lineage<'_>>;

    fn lineage_of_pane(&self, id: PaneId) -> Option<(Lineage<'_>, &PaneInfo)>;

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
    fn checkouts(&self) -> impl Iterator<Item = &CheckoutInfo> {
        self.iter()
            .flat_map(|project| project.repositories.iter())
            .flat_map(|repository| repository.checkouts.iter())
    }

    fn panes_mut(&mut self) -> impl Iterator<Item = &mut PaneInfo> {
        self.iter_mut()
            .flat_map(|project| project.repositories.iter_mut())
            .flat_map(|repository| repository.checkouts.iter_mut())
            .flat_map(|checkout| checkout.panes.iter_mut())
    }

    fn lineage_of_checkout(&self, id: CheckoutId) -> Option<Lineage<'_>> {
        lineages(self).find(|lineage| lineage.checkout.id == id)
    }

    fn lineage_of_pane(&self, id: PaneId) -> Option<(Lineage<'_>, &PaneInfo)> {
        lineages(self).find_map(|lineage| {
            let pane = lineage.checkout.panes.iter().find(|pane| pane.id == id)?;
            Some((lineage, pane))
        })
    }
}

fn lineages(tree: &[ProjectInfo]) -> impl Iterator<Item = Lineage<'_>> {
    tree.iter().flat_map(|project| {
        project.repositories.iter().flat_map(move |repository| {
            repository.checkouts.iter().map(move |checkout| Lineage { project, repository, checkout })
        })
    })
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
/// `next`'s order. Which of them are worth a bell or a note is each
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
    use crate::tree::ids::{CheckoutId, ProjectId, RepositoryId};
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
            background: false,
            since: None,
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
                    unmerged: Default::default(),
                    last_used: None,
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

    fn ids(tree: &(impl HeldTree + ?Sized)) -> Vec<u64> {
        tree.panes().map(|pane| pane.id.0).collect()
    }

    #[test]
    fn every_pane_is_walked_in_every_project() {
        let mut editor = pane(3, PaneStatus::Idle);
        editor.kind = PaneKind::Editor;
        let tree = projects(vec![
            vec![pane(1, PaneStatus::Idle), pane(2, PaneStatus::Idle)],
            vec![editor],
        ]);
        assert_eq!(ids(tree.as_slice()), [1, 2, 3], "editors are panes too");
    }

    #[test]
    fn every_checkout_is_walked_and_found_in_every_project() {
        let tree = projects(vec![vec![], vec![], vec![]]);
        let walked: Vec<u64> = tree.checkouts().map(|checkout| checkout.id.0).collect();
        assert_eq!(walked, [0, 1, 2]);
        assert_eq!(tree.checkout(CheckoutId(2)).unwrap().path, "/r2");
    }

    #[test]
    fn an_editor_pane_is_placed_under_its_checkout() {
        let mut editor = pane(3, PaneStatus::Idle);
        editor.kind = PaneKind::Editor;
        let tree = projects(vec![vec![pane(1, PaneStatus::Idle)], vec![pane(2, PaneStatus::Idle), editor]]);

        let (lineage, pane) = tree.lineage_of_pane(PaneId(3)).expect("editors are placed too");
        assert_eq!(pane.id, PaneId(3));
        assert_eq!(lineage.project.id, ProjectId(1));
        assert_eq!(lineage.repository.id, RepositoryId(1));
        assert_eq!(lineage.checkout.id, CheckoutId(1));
    }

    #[test]
    fn a_checkout_lineage_names_everything_above_it() {
        let tree = projects(vec![vec![], vec![]]);
        let lineage = tree.lineage_of_checkout(CheckoutId(1)).unwrap();
        assert_eq!(
            (lineage.project.name.as_str(), lineage.repository.name.as_str(), lineage.checkout.path.as_str()),
            ("p1", "r1", "/r1")
        );
    }

    #[test]
    fn a_lineage_names_the_repository_and_checkout_that_hold_it() {
        let mut tree = projects(vec![vec![]]);
        let first = tree[0].repositories[0].clone();
        let checkout = |id: u64, panes: Vec<PaneInfo>| CheckoutInfo {
            id: CheckoutId(id),
            name: format!("c{id}"),
            path: format!("/c{id}"),
            panes,
            git: None,
            primary: false,
        };
        tree[0].repositories.push(RepositoryInfo {
            id: RepositoryId(7),
            name: "second".into(),
            checkouts: vec![checkout(10, vec![pane(1, PaneStatus::Idle)]), checkout(11, vec![pane(2, PaneStatus::Idle)])],
            ..first
        });

        let lineage = tree.lineage_of_checkout(CheckoutId(11)).unwrap();
        assert_eq!((lineage.repository.id, lineage.checkout.id), (RepositoryId(7), CheckoutId(11)));
        let (lineage, found) = tree.lineage_of_pane(PaneId(2)).unwrap();
        assert_eq!(found.id, PaneId(2));
        assert_eq!((lineage.repository.id, lineage.checkout.id), (RepositoryId(7), CheckoutId(11)));
    }

    #[test]
    fn a_missing_id_is_found_nowhere() {
        let tree = projects(vec![vec![pane(1, PaneStatus::Idle)]]);
        assert!(tree.checkout(CheckoutId(9)).is_none());
        assert!(tree.lineage_of_checkout(CheckoutId(9)).is_none());
        assert!(tree.lineage_of_pane(PaneId(9)).is_none());
    }

    #[test]
    fn telemetry_lands_on_its_pane_and_says_whether_anything_changed() {
        let mut tree = projects(vec![vec![pane(1, PaneStatus::Working)], vec![pane(2, PaneStatus::Working)]]);
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
        let previous = projects(vec![
            vec![pane(1, PaneStatus::Working), pane(2, PaneStatus::Working)],
            vec![pane(3, PaneStatus::Working)],
        ]);
        let mut child_waits = pane(1, PaneStatus::Working);
        child_waits.children.push(ChildAgentInfo {
            label: "reviewer".into(),
            status: PaneStatus::Waiting,
            note: Some("approve rm".into()),
        });
        let next = projects(vec![
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
