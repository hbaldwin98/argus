//! What the command palette can go to: the project's panes and checkouts,
//! every one of them rather than those of the repositories open on the
//! rail, and the features of the repository the Feature stage reads.
//!
//! Its commands are the keymap's (`keymap::commands`), sent once with the
//! bindings; this is what changes while it is open. The search is the
//! page's, over what this lists.

use argus_protocol::{DecisionBoard, PaneKind, ProjectInfo};
use argus_protocol::board::standing;

use crate::connection::page::{Palette, PaletteCheckout, PaletteFeature, PalettePane};
use crate::{chrome::rail, features};

/// `board` is the chosen repository's, once it has arrived.
pub fn build(project: Option<&ProjectInfo>, board: Option<&DecisionBoard>, tree: &[ProjectInfo]) -> Palette {
    let mut palette = Palette { open: true, features_loaded: board.is_some(), ..Palette::default() };
    for repo in project.iter().flat_map(|p| &p.repositories) {
        for checkout in &repo.checkouts {
            palette.checkouts.push(PaletteCheckout {
                id: checkout.id.0,
                name: checkout.name.clone(),
                repository: repo.name.clone(),
                branch: rail::branch(checkout),
                git: checkout.git.as_ref().map(|g| g.summary()),
                clean: checkout.git.as_ref().is_some_and(|g| !g.dirty),
            });
            for pane in checkout.listed_panes() {
                palette.panes.push(PalettePane {
                    id: pane.id.0,
                    title: pane.title.clone(),
                    agent: pane.kind == PaneKind::Agent,
                    status: pane.loudest_state().status.word(),
                    place: format!("{} / {}", repo.name, checkout.name),
                });
            }
        }
    }
    if let Some(board) = board {
        palette.features = standing::feature_rows(board, tree)
            .iter()
            .map(|row| PaletteFeature {
                slug: row.slug.clone(),
                number: row.number,
                title: row.title.clone(),
                standing: features::word(row.standing),
            })
            .collect();
    }
    palette
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{board, checkout, feature_tree, pane, project, repository};
    use argus_protocol::PaneStatus;

    #[test]
    fn every_checkout_and_pane_is_listed_with_where_it_is() {
        let tree = vec![project(1, "src", vec![
            repository(10, "argus", None, vec![
                checkout(100, "main", true, vec![pane(1, PaneKind::Agent, "claude", PaneStatus::Waiting)]),
                checkout(101, "spike", false, Vec::new()),
            ]),
            repository(20, "api", None, vec![checkout(200, "api", true, Vec::new())]),
        ])];
        let palette = build(tree.first(), None, &tree);
        let names: Vec<_> = palette.checkouts.iter().map(|c| (c.repository.as_str(), c.name.as_str())).collect();
        assert_eq!(names, [("argus", "main"), ("argus", "spike"), ("api", "api")], "quiet ones and worktrees too");
        assert_eq!(palette.panes.len(), 1);
        assert_eq!((palette.panes[0].status, palette.panes[0].place.as_str()), ("waiting", "argus / main"));
        assert!(!palette.features_loaded && palette.features.is_empty());
    }

    #[test]
    fn the_features_are_the_boards_with_how_each_stands() {
        let tree = feature_tree();
        let board = board();
        let palette = build(tree.first(), Some(&board), &tree);
        assert!(palette.features_loaded);
        assert_eq!(palette.features.len(), board.features.len());
        let old = palette.features.iter().find(|f| f.slug == "old").unwrap();
        assert_eq!((old.number, old.standing), (4, "archived"));
    }
}
