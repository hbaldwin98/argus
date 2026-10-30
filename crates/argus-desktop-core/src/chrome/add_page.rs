//! What the add-a-project dialog is to the page: the gestures the page makes in it, the kinds of
//! add, and the page it is told, with its folder rows and the projects it can add into.
//!
//! The state behind them is `add_project`; this is only the shape that crosses to the page and to
//! the GPUI window, which read the same page and send the same gestures.

use serde::{Deserialize, Serialize};

/// What was done in the add-a-project dialog.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum AddProjectGesture {
    Kind { kind: AddKind },
    /// To a folder, by the daemon's own spelling of its path: one typed,
    /// or the parent.
    Browse { path: String },
    /// Into a folder listed in the one browsed.
    Into { name: String },
    /// What the fields say now.
    Fields { name: String, skip: String, folder: String },
    /// Add, with what the fields say as it is pressed: calls need not
    /// arrive in the order sent, so the last `Fields` may come after.
    Add { name: String, skip: String, folder: String },
    /// Into this project, or `None` for a project of its own.
    AddTo { project: Option<u64> },
    Close,
}

/// What a project is added from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum AddKind {
    /// Every repository under a folder, found by the daemon's scan.
    #[default]
    Folder,
    /// The one repository a folder is.
    Repository,
    /// A repository `git init` makes.
    New,
}

/// The add-a-project dialog: a folder browsed on the daemon's machine, what
/// adding it would find, and what to call the project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AddProjectPage {
    pub open: bool,
    pub kind: AddKind,
    /// The folder browsed, as the daemon spells it; `None` until listed.
    pub path: Option<String>,
    pub parent: Option<String>,
    pub entries: Vec<FolderEntry>,
    /// A listing has been asked for and not yet come.
    pub browsing: bool,
    /// Why the folder lists nothing: gone, or not readable.
    pub listing_error: Option<String>,
    /// What adding the folder would add, as the daemon's scan finds it:
    /// root-relative, the folder itself named by its own name.
    pub repositories: Vec<String>,
    pub previewing: bool,
    /// How deep the daemon's scan looks, and what it always skips, once a
    /// preview has said.
    pub depth: Option<usize>,
    pub always_skipped: Vec<String>,
    /// What the name, skip and new-folder fields say.
    pub name: String,
    pub skip: String,
    pub folder: String,
    /// The name the project gets while the name field is empty.
    pub default_name: String,
    /// For a new repository: the folder `git init` runs in.
    pub target: Option<String>,
    /// Why Add cannot be pressed yet; `None` when it can.
    pub blocked: Option<String>,
    /// Where it is added: the project every repository goes into. `None` makes a project of its own.
    pub into: Option<ProjectRef>,
    /// Every project, to choose the one added to from.
    pub projects: Vec<ProjectRef>,
    /// The project the rail lists, which is the usual one to add to.
    pub current: Option<ProjectRef>,
    /// For one repository added into a project: the project of its own it already is, which stays.
    pub own_project: Option<ProjectRef>,
    /// The add is with the daemon.
    pub adding: bool,
    /// What the daemon said when it refused the last add.
    pub refusal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    pub name: String,
    pub is_repo: bool,
    /// With a project to add to: where the daemon already holds this folder, if it does.
    pub held: Option<Held>,
}

/// Where a folder that is a repository is already held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Held {
    /// In the project being added to.
    InTarget,
    /// In a project of its own, whose only repository it is.
    OwnProject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ProjectRef {
    pub id: u64,
    pub name: String,
}
