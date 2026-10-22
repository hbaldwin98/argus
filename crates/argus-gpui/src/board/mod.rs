//! The Feature stage's surfaces as the window draws them: a feature opened from the index and
//! the lenses it is read through, and a new feature written as one note.

pub mod capture;
pub mod feature_view;
pub mod install;
pub mod lenses;
#[cfg(test)]
pub(crate) mod snapshots;
