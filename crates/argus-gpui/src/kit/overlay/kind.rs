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
    /// The question an extension view's action asks before it is sent; apart from the extension's own
    /// dialog, so neither's follow closes the other's.
    ExtensionConfirm,
    /// The tracker link dialog: the URL of a ticket, the tickets linked, the tracker map.
    Link,
    /// The reconcile dialog: an agent's proposal to bring a feature and its tickets in line.
    Reconcile,
    /// Naming a pane.
    Rename,
    /// Naming a workspace to make.
    NewWorkspace,
    /// Naming the branch of a new worktree.
    Worktree,
    /// Asking why a feature is held.
    Hold,
    /// The card over the window while the daemon is gone.
    Stopped,
    /// A drawing of an agent's view opened larger, over the whole window.
    Figure,
    /// The recent sessions of a repository, and the transcript read in its place.
    Sessions,
}

impl Overlay {
    /// The palette, the sheet and the settings are never up together: opening one closes the
    /// others (SHELL-KEYS-13). Nothing else is exclusive, so a dialog may be over another.
    pub fn exclusive(self) -> bool {
        matches!(self, Overlay::Palette | Overlay::Sheet | Overlay::Settings)
    }

    /// Takes the keys from a pane when it opens and gives them back when the last of its kind
    /// closes (SHELL-SEAM-10, `takesFromPane`). The page's list names more dialogs (sessions and
    /// the readers); each joins when its surface does.
    pub fn takes_the_keys_from_a_pane(self) -> bool {
        matches!(self, Overlay::AddProject | Overlay::Link | Overlay::Reconcile | Overlay::Sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::Overlay;

    /// SHELL-SEAM-10, BOARD-LINK-15: the tracker's dialogs and the sessions dialog take the keys from a pane, so the host
    /// hands them back when the last closes; the settings and the palette do not.
    #[test]
    fn the_add_project_link_and_reconcile_dialogs_take_the_keys_from_a_pane() {
        for kind in [Overlay::AddProject, Overlay::Link, Overlay::Reconcile, Overlay::Sessions] {
            assert!(kind.takes_the_keys_from_a_pane(), "{kind:?}");
        }
        for kind in [Overlay::Palette, Overlay::Sheet, Overlay::Settings, Overlay::Question] {
            assert!(!kind.takes_the_keys_from_a_pane(), "{kind:?}");
        }
    }
}
