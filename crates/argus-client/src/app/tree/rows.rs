//! Naming a row of the tree independently of where it currently sits.
//!
//! Row indices shift whenever a branch appears or disappears above the
//! selection, so a tree arriving from the daemon has to be able to find
//! the same row again by identity rather than by position.

use crate::app::*;

/// One row of the checkouts column; its order is the protocol's, so the
/// desktop window lists the same rows.
pub use argus_protocol::CheckoutRow;

/// What the checkouts column had selected, as an identity rather than a
/// position. Row indices shift under the column whenever a branch row
/// appears or disappears above the selection, so a tree arriving from the
/// daemon has to be able to find the same row again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutAnchor {
    Checkout(CheckoutId),
    /// A branch with no directory, held by name: the checkout that would
    /// give it one is the same row as far as the user is concerned.
    Branch(String),
    Remote(String),
}

/// A pane's coordinates in the daemon tree. `checkout` indexes the
/// repository's checkouts, while `pane` indexes `listed_panes()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneLocation {
    pub project: usize,
    pub repository: usize,
    pub checkout: usize,
    pub pane: usize,
}
