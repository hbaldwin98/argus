//! Which row holds the keys, and where the pick goes when the tree changes under it.
//!
//! The one thing the rail holds that the tree does not say is the picked row (SHELL-RAIL-19), which
//! only the window knows, and it follows the tree: a fold or a close moves it to the checkout it
//! was under (SHELL-RAIL-8, 19), so the keys never act on a row that is gone.

use argus_desktop_core::connection::page::Rail;

use super::RailView;

/// The row that holds the keys: a checkout, or a pane and the checkout it is
/// under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picked {
    pub checkout: u64,
    pub pane: Option<u64>,
}

/// Where the pick goes once the tree has changed under it: only a pane no
/// longer listed (closed, or its checkout folded) moves it, to its
/// checkout. Nothing else does (SHELL-RAIL-19), so a checkout that is gone
/// keeps the pick, which then marks no row.
pub(super) fn settled(picked: Option<Picked>, rail: &Rail) -> Option<Picked> {
    let picked = picked?;
    let Some(pane) = picked.pane else { return Some(picked) };
    let listed = rail
        .project
        .iter()
        .flat_map(|p| &p.repositories)
        .filter(|r| r.open)
        .flat_map(|r| &r.checkouts)
        .find(|c| c.id == picked.checkout)
        .is_some_and(|c| c.open && c.panes.iter().any(|p| p.id == pane));
    Some(if listed { picked } else { Picked { checkout: picked.checkout, pane: None } })
}

impl RailView {
    pub fn picked(&self) -> Option<Picked> {
        self.picked
    }

    /// Picks `row` unless the sidebar is held.
    pub(super) fn pick(&mut self, held: bool, row: Picked) {
        if !held {
            self.picked = Some(row);
        }
    }
}
