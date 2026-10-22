//! What the add-a-project dialog says of what adding would do, and what its Add button reads: the
//! folder list's states, the outcome beside it per kind, the footer, and the pause before typing is
//! sent (the page's `chrome/projects.ts`, `folders`, `outcome` and `addDialog`).
//!
//! The dialog's state is `chrome/add_project`; this is its wording, so the page and the GPUI window
//! say the same thing about the same page. Whether a press is allowed is Rust's `blocked`, which the
//! button only shows.

use std::time::Duration;

use super::add_project::{AddKind, AddProjectPage};
use super::projects::counted;

/// How long typing pauses before the fields are sent: a changed skip scans the folder again, so a
/// scan a letter is too many.
pub const FIELDS_PAUSE: Duration = Duration::from_millis(250);

/// The kinds of add, as the control offers them.
pub const KINDS: [(AddKind, &str); 3] =
    [(AddKind::Folder, "A folder of repositories"), (AddKind::Repository, "One repository"), (AddKind::New, "A new repository")];

pub const NEW_FOLDER_LABEL: &str = "New repository\u{2019}s folder";
pub const NEW_FOLDER_PLACEHOLDER: &str = "scratch";
pub const NEW_NOTE: &str = "Made inside the folder on the left, and git init runs there. A folder that is already a repository is added as it is.";
pub const NOT_A_REPOSITORY: &str = "Pick a folder marked git, or add this one as a folder of repositories.";
pub const SKIP_PLACEHOLDER: &str = "vendor, infra/legacy";
pub const NO_FOLDERS: &str = "No folders in here.";

/// What the folder list holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Folders {
    /// Browsing with nothing listed yet: four bars.
    Waiting,
    /// The listing failed, in the daemon's words.
    Failed(String),
    Empty,
    /// The page's `entries`.
    Listed,
}

pub fn folders(page: &AddProjectPage) -> Folders {
    if page.browsing && page.entries.is_empty() {
        Folders::Waiting
    } else if let Some(error) = &page.listing_error {
        Folders::Failed(error.clone())
    } else if page.entries.is_empty() {
        Folders::Empty
    } else {
        Folders::Listed
    }
}

/// What the add would do, as the daemon has said it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A new repository: its full target path when there is one.
    New { target: Option<String> },
    /// Looking for repositories: three bars.
    Looking,
    /// One repository: the name it would get, or `None` when the folder is not exactly one.
    Repository { would_add: Option<String> },
    /// A folder of repositories: `Would add N repositories`, which, and how it searched.
    Folder { heading: String, repositories: Vec<String>, searched: Option<String> },
}

pub fn outcome(page: &AddProjectPage) -> Outcome {
    if page.kind == AddKind::New {
        return Outcome::New { target: page.target.clone() };
    }
    if page.previewing || (page.path.is_some() && page.depth.is_none() && page.listing_error.is_none()) {
        return Outcome::Looking;
    }
    if page.kind == AddKind::Repository {
        let one = page.repositories.len() == 1 && page.repositories[0] == page.default_name;
        return Outcome::Repository { would_add: one.then(|| page.default_name.clone()) };
    }
    Outcome::Folder {
        heading: format!("Would add {}", counted(page.repositories.len(), "repository", "repositories")),
        repositories: page.repositories.clone(),
        searched: page.depth.map(|depth| {
            format!(
                "Searched up to {depth} folders deep. A repository\u{2019}s own submodules and worktrees stay part of it. New ones cloned here later join on their own."
            )
        }),
    }
}

/// What sits beside the skip field: what is always skipped, when the scan has said.
pub fn always_skipped(page: &AddProjectPage) -> Option<String> {
    (!page.always_skipped.is_empty()).then(|| format!("{} are always skipped", page.always_skipped.join(", ")))
}

pub fn footer(kind: AddKind) -> &'static str {
    if kind == AddKind::New {
        "Only Argus\u{2019}s config changes, and the new repository"
    } else {
        "Only Argus\u{2019}s config changes"
    }
}

/// `Add <name>`: the name typed, trimmed, else the default name, else `project`; `Adding\u{2026}`
/// while the daemon has the add.
pub fn add_label(typed_name: &str, page: &AddProjectPage) -> String {
    if page.adding {
        return "Adding\u{2026}".to_owned();
    }
    let typed = typed_name.trim();
    let name = if typed.is_empty() { page.default_name.as_str() } else { typed };
    format!("Add {}", if name.is_empty() { "project" } else { name })
}

/// Whether Add can be pressed: not while blocked or adding.
pub fn can_add(page: &AddProjectPage) -> bool {
    page.blocked.is_none() && !page.adding
}

#[cfg(test)]
mod tests;
