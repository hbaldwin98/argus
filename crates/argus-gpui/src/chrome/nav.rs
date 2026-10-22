//! The sidebar's way between stages: which items there are, which one the
//! stage on screen lights, and the call each press makes.
//!
//! The calls are the page's (`main.ts`): Workspace is the Feature stage
//! shut, so it sends `show_features{open:false}`. Git is not a stage of the
//! session's: it asks for a review, which names a checkout only the window
//! knows, so its calls are `review::entry`'s.

use argus_desktop_core::connection::page::{Call, Stage};
use argus_desktop_core::panes::console::ConsoleGesture;
use argus_desktop_core::keys::keymap::Action;

use crate::icons::Glyph;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Search,
    New,
    Workspace,
    Features,
    Checkouts,
    Git,
    Code,
    Console,
}

impl Nav {
    pub const ALL: [Nav; 8] = [
        Nav::Search,
        Nav::New,
        Nav::Workspace,
        Nav::Features,
        Nav::Checkouts,
        Nav::Git,
        Nav::Code,
        Nav::Console,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Nav::Search => "Search",
            Nav::New => "New",
            Nav::Workspace => "Workspace",
            Nav::Features => "Features",
            Nav::Checkouts => "Checkouts",
            Nav::Git => "Git",
            Nav::Code => "Code",
            Nav::Console => "Console",
        }
    }

    /// The stable id a test finds the item by.
    pub fn id(self) -> &'static str {
        match self {
            Nav::Search => "nav-search",
            Nav::New => "nav-new",
            Nav::Workspace => "nav-workspace",
            Nav::Features => "nav-feature",
            Nav::Checkouts => "nav-checkouts",
            Nav::Git => "nav-git",
            Nav::Code => "nav-code",
            Nav::Console => "nav-console",
        }
    }

    /// The page's glyph for the item. The page has no Git item (its history and
    /// review are dialogs over a checkout), so Git gets a commit mark in the same hand.
    pub fn glyph(self) -> Glyph {
        match self {
            Nav::Search => Glyph::Search,
            Nav::New => Glyph::Plus,
            Nav::Workspace => Glyph::Workspace,
            Nav::Features => Glyph::Features,
            Nav::Checkouts => Glyph::Branch,
            Nav::Git => Glyph::Commit,
            Nav::Code => Glyph::Code,
            Nav::Console => Glyph::Console,
        }
    }

    /// The keymap action whose keys the page writes beside the item (`drawNav`).
    pub fn action(self) -> Option<Action> {
        Some(match self {
            Nav::Search => Action::Palette,
            Nav::New => Action::Compose,
            Nav::Workspace => Action::Workspace,
            Nav::Features => Action::Feature,
            Nav::Checkouts => Action::Checkouts,
            Nav::Git => return None,
            Nav::Code => Action::Code,
            Nav::Console => Action::Console,
        })
    }

    /// The call a press sends, for every item but Git, whose call names a
    /// checkout and is `review::entry`'s.
    pub fn call(self) -> Option<Call> {
        Some(match self {
            Nav::Search => Call::Palette { open: true },
            Nav::New => Call::ShowComposer { open: true },
            Nav::Workspace => Call::ShowFeatures { open: false },
            Nav::Features => Call::ShowFeatures { open: true },
            Nav::Checkouts => Call::ShowCheckouts { open: true },
            Nav::Git => return None,
            Nav::Code => Call::ShowCode { open: true },
            Nav::Console => Call::Console { gesture: ConsoleGesture::Tab },
        })
    }

    /// Whether a press leaves the Git stage: every item that goes to a stage
    /// does. Search is a palette over whatever is up, and Git is the stage.
    pub fn leaves_git(self) -> bool {
        !matches!(self, Nav::Search | Nav::Git)
    }

    /// Whether the item is lit: Git while its stage is up, which the window
    /// draws over whichever stage the session is on, else the item `stage` lights.
    pub fn lit(self, stage: Stage, git_up: bool) -> bool {
        if git_up {
            self == Nav::Git
        } else {
            self.lit_by(stage)
        }
    }

    /// Whether `stage` is the one this item opens. Search is a palette over
    /// whatever is up, so it is never the stage's.
    pub fn lit_by(self, stage: Stage) -> bool {
        matches!(
            (self, stage),
            (Nav::New, Stage::Composer)
                | (Nav::Workspace, Stage::Workspace)
                | (Nav::Features, Stage::Feature)
                | (Nav::Checkouts, Stage::Checkouts)
                | (Nav::Code, Stage::Code)
                | (Nav::Console, Stage::Console)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_stage_lights_at_most_one_item() {
        for stage in [
            Stage::Workspace,
            Stage::Feature,
            Stage::Checkouts,
            Stage::Composer,
            Stage::Console,
            Stage::Extension,
            Stage::View,
            Stage::Code,
        ] {
            let lit = Nav::ALL.iter().filter(|nav| nav.lit_by(stage)).count();
            assert!(lit <= 1, "{stage:?} lit {lit} items");
        }
    }

    #[test]
    fn a_stage_lights_the_item_that_opens_it() {
        let lit = |stage| Nav::ALL.into_iter().find(|nav| nav.lit_by(stage));
        assert_eq!(lit(Stage::Workspace), Some(Nav::Workspace));
        assert_eq!(lit(Stage::Feature), Some(Nav::Features));
        assert_eq!(lit(Stage::Checkouts), Some(Nav::Checkouts));
        assert_eq!(lit(Stage::Composer), Some(Nav::New));
        assert_eq!(lit(Stage::Code), Some(Nav::Code));
        assert_eq!(lit(Stage::Console), Some(Nav::Console));
        assert_eq!(lit(Stage::View), None, "a page over a stage lights nothing");
    }

    #[test]
    fn git_is_lit_by_its_own_stage_over_whichever_stage_the_session_is_on() {
        for stage in [Stage::Workspace, Stage::Feature, Stage::Console] {
            let lit: Vec<Nav> = Nav::ALL.into_iter().filter(|nav| nav.lit(stage, true)).collect();
            assert_eq!(lit, [Nav::Git], "{stage:?}");
            assert!(!Nav::Git.lit(stage, false), "no stage of the session's lights Git");
        }
        assert!(Nav::Workspace.lit(Stage::Workspace, false));
    }

    #[test]
    fn every_stage_item_leaves_git_and_search_and_git_do_not() {
        let leaving: Vec<Nav> = Nav::ALL.into_iter().filter(|nav| nav.leaves_git()).collect();
        assert_eq!(leaving, [Nav::New, Nav::Workspace, Nav::Features, Nav::Checkouts, Nav::Code, Nav::Console]);
        for nav in leaving {
            assert!(nav.call().is_some(), "{nav:?} goes to a stage with a call");
        }
        assert!(Nav::Git.call().is_none(), "Git's call names a checkout the window knows");
    }

    #[test]
    fn workspace_is_the_feature_stage_shut() {
        assert!(matches!(Nav::Workspace.call(), Some(Call::ShowFeatures { open: false })));
        assert!(matches!(Nav::Features.call(), Some(Call::ShowFeatures { open: true })));
    }
}
