//! The console: shells that belong to no checkout, as the window holds
//! them — where it is (docked under a stage, the Console stage, fullscreen
//! beside the sidebar, or hidden), how tall it is docked, the daemon's
//! consoles and the one its tabs have chosen — and the page it draws.
//!
//! The consoles are the daemon's `ServerMsg::Consoles`, never predicted: a
//! tab appears once the daemon says the console exists and goes once it
//! says it ended. Whether it is docked and its height are remembered in
//! `desktop.json`; fullscreen and the chosen tab last as long as the run.
//! Which pane the window's one canvas draws is `shown`'s, and the session
//! decides when it moves here; this module only says where the console is.

use argus_protocol::{PaneId, PaneInfo};

use crate::page::{ConsolePage, ConsolePlace, ConsoleTab, Stage};
use crate::settings::ConsoleLayout;

/// The sash stops short of a console too short for a prompt, and of one
/// that leaves the stage above no room.
pub const MIN_HEIGHT: u32 = 120;
pub const MAX_HEIGHT: u32 = 1200;

pub struct Console {
    panes: Vec<PaneInfo>,
    chosen: Option<PaneId>,
    docked: bool,
    height: u32,
    fullscreen: bool,
    /// The file has not kept the layout since it changed.
    unsaved: bool,
}

impl Console {
    pub fn new(layout: ConsoleLayout) -> Console {
        Console {
            panes: Vec::new(),
            chosen: None,
            docked: layout.docked,
            height: layout.height.clamp(MIN_HEIGHT, MAX_HEIGHT),
            fullscreen: false,
            unsaved: false,
        }
    }

    /// The daemon's consoles, whole. The tab chosen stays chosen while it
    /// lasts; when it ends, the one started before it is, as closing a tab
    /// in an editor falls back to its neighbour.
    pub fn receive(&mut self, panes: Vec<PaneInfo>) {
        if let Some(chosen) = self.chosen.filter(|c| !panes.iter().any(|p| p.id == *c)) {
            let at = self.panes.iter().position(|p| p.id == chosen).unwrap_or(0);
            let before = self.panes[..at].iter().rev().chain(&self.panes[at..]);
            self.chosen = before.map(|p| p.id).find(|id| panes.iter().any(|p| p.id == *id));
        }
        self.panes = panes;
        if self.chosen.is_none() {
            self.chosen = self.panes.first().map(|p| p.id);
        }
    }

    pub fn holds(&self, pane: PaneId) -> bool {
        self.panes.iter().any(|p| p.id == pane)
    }

    pub fn chosen(&self) -> Option<PaneId> {
        self.chosen
    }

    /// Chooses a tab, if the daemon holds it.
    pub fn choose(&mut self, pane: PaneId) -> bool {
        let held = self.holds(pane);
        if held {
            self.chosen = Some(pane);
        }
        held
    }

    /// Where the console is with `stage` up. Fullscreen covers whichever
    /// stage is up, and leaving it shows that stage as it was; docked is
    /// under the three stages that read a project, not the composer.
    pub fn place(&self, stage: Stage) -> ConsolePlace {
        match stage {
            _ if self.fullscreen => ConsolePlace::Fullscreen,
            Stage::Console => ConsolePlace::Tab,
            Stage::Composer => ConsolePlace::Hidden,
            _ if self.docked => ConsolePlace::Docked,
            _ => ConsolePlace::Hidden,
        }
    }

    pub fn set_docked(&mut self, docked: bool) {
        if self.docked != docked {
            self.docked = docked;
            self.unsaved = true;
        }
    }

    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }

    pub fn set_height(&mut self, height: u32) {
        let height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
        if self.height != height {
            self.height = height;
            self.unsaved = true;
        }
    }

    pub fn layout(&self) -> ConsoleLayout {
        ConsoleLayout { docked: self.docked, height: self.height }
    }

    /// Whether the layout changed since last asked, so it is saved.
    pub fn take_unsaved(&mut self) -> bool {
        std::mem::take(&mut self.unsaved)
    }

    /// `showing` is the console on the window's canvas, if one is.
    pub fn page(&self, stage: Stage, showing: Option<PaneId>, can_spawn: bool) -> ConsolePage {
        ConsolePage {
            place: self.place(stage),
            height: self.height,
            tabs: self
                .panes
                .iter()
                .map(|p| ConsoleTab {
                    pane: p.id.0,
                    title: p.title.clone(),
                    status: p.loudest_state().status.word(),
                    on: Some(p.id) == self.chosen,
                })
                .collect(),
            showing: showing.filter(|pane| self.holds(*pane)).map(|pane| pane.0),
            can_spawn,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::pane;
    use argus_protocol::{PaneKind, PaneStatus};

    fn shells(ids: &[u64]) -> Vec<PaneInfo> {
        ids.iter().map(|&id| pane(id, PaneKind::Shell, "pwsh", PaneStatus::Idle)).collect()
    }

    fn hidden() -> Console {
        Console::new(ConsoleLayout::default())
    }

    #[test]
    fn the_first_console_is_chosen_and_a_closed_one_falls_back_to_the_one_before() {
        let mut console = hidden();
        console.receive(shells(&[5, 6, 7]));
        assert_eq!(console.chosen(), Some(PaneId(5)));
        assert!(console.choose(PaneId(7)));
        assert!(!console.choose(PaneId(9)), "not the daemon's");
        console.receive(shells(&[5, 6]));
        assert_eq!(console.chosen(), Some(PaneId(6)));
        console.receive(shells(&[]));
        assert_eq!(console.chosen(), None);
    }

    #[test]
    fn the_place_follows_the_stage_unless_fullscreen() {
        let mut console = hidden();
        assert_eq!(console.place(Stage::Workspace), ConsolePlace::Hidden);
        assert_eq!(console.place(Stage::Console), ConsolePlace::Tab);
        console.set_docked(true);
        for stage in [Stage::Workspace, Stage::Feature, Stage::Checkouts] {
            assert_eq!(console.place(stage), ConsolePlace::Docked, "{stage:?}");
        }
        assert_eq!(console.place(Stage::Composer), ConsolePlace::Hidden, "the composer is not one of the three");
        console.set_fullscreen(true);
        assert_eq!(console.place(Stage::Feature), ConsolePlace::Fullscreen);
        console.set_fullscreen(false);
        assert_eq!(console.place(Stage::Feature), ConsolePlace::Docked, "back where it was");
    }

    #[test]
    fn the_docked_layout_is_saved_once_and_held_to_the_sashs_bounds() {
        let mut console = Console::new(ConsoleLayout { docked: false, height: 9 });
        assert_eq!(console.layout().height, MIN_HEIGHT, "a hand-edited file too");
        console.set_docked(true);
        console.set_height(10_000);
        assert_eq!(console.layout(), ConsoleLayout { docked: true, height: MAX_HEIGHT });
        assert!(console.take_unsaved());
        assert!(!console.take_unsaved());
        console.set_fullscreen(true);
        console.set_height(MAX_HEIGHT);
        assert!(!console.take_unsaved(), "fullscreen and an unchanged height are not kept");
    }

    #[test]
    fn the_page_marks_the_chosen_tab_and_only_a_console_as_showing() {
        let mut console = hidden();
        console.receive(shells(&[5, 6]));
        console.choose(PaneId(6));
        let page = console.page(Stage::Console, Some(PaneId(6)), true);
        assert_eq!(page.place, ConsolePlace::Tab);
        assert_eq!(page.tabs.iter().map(|t| (t.pane, t.on)).collect::<Vec<_>>(), [(5, false), (6, true)]);
        assert_eq!(page.showing, Some(6));
        assert_eq!(console.page(Stage::Console, Some(PaneId(1)), true).showing, None, "a checkout's pane");
    }

    #[test]
    #[ignore]
    fn dump_console() {
        // The repo's dump-test convention, as text: there is no screen to
        // render a webview to here.
        let mut console = Console::new(ConsoleLayout { docked: true, height: 280 });
        let mut panes = shells(&[5, 6, 7]);
        panes[1].title = "cargo test".into();
        panes[2].status = PaneStatus::Working;
        console.receive(panes);
        console.choose(PaneId(6));
        for stage in [Stage::Workspace, Stage::Console, Stage::Composer] {
            // A hidden console never keeps the canvas: the session gives it back.
            let showing = (console.place(stage) != ConsolePlace::Hidden).then_some(PaneId(6));
            let page = console.page(stage, showing, true);
            println!("{stage:?} stage: console {:?}, {}px", page.place, page.height);
            if page.place == ConsolePlace::Hidden {
                println!("  sidebar foot: ›_ {} consoles", page.tabs.len());
                continue;
            }
            let tabs: Vec<String> = page
                .tabs
                .iter()
                .map(|t| format!("{}{} {}{}", if t.on { "[" } else { " " }, t.title, t.status, if t.on { "]" } else { " " }))
                .collect();
            println!("  ›_ Console  {}  +", tabs.join(" "));
            println!("  canvas: {}", page.showing.map_or("the Workspace's pane".into(), |p| format!("console {p}")));
        }
        console.set_fullscreen(true);
        println!("fullscreen: {:?}", console.page(Stage::Feature, None, true).place);
    }
}
