//! The Feature stage as the window draws it: the index of a repository's features as a list
//! beside a peek or as a board, a feature opened from it and the lenses it is read through, and
//! a new feature written as one note.

pub mod capture;
pub mod cells;
pub mod columns;
pub mod ctx;
pub mod feature_stage;
pub mod feature_view;
pub mod install;
pub mod lenses;
pub mod list;
pub mod menus;
pub mod peek;
#[cfg(test)]
pub(crate) mod snapshots;
pub mod tracker;
