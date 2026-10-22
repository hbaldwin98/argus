//! Which surface fills the stage, and which parts of the window's frame are put away around it:
//! the one place the session's stage, the window's Git stage and the console's place are read
//! together (SHELL-STAGE-1..4).
//!
//! The session has no Git stage (`review::entry`), so Git is the window's word; it comes first,
//! over the console too, as the work area has always drawn it. The console is the stage while it
//! is a tab or fullscreen. Else the session's stage decides, and the Workspace is its pane or,
//! with none on screen, the empty stage. Two sources of truth read apart would disagree for a
//! frame, so every view that needs the answer asks here.

use argus_desktop_core::connection::page::{Rail, Stage};
use argus_desktop_core::panes::console::ConsolePlace;

/// What fills the stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Git,
    /// The console, as the Console stage or fullscreen.
    Console,
    /// The Workspace's grid of panes.
    Pane,
    /// The Workspace with no pane on screen: connecting, no project, or no pane.
    Empty,
    /// A stage whose surface is not in the window yet.
    Pending(Stage),
}

pub fn surface(rail: &Rail, console: ConsolePlace, git_up: bool) -> Surface {
    if git_up {
        return Surface::Git;
    }
    if matches!(console, ConsolePlace::Tab | ConsolePlace::Fullscreen) {
        return Surface::Console;
    }
    match rail.stage {
        Stage::Workspace if rail.shown.is_some() => Surface::Pane,
        Stage::Workspace => Surface::Empty,
        stage => Surface::Pending(stage),
    }
}

/// Which parts of the window's frame are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    /// The sidebar: put away while the pane fills the window (`body.filled .rail`). The strip
    /// stays, since it holds the window's buttons.
    pub rail: bool,
    /// The sidebar's nav and foot, and the docks: put away by a fullscreen console, which keeps
    /// the sidebar's projects and the strip's buttons (`body.console-fullscreen`).
    pub furniture: bool,
}

pub fn frame(rail: &Rail, console: ConsolePlace) -> Frame {
    Frame { rail: !rail.filled, furniture: console != ConsolePlace::Fullscreen }
}

/// Whether a window `width` logical pixels wide is narrow: the rail folds, the right dock is a
/// drawer, and the strip says the pane's status. The page's `max-width` is inclusive.
pub fn narrow(width: f32, below: f32) -> bool {
    width <= below
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::Shown;

    fn shown() -> Shown {
        Shown {
            pane: 1,
            repository: "argus".into(),
            checkout: "main".into(),
            branch: "main".into(),
            agent: false,
            browser: false,
            title: "zsh".into(),
            status: "idle",
            resumed: None,
        }
    }

    fn rail(stage: Stage, pane: bool) -> Rail {
        Rail { stage, shown: pane.then(shown), ..Rail::default() }
    }

    #[test]
    fn the_workspace_is_its_pane_or_the_empty_stage() {
        assert_eq!(surface(&rail(Stage::Workspace, true), ConsolePlace::Hidden, false), Surface::Pane);
        assert_eq!(surface(&rail(Stage::Workspace, false), ConsolePlace::Hidden, false), Surface::Empty);
        assert_eq!(surface(&rail(Stage::Workspace, true), ConsolePlace::Docked, false), Surface::Pane, "a docked console is beside the stage");
    }

    #[test]
    fn the_console_is_the_stage_as_a_tab_or_fullscreen_whatever_is_under_it() {
        for place in [ConsolePlace::Tab, ConsolePlace::Fullscreen] {
            assert_eq!(surface(&rail(Stage::Workspace, true), place, false), Surface::Console);
        }
    }

    #[test]
    fn git_takes_the_stage_from_everything() {
        for place in [ConsolePlace::Hidden, ConsolePlace::Tab, ConsolePlace::Fullscreen] {
            assert_eq!(surface(&rail(Stage::Workspace, true), place, true), Surface::Git);
        }
        assert_eq!(surface(&rail(Stage::Checkouts, false), ConsolePlace::Hidden, true), Surface::Git);
    }

    #[test]
    fn a_stage_without_a_surface_yet_is_pending_by_name() {
        for stage in [Stage::Feature, Stage::Checkouts, Stage::Composer, Stage::Extension, Stage::View, Stage::Code, Stage::Browsers] {
            assert_eq!(surface(&rail(stage, true), ConsolePlace::Hidden, false), Surface::Pending(stage));
        }
    }

    #[test]
    fn filling_the_window_puts_the_sidebar_away_and_a_fullscreen_console_its_furniture() {
        let mut filled = rail(Stage::Workspace, true);
        assert_eq!(frame(&filled, ConsolePlace::Hidden), Frame { rail: true, furniture: true });
        filled.filled = true;
        assert_eq!(frame(&filled, ConsolePlace::Hidden), Frame { rail: false, furniture: true });
        assert_eq!(frame(&rail(Stage::Workspace, true), ConsolePlace::Fullscreen), Frame { rail: true, furniture: false });
        assert_eq!(frame(&rail(Stage::Workspace, true), ConsolePlace::Tab), Frame { rail: true, furniture: true }, "the tab keeps the nav");
    }

    #[test]
    fn the_window_is_narrow_at_the_breakpoint_itself() {
        assert!(narrow(1200.0, 1200.0));
        assert!(narrow(720.0, 1200.0));
        assert!(!narrow(1200.5, 1200.0));
    }
}
