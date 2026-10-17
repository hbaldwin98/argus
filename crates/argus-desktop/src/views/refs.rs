//! Where a reference in an agent's view points: the checkout, repository
//! or pane it names, found in the tree from the pane that showed the view.
//!
//! The page never builds a path or picks a checkout; it says which
//! reference was pressed and which pane showed it, and this answers with
//! the one thing to do or the sentence to say instead. An extension's
//! table has no pane, so it is read against the checkout on screen.

use argus_protocol::{CheckoutId, CheckoutInfo, DecisionBoard, PaneId, ProjectInfo, RepositoryId, RepositoryInfo};

use crate::connection::page::Reference;

/// What a reference comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// `path`, relative to `checkout`'s root, in the person's editor.
    Editor { checkout: CheckoutId, path: String, line: Option<u32> },
    /// The checkout's history, the cursor on `sha`.
    History { checkout: CheckoutId, sha: String },
    /// The feature numbered `number` of `repository`.
    Feature { repository: RepositoryId, number: i64 },
    Pane(PaneId),
}

/// The repository and checkout holding `pane`.
pub fn locate(tree: &[ProjectInfo], pane: PaneId) -> Option<(&RepositoryInfo, &CheckoutInfo)> {
    tree.iter().flat_map(|p| &p.repositories).find_map(|r| {
        r.checkouts.iter().find(|c| c.panes.iter().any(|p| p.id == pane)).map(|c| (r, c))
    })
}

fn holding(tree: &[ProjectInfo], checkout: CheckoutId) -> Option<(&RepositoryInfo, &CheckoutInfo)> {
    tree.iter().flat_map(|p| &p.repositories).find_map(|r| r.checkouts.iter().find(|c| c.id == checkout).map(|c| (r, c)))
}

fn is_hash(sha: &str) -> bool {
    (4..=64).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Resolves `reference`, shown by `from`'s agent, or by nobody and read
/// against `here`, the checkout on screen.
pub fn resolve(reference: &Reference, from: Option<PaneId>, here: Option<CheckoutId>, tree: &[ProjectInfo]) -> Result<Target, String> {
    if let Reference::Pane { pane } = reference {
        let pane = PaneId(*pane);
        return locate(tree, pane).map(|_| Target::Pane(pane)).ok_or_else(|| format!("pane #{} is gone", pane.0));
    }
    let (repository, checkout) = match from {
        Some(pane) => locate(tree, pane).ok_or_else(|| "the pane that showed this has gone".to_string())?,
        None => here.and_then(|c| holding(tree, c)).ok_or_else(|| "no checkout is on screen to read this against".to_string())?,
    };
    match reference {
        Reference::Path { path, line } if argus_protocol::is_checkout_relative(path) => {
            Ok(Target::Editor { checkout: checkout.id, path: path.replace('\\', "/"), line: *line })
        }
        Reference::Path { path, .. } => Err(format!("{path} is not a path inside the checkout")),
        Reference::Commit { sha } if is_hash(sha) => Ok(Target::History { checkout: checkout.id, sha: sha.clone() }),
        Reference::Commit { sha } => Err(format!("{sha} is not a commit hash")),
        Reference::Feature { number } => Ok(Target::Feature { repository: repository.id, number: *number }),
        Reference::Pane { .. } => unreachable!("handled above"),
    }
}

/// A feature a reference named, waiting for the board of its repository:
/// the number is only known there, so the reference cannot be turned into
/// the feature until the board has come.
#[derive(Default)]
pub struct PendingFeature {
    waiting: Option<(RepositoryId, i64)>,
}

/// What the wait comes to.
#[derive(Debug, PartialEq, Eq)]
pub enum Found {
    /// The feature, by the slug the Feature stage reveals it by.
    Slug(String),
    /// The board is here and holds no feature of that number.
    Missing(i64),
}

impl PendingFeature {
    pub fn wait_for(&mut self, repository: RepositoryId, number: i64) {
        self.waiting = Some((repository, number));
    }

    /// Settles the wait if it can be. `there` is whether the Feature stage
    /// is up on the repository waited for; the person going elsewhere
    /// meanwhile abandons it. `board` is the one the stage holds, if any.
    pub fn settle(&mut self, there: impl FnOnce(RepositoryId) -> bool, board: Option<&DecisionBoard>) -> Option<Found> {
        let (repository, number) = self.waiting?;
        if !there(repository) {
            self.waiting = None;
            return None;
        }
        let board = board.filter(|b| b.repositories.is_empty() || b.repositories.contains(&repository))?;
        self.waiting = None;
        let slug = board.features.iter().find(|f| f.number == number).map(|f| f.slug.clone());
        Some(slug.map_or(Found::Missing(number), Found::Slug))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane, project, repository};
    use argus_protocol::{PaneKind, PaneStatus};

    fn tree() -> Vec<ProjectInfo> {
        let agent = |id| pane(id, PaneKind::Agent, "claude", PaneStatus::Idle);
        vec![project(1, "src", vec![
            repository(10, "argus", None, vec![checkout(100, "main", true, vec![agent(1)]), checkout(101, "work", false, vec![agent(2)])]),
            repository(11, "other", None, vec![checkout(110, "main", true, vec![agent(3)])]),
        ])]
    }

    fn path(p: &str) -> Reference {
        Reference::Path { path: p.into(), line: Some(7) }
    }

    #[test]
    fn a_path_opens_in_the_checkout_of_the_pane_that_showed_it() {
        let target = resolve(&path("src/a.rs"), Some(PaneId(2)), None, &tree());
        assert_eq!(target, Ok(Target::Editor { checkout: CheckoutId(101), path: "src/a.rs".into(), line: Some(7) }));
    }

    #[test]
    fn an_extensions_reference_is_read_against_the_checkout_on_screen() {
        let target = resolve(&Reference::Commit { sha: "ab12cd".into() }, None, Some(CheckoutId(110)), &tree());
        assert_eq!(target, Ok(Target::History { checkout: CheckoutId(110), sha: "ab12cd".into() }));
        assert!(resolve(&path("a"), None, None, &tree()).is_err(), "nothing on screen, nothing to read against");
    }

    #[test]
    fn a_path_outside_the_checkout_is_refused() {
        for bad in ["/etc/passwd", "\\win", "C:\\x", "c:/x", "../x", "a/../../x", "a\\..\\x", "", "a\0b", ".. ", "...", "a/.../b", "a\nb"] {
            assert!(resolve(&path(bad), Some(PaneId(1)), None, &tree()).is_err(), "{bad:?}");
        }
        assert!(resolve(&path("a..b/c.rs"), Some(PaneId(1)), None, &tree()).is_ok(), "dots in a name are not going up");
    }

    #[test]
    fn a_commit_must_read_as_a_hash() {
        for bad in ["", "abc", "xyz123", "main; rm", "--all"] {
            assert!(resolve(&Reference::Commit { sha: bad.into() }, Some(PaneId(1)), None, &tree()).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_feature_is_of_the_repository_the_pane_belongs_to() {
        let target = resolve(&Reference::Feature { number: 4 }, Some(PaneId(3)), Some(CheckoutId(100)), &tree());
        assert_eq!(target, Ok(Target::Feature { repository: RepositoryId(11), number: 4 }));
    }

    #[test]
    fn a_pane_that_is_gone_is_said_so_and_one_that_is_there_is_focused() {
        assert_eq!(resolve(&Reference::Pane { pane: 2 }, None, None, &tree()), Ok(Target::Pane(PaneId(2))));
        assert!(resolve(&Reference::Pane { pane: 99 }, Some(PaneId(1)), None, &tree()).unwrap_err().contains("gone"));
        assert!(resolve(&Reference::Feature { number: 1 }, Some(PaneId(99)), None, &tree()).is_err());
    }

    #[test]
    fn a_feature_reference_waits_for_its_repositorys_board_and_is_abandoned_when_the_person_goes() {
        let mut pending = PendingFeature::default();
        assert_eq!(pending.settle(|_| true, None), None, "nothing waited for");
        pending.wait_for(RepositoryId(10), 4);
        assert_eq!(pending.settle(|_| true, None), None, "no board yet, still waiting");
        let mut board = DecisionBoard { repositories: vec![RepositoryId(11)], ..Default::default() };
        assert_eq!(pending.settle(|_| true, Some(&board)), None, "another repository's board is not it");
        board.repositories = vec![RepositoryId(10)];
        assert_eq!(pending.settle(|_| true, Some(&board)), Some(Found::Missing(4)));
        assert_eq!(pending.settle(|_| true, Some(&board)), None, "settled once");
        pending.wait_for(RepositoryId(10), 4);
        assert_eq!(pending.settle(|_| false, Some(&board)), None);
        assert_eq!(pending.settle(|_| true, Some(&board)), None, "abandoned for good");
    }
}
