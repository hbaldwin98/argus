//! The layers that float over the columns: the fuzzy picker, the floating
//! window, the settings rows, and the typed or confirmed prompt.
//!
//! Mutually exclusive by construction — each is an `Option` on `App`, and
//! there is one input path per kind — so no gesture is ever ambiguous
//! about which layer it was aimed at.

use super::*;

/// What confirming a picker selection does. The picker is one widget with
/// one set of keys; this is the only thing that differs between uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerKind {
    /// Spawn the chosen agent template in the selected checkout.
    Agent,
    /// Switch to the chosen workspace. Ids ride along per row, and the
    /// bare names beside them: the rows themselves carry rollup counts, so
    /// they are not what "does this name already exist" can be asked of.
    Workspace {
        ids: Vec<WorkspaceId>,
        names: Vec<String>,
    },
    /// Switch the color theme.
    Theme,
    /// `git switch` the current checkout to the chosen branch. The last
    /// row is a synthetic "create" entry when the query names no existing
    /// branch, so making one is the same gesture as picking one.
    Branch { checkout: CheckoutId },
    /// Open the chosen file in the user's editor.
    File { checkout: CheckoutId },
    /// Jump the review cursor to the chosen changed file.
    Change,
    /// Send a prepared review comment to one live agent pane.
    ReviewRecipient {
        panes: Vec<PaneId>,
        checkout: CheckoutId,
        anchor: ReviewAnchor,
        body: String,
    },
}

impl PickerKind {
    /// Whether typing filters the list. The short lists don't need it, and
    /// a query line over four themes would be clutter.
    pub fn is_fuzzy(&self) -> bool {
        matches!(
            self,
            PickerKind::Branch { .. }
                | PickerKind::File { .. }
                | PickerKind::Change
                | PickerKind::Workspace { .. }
        )
    }

    /// Paths score differently from plain words.
    pub(super) fn matcher(&self) -> Fuzzy {
        match self {
            PickerKind::File { .. } | PickerKind::Change => Fuzzy::paths(),
            _ => Fuzzy::new(),
        }
    }
}

pub struct Picker {
    pub kind: PickerKind,
    pub title: &'static str,
    /// Everything on offer, in the order the daemon sent it.
    pub items: Vec<String>,
    /// What the user has typed, on a fuzzy picker.
    pub query: String,
    /// Indices into `items`, best match first. `sel` indexes into *this*.
    pub shown: Vec<usize>,
    pub sel: usize,
    /// An extra row offered below the matches — creating the branch you
    /// just typed the name of.
    pub create: Option<String>,
}

impl Picker {
    pub fn new(kind: PickerKind, title: &'static str, items: Vec<String>, sel: usize) -> Self {
        let shown = (0..items.len()).collect();
        Picker {
            kind,
            title,
            items,
            query: String::new(),
            shown,
            sel,
            create: None,
        }
    }

    /// The item under the cursor, or `None` when the cursor is on the
    /// create row.
    pub fn selected(&self) -> Option<&str> {
        let idx = *self.shown.get(self.sel)?;
        self.items.get(idx).map(String::as_str)
    }

    /// How many rows are on screen, the create row included.
    pub fn len(&self) -> usize {
        self.shown.len() + usize::from(self.create.is_some())
    }

    pub fn is_fuzzy(&self) -> bool {
        self.kind.is_fuzzy()
    }

    /// Sets the query and re-filters. For tests and dumps; the app itself
    /// goes through the key handler.
    #[cfg(test)]
    pub fn type_query(&mut self, q: &str) {
        self.query = q.to_string();
        self.refilter();
    }

    pub(super) fn on_create_row(&self) -> bool {
        self.create.is_some() && self.sel == self.shown.len()
    }

    /// Re-filters after a keystroke, keeping the cursor in range. The
    /// cursor goes back to the top: after a new query the old position
    /// refers to a row that is no longer there.
    pub(super) fn refilter(&mut self) {
        let mut matcher = self.kind.matcher();
        // Workspace rows carry rollup counts, so they are matched on their
        // names — otherwise typing a digit would "find" a workspace by how
        // many panes it happens to be running.
        self.shown = match &self.kind {
            PickerKind::Workspace { names, .. } => matcher.filter(&self.query, names),
            _ => matcher.filter(&self.query, &self.items),
        };

        // Offering to create a branch that already exists would be a
        // second, worse way to switch to it.
        self.create = match &self.kind {
            PickerKind::Branch { .. } => {
                let q = self.query.trim();
                (!q.is_empty() && !self.items.iter().any(|b| b == q)).then(|| q.to_string())
            }
            PickerKind::Workspace { names, .. } => {
                let q = self.query.trim();
                (!q.is_empty() && !names.iter().any(|n| n == q)).then(|| q.to_string())
            }
            _ => None,
        };
        self.sel = 0;
    }
}

/// A window floating above the columns, for things the five-column spine
/// has no room for. Unlike a picker it can be large and can hold a live
/// pane: a terminal editor in a 38%-wide column is unusable, and the whole
/// point of `$EDITOR` support is that it be usable.
///
/// It floats rather than replacing the columns — the tree stays visible
/// around the edges, which is the same rule every other view here follows.
pub enum Overlay {
    /// A pty pane, drawn large. The title is carried rather than looked up
    /// so it survives the pane leaving the tree.
    Pane {
        pane: PaneId,
        title: String,
        /// Kill the pane when the window closes. True for editors: they
        /// are not listed in the panes column, so a surviving one would be
        /// a process with no window and no way back to it.
        ephemeral: bool,
    },
    /// Preferences, with room to say what each one does.
    Settings { sel: usize },
    /// The diff. The view itself lives on `App::review`; this only says
    /// which window is up. Floating rather than in the column so reading a
    /// diff never costs you sight of the agent that produced it.
    Review,
    /// Recent commits and the files each changed. Same window as review;
    /// opening a commit replaces this with [`Overlay::Review`] without
    /// dropping the list, so going back is instant.
    History,
    /// A project's or checkout's note. The text lives on `App::notes`;
    /// this only says which window is up. Floating for the same reason
    /// review is: writing down what a checkout still owes should not cost
    /// you sight of the agent working in it.
    Notes,
}

impl Overlay {
    pub(super) fn pane(&self) -> Option<PaneId> {
        match self {
            Overlay::Pane { pane, .. } => Some(*pane),
            Overlay::Settings { .. } | Overlay::Review | Overlay::History | Overlay::Notes => {
                None
            }
        }
    }
}

/// The rows of the settings panel, in the order they are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Editor,
    EditorCmd,
    PaneView,
    Theme,
    Notifications,
}

impl Setting {
    pub const ALL: &'static [Setting] = &[
        Setting::Editor,
        Setting::EditorCmd,
        Setting::PaneView,
        Setting::Theme,
        Setting::Notifications,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Setting::Editor => "editor opens",
            Setting::EditorCmd => "editor command",
            Setting::PaneView => "pane view",
            Setting::Theme => "theme",
            Setting::Notifications => "notifications",
        }
    }
}

/// What a `ConfirmRemove` prompt is about to take away. A checkout's
/// removal deletes its worktree and branch, and a branch's deletes the
/// branch; the other two only stop showing something, leaving every file
/// where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveTarget {
    Checkout(CheckoutId),
    Repository(RepositoryId),
    Project(ProjectId),
    /// A branch with no directory of its own. It carries the checkout git
    /// will be run from, precisely because the branch hasn't got one.
    ///
    /// `force` is the second ask, raised only after git has refused the
    /// first: it is `git branch -D`, and what it takes away is commits, not
    /// just a name.
    Branch {
        checkout: CheckoutId,
        branch: String,
        force: bool,
    },
}

impl RemoveTarget {
    pub(super) fn message(&self) -> ClientMsg {
        match self {
            RemoveTarget::Checkout(checkout) => ClientMsg::RemoveCheckout {
                checkout: *checkout,
            },
            RemoveTarget::Repository(repository) => ClientMsg::RemoveRepository {
                repository: *repository,
            },
            RemoveTarget::Project(project) => ClientMsg::RemoveProject { project: *project },
            RemoveTarget::Branch {
                checkout,
                branch,
                force,
            } => ClientMsg::DeleteBranch {
                checkout: *checkout,
                branch: branch.clone(),
                force: *force,
            },
        }
    }

    /// Popup title and the line under the name — what the user is agreeing
    /// to, which for two of the four is "nothing on disk".
    pub fn wording(&self) -> (&'static str, &'static str) {
        match self {
            RemoveTarget::Checkout(_) => {
                ("remove checkout?", "  — worktree, branch, and its panes")
            }
            RemoveTarget::Repository(_) => {
                ("remove repository?", "  — from this panel only; files stay")
            }
            RemoveTarget::Project(_) => ("remove project?", "  — from this panel only; files stay"),
            RemoveTarget::Branch { force: false, .. } => (
                "delete branch?",
                "  — the local branch only; the remote is untouched",
            ),
            RemoveTarget::Branch { force: true, .. } => (
                "branch isn't merged — delete anyway?",
                "  — its commits stop being reachable",
            ),
        }
    }
}

/// A modal text/confirm prompt, mutually exclusive with `Picker`. Both new
/// worktree (free text) and remove (yes/no) go through this so
/// there's one input path and one place `on_mouse` has to know to ignore.
pub enum Prompt {
    NewWorktree {
        base: CheckoutId,
        input: String,
    },
    /// The name of a repository to create, once the directory browser has
    /// settled where it goes. Empty means the chosen directory itself —
    /// the folder is already there and only needs a `git init`.
    NewRepository {
        project: ProjectId,
        parent: String,
        input: String,
    },
    ConfirmRemove {
        target: RemoveTarget,
        label: String,
    },
    Comment {
        anchor: ReviewAnchor,
        input: String,
    },
    /// The editor command, typed rather than cycled — it is free text.
    EditorCommand {
        input: String,
    },
}
