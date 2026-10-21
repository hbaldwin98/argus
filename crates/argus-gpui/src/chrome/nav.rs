//! The sidebar's way between stages: which items there are, which one the
//! stage on screen lights, and the call each press makes.
//!
//! The calls are the page's (`main.ts`): Workspace is the Feature stage
//! shut, so it sends `show_features{open:false}`.

use argus_desktop_core::connection::page::{Call, Stage};
use argus_desktop_core::panes::console::ConsoleGesture;
use gpui_kit::component::IconName;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Search,
    New,
    Workspace,
    Features,
    Checkouts,
    Code,
    Console,
}

impl Nav {
    pub const ALL: [Nav; 7] =
        [Nav::Search, Nav::New, Nav::Workspace, Nav::Features, Nav::Checkouts, Nav::Code, Nav::Console];

    pub fn label(self) -> &'static str {
        match self {
            Nav::Search => "Search",
            Nav::New => "New",
            Nav::Workspace => "Workspace",
            Nav::Features => "Features",
            Nav::Checkouts => "Checkouts",
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
            Nav::Code => "nav-code",
            Nav::Console => "nav-console",
        }
    }

    /// The bundled Lucide set has no git or code glyph; the nearest one stands in.
    pub fn icon(self) -> IconName {
        match self {
            Nav::Search => IconName::Search,
            Nav::New => IconName::Plus,
            Nav::Workspace => IconName::LayoutDashboard,
            Nav::Features => IconName::Star,
            Nav::Checkouts => IconName::HardDrive,
            Nav::Code => IconName::FileText,
            Nav::Console => IconName::SquareTerminal,
        }
    }

    pub fn call(self) -> Call {
        match self {
            Nav::Search => Call::Palette { open: true },
            Nav::New => Call::ShowComposer { open: true },
            Nav::Workspace => Call::ShowFeatures { open: false },
            Nav::Features => Call::ShowFeatures { open: true },
            Nav::Checkouts => Call::ShowCheckouts { open: true },
            Nav::Code => Call::ShowCode { open: true },
            Nav::Console => Call::Console { gesture: ConsoleGesture::Tab },
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
    fn workspace_is_the_feature_stage_shut() {
        assert!(matches!(Nav::Workspace.call(), Call::ShowFeatures { open: false }));
        assert!(matches!(Nav::Features.call(), Call::ShowFeatures { open: true }));
    }
}
