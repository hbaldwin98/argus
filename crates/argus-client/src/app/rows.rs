//! Naming a row of the tree independently of where it currently sits.
//!
//! Row indices shift whenever a branch appears or disappears above the
//! selection, so a tree arriving from the daemon has to be able to find
//! the same row again by identity rather than by position.

use super::*;

/// One row of the checkouts column, as an index into the repository's own
/// `checkouts` or `branches`. The two kinds interleave — the main branch
/// leads the column whichever it turns out to be — so the column's order is
/// [`App::checkout_rows`] rather than either list on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutRow {
    Checkout(usize),
    Branch(usize),
    /// A branch that exists on a remote and nowhere else — an index into
    /// `remote_branches`, which holds them as `origin/feature`.
    Remote(usize),
}

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
