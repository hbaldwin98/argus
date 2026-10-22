//! Which overlays the window has, and the rules each one is held to.
//!
//! Only the rules that do not depend on what an overlay shows are here: whether it shares the
//! window with others of its kind, and whether it takes the keys from a pane. The overlays are
//! added as their surfaces land; the host (`host`) knows no more than this list.

/// An overlay the window can have up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    /// The command palette.
    Palette,
    /// The shortcuts sheet.
    Sheet,
    Settings,
    /// Add a project.
    AddProject,
    /// A question to answer yes or no: removing a project.
    Question,
    /// The Checkouts stage's question before a worktree is removed or a branch forced out; apart from
    /// the project's so neither stage's follow closes the other's.
    Removal,
    /// A dialog an extension asks.
    Extension,
    /// Naming a pane.
    Rename,
    /// Naming the branch of a new worktree.
    Worktree,
    /// The card over the window while the daemon is gone.
    Stopped,
    /// A drawing of an agent's view opened larger, over the whole window.
    Figure,
}

impl Overlay {
    /// The palette, the sheet and the settings are never up together: opening one closes the
    /// others (SHELL-KEYS-13). Nothing else is exclusive, so a dialog may be over another.
    pub fn exclusive(self) -> bool {
        matches!(self, Overlay::Palette | Overlay::Sheet | Overlay::Settings)
    }

    /// Takes the keys from a pane when it opens and gives them back when the last of its kind
    /// closes (SHELL-SEAM-10, `takesFromPane`). The page's list names more dialogs (link,
    /// reconcile, sessions and the readers); each joins when its surface does.
    pub fn takes_the_keys_from_a_pane(self) -> bool {
        matches!(self, Overlay::AddProject)
    }
}
