//! A checkout's folder as it is on disk, one level at a time: what is in it
//! and what Git makes of each entry. Read-only; nothing here writes to the
//! folder. The Code stage reads a version out of Git, and this reads the
//! disk, so untracked and ignored files are in it.

use serde::{Deserialize, Serialize};

use crate::tree::ids::CheckoutId;

/// Most entries one folder's listing carries: a folder past this is cut,
/// and says so, rather than flooding the connection.
pub const MAX_DIR_ENTRIES: usize = 5_000;

/// What Git makes of an entry that is not clean. A tracked, unchanged entry
/// has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryGit {
    /// Tracked and changed from `HEAD`, in the index or the working tree; a
    /// folder when anything under it is changed or untracked.
    Changed,
    /// Not tracked and not ignored.
    Untracked,
    /// Ignored by Git.
    Ignored,
}

/// One thing in a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutEntry {
    pub name: String,
    /// A folder (a symlink to one included).
    pub dir: bool,
    /// Bytes; none for a folder.
    pub size: Option<u64>,
    /// Seconds since the Unix epoch.
    pub modified: Option<u64>,
    pub git: Option<EntryGit>,
}

/// One folder of a checkout: folders first, then files, each by name
/// without regard to case. `.git` is never in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutDir {
    pub request_id: u64,
    pub checkout: CheckoutId,
    /// Relative to the checkout, `/`-separated; empty for its root.
    pub path: String,
    pub entries: Vec<CheckoutEntry>,
    /// The folder held more than [`MAX_DIR_ENTRIES`].
    #[serde(default)]
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_survives_the_wire() {
        let dir = CheckoutDir {
            request_id: 4,
            checkout: CheckoutId(2),
            path: "crates/argusd".into(),
            entries: vec![
                CheckoutEntry { name: "src".into(), dir: true, size: None, modified: Some(9), git: Some(EntryGit::Changed) },
                CheckoutEntry { name: "a.rs".into(), dir: false, size: Some(12), modified: None, git: None },
            ],
            truncated: true,
        };
        let bytes = rmp_serde::to_vec_named(&dir).unwrap();
        assert_eq!(rmp_serde::from_slice::<CheckoutDir>(&bytes).unwrap(), dir);
    }
}
