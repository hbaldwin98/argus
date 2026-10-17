//! The console: shells that belong to no checkout, as the window holds
//! them — where it is (docked, the Console stage, fullscreen beside the
//! sidebar, or hidden), the daemon's consoles, the tab chosen for each side
//! of a split — and the page it draws, and what the page last heard.
//!
//! The consoles are the daemon's `ServerMsg::Consoles`, never predicted: a
//! tab appears once the daemon says the console exists and goes once it
//! says it ended. Which dock it is in, whether that dock shows it, and how
//! big it is are `layout::docks`'s, remembered in `desktop.json`;
//! fullscreen, the split and the tabs chosen last as long as the run: a
//! daemon restarted brings its consoles back as new shells under new ids,
//! so a split kept for the next run would not find its two. Which panes are
//! on screen, and which has the keys, is `shown`'s, and the session decides
//! when they move here; this module only says what the console would show.

use argus_protocol::{PaneId, PaneInfo};
use serde::{Deserialize, Serialize};

use crate::connection::page::{Event, Stage};
use crate::connection::sent::Sent;

/// What was done to the console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum ConsoleGesture {
    /// Shows it docked, or hides it wherever it is.
    Toggle,
    /// The Console stage: the console fills the stage.
    Tab,
    /// In its dock, from hidden, the Console stage or fullscreen.
    Dock,
    /// Fullscreen beside the sidebar, or back to where it was.
    Fullscreen,
    Hide,
    /// One of its tabs, or a split side's header: that console on screen,
    /// with the keys.
    Select { pane: u64 },
    /// "+": a new console, shown once the daemon has made it.
    Spawn,
    /// A tab's close: the daemon ends that console.
    Close { pane: u64 },
    /// Splits the console in two, or puts the split away keeping the side
    /// with the keys.
    Split,
    /// A split side's close: that side leaves the split, its console stays
    /// a tab.
    CloseSide { pane: u64 },
}

/// Where the console is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ConsolePlace {
    /// Only the sidebar's foot says it is there.
    #[default]
    Hidden,
    /// In its dock beside the Workspace, Feature or Checkouts stage.
    Docked,
    /// The Console stage.
    Tab,
    /// The whole window but the sidebar's projects: the stages' nav and
    /// the sidebar's foot are put away.
    Fullscreen,
}

/// The console as the page draws it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ConsolePage {
    pub place: ConsolePlace,
    /// A tab per console the daemon holds, in the order they were started.
    pub tabs: Vec<ConsoleTab>,
    /// The console whose screen the window's canvas is drawing, on the
    /// first side. The canvas is the Workspace's or the console's, so
    /// while this is set the Workspace stage has none.
    pub showing: Option<u64>,
    /// The console on the second side, while the console is split and
    /// has the canvas.
    pub beside: Option<u64>,
    /// Which of `showing` and `beside` has the keys, and is typed in.
    pub typing: Option<u64>,
    /// The daemon takes `SpawnConsole`; an older one offers no "+".
    pub can_spawn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ConsoleTab {
    pub pane: u64,
    pub title: String,
    #[cfg_attr(test, ts(type = "\"idle\" | \"working\" | \"waiting\" | \"needs-review\" | \"done\" | \"failed\" | \"exited\""))]
    pub status: &'static str,
    /// The tab chosen for one side: shown when the console has the canvas.
    pub on: bool,
}

#[derive(Default)]
pub struct Console {
    panes: Vec<PaneInfo>,
    /// The tab on the first side, the only one when it is not split.
    chosen: Option<PaneId>,
    /// The tab on the second side while it is split: never `chosen`, and
    /// never without it.
    beside: Option<PaneId>,
    fullscreen: bool,
    sent: Sent<ConsolePage>,
}

/// What the console's sides are drawing and which has the keys, as
/// `shown` holds them.
#[derive(Debug, Clone, Copy, Default)]
pub struct OnScreen {
    pub first: Option<PaneId>,
    pub second: Option<PaneId>,
    pub keys: Option<PaneId>,
}

impl Console {
    /// The daemon's consoles, whole. A side's tab that ended takes the
    /// split with it, the other side filling the console; a lone tab that
    /// ended falls back to the one started before it, as closing a tab in
    /// an editor falls back to its neighbour.
    pub fn receive(&mut self, panes: Vec<PaneInfo>) {
        let held = |id: PaneId| panes.iter().any(|p| p.id == id);
        if self.beside.is_some_and(|b| !held(b)) {
            self.beside = None;
        }
        if let Some(chosen) = self.chosen.filter(|c| !held(*c)) {
            self.chosen = match self.beside.take() {
                Some(beside) => Some(beside),
                None => {
                    let at = self.panes.iter().position(|p| p.id == chosen).unwrap_or(0);
                    let before = self.panes[..at].iter().rev().chain(&self.panes[at..]);
                    before.map(|p| p.id).find(|id| held(*id))
                }
            };
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

    pub fn beside(&self) -> Option<PaneId> {
        self.beside
    }

    /// Chooses a tab for the first side, if the daemon holds it. The
    /// second side's tab chosen for the first trades places with it.
    pub fn choose(&mut self, pane: PaneId) -> bool {
        let held = self.holds(pane);
        if held {
            if self.beside == Some(pane) {
                self.beside = self.chosen;
            }
            self.chosen = Some(pane);
        }
        held
    }

    /// Splits the console with `pane` on the second side, or chooses it
    /// for the second side of one already split. With no tab chosen there
    /// is nothing to split beside, and it is chosen instead.
    pub fn set_beside(&mut self, pane: PaneId) -> bool {
        if !self.holds(pane) {
            return false;
        }
        match self.chosen {
            None => self.chosen = Some(pane),
            Some(chosen) if chosen == pane => {}
            Some(_) => self.beside = Some(pane),
        }
        true
    }

    /// The first tab that is on neither side, to split beside.
    pub fn unshown(&self) -> Option<PaneId> {
        self.panes.iter().map(|p| p.id).find(|&id| Some(id) != self.chosen && Some(id) != self.beside)
    }

    /// Puts the split away, `keep` filling the console; both stay tabs.
    pub fn unsplit(&mut self, keep: PaneId) {
        if self.beside == Some(keep) {
            self.chosen = Some(keep);
        }
        self.beside = None;
    }

    /// Where the console is with `stage` up, `docked` while its dock shows
    /// it. Fullscreen covers whichever stage is up, and leaving it shows
    /// that stage as it was; docked is beside the three stages that read a
    /// project, not the composer.
    pub fn place(&self, stage: Stage, docked: bool) -> ConsolePlace {
        match stage {
            _ if self.fullscreen => ConsolePlace::Fullscreen,
            Stage::Console => ConsolePlace::Tab,
            Stage::Composer => ConsolePlace::Hidden,
            _ if docked => ConsolePlace::Docked,
            _ => ConsolePlace::Hidden,
        }
    }

    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }

    /// What the page has not heard of the console.
    pub fn take(&mut self, place: ConsolePlace, on_screen: OnScreen, can_spawn: bool) -> Option<Event> {
        let page = self.page(place, on_screen, can_spawn);
        self.sent.take(Some(page)).map(|page| Event::Console(Box::new(page)))
    }

    /// The page reloaded and holds nothing: the console is sent again.
    pub fn reload(&mut self) {
        self.sent.forget();
    }

    /// `on_screen` is what the window's sides draw; only the consoles
    /// among them are the console's to name.
    fn page(&self, place: ConsolePlace, on_screen: OnScreen, can_spawn: bool) -> ConsolePage {
        let ours = |pane: Option<PaneId>| pane.filter(|pane| self.holds(*pane)).map(|pane| pane.0);
        let showing = ours(on_screen.first);
        let beside = showing.and(ours(on_screen.second));
        ConsolePage {
            place,
            tabs: self
                .panes
                .iter()
                .map(|p| ConsoleTab {
                    pane: p.id.0,
                    title: p.title.clone(),
                    status: p.loudest_state().status.word(),
                    on: Some(p.id) == self.chosen || Some(p.id) == self.beside,
                })
                .collect(),
            showing,
            beside,
            typing: ours(on_screen.keys).filter(|&keys| Some(keys) == showing || Some(keys) == beside),
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
        Console::default()
    }

    fn alone(pane: u64) -> OnScreen {
        OnScreen { first: Some(PaneId(pane)), second: None, keys: Some(PaneId(pane)) }
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
    fn a_split_sides_console_ending_leaves_the_other_filling_the_console() {
        let mut console = hidden();
        console.receive(shells(&[5, 6, 7]));
        assert!(console.set_beside(PaneId(7)));
        assert_eq!(console.unshown(), Some(PaneId(6)));
        console.receive(shells(&[5, 6]));
        assert_eq!((console.chosen(), console.beside()), (Some(PaneId(5)), None), "the second side went");

        console.set_beside(PaneId(6));
        console.receive(shells(&[6]));
        assert_eq!((console.chosen(), console.beside()), (Some(PaneId(6)), None), "the first side went");
    }

    #[test]
    fn a_tab_chosen_for_the_other_side_trades_places_and_unsplitting_keeps_the_one_asked() {
        let mut console = hidden();
        console.receive(shells(&[5, 6]));
        assert!(console.set_beside(PaneId(5)), "already the first side's: nothing to split");
        assert_eq!(console.beside(), None);
        console.set_beside(PaneId(6));
        console.choose(PaneId(6));
        assert_eq!((console.chosen(), console.beside()), (Some(PaneId(6)), Some(PaneId(5))));
        assert_eq!(console.unshown(), None, "both are shown");
        console.unsplit(PaneId(5));
        assert_eq!((console.chosen(), console.beside()), (Some(PaneId(5)), None));
    }

    #[test]
    fn the_place_follows_the_stage_unless_fullscreen() {
        let mut console = hidden();
        assert_eq!(console.place(Stage::Workspace, false), ConsolePlace::Hidden);
        assert_eq!(console.place(Stage::Console, false), ConsolePlace::Tab);
        for stage in [Stage::Workspace, Stage::Feature, Stage::Checkouts] {
            assert_eq!(console.place(stage, true), ConsolePlace::Docked, "{stage:?}");
        }
        assert_eq!(console.place(Stage::Composer, true), ConsolePlace::Hidden, "the composer is not one of the three");
        console.set_fullscreen(true);
        assert_eq!(console.place(Stage::Feature, true), ConsolePlace::Fullscreen);
        assert_eq!(console.place(Stage::Feature, false), ConsolePlace::Fullscreen, "from hidden too");
        console.set_fullscreen(false);
        assert_eq!(console.place(Stage::Feature, true), ConsolePlace::Docked, "back where it was");
    }

    #[test]
    fn the_page_marks_the_chosen_tab_and_only_a_console_as_showing() {
        let mut console = hidden();
        console.receive(shells(&[5, 6]));
        console.choose(PaneId(6));
        let page = console.page(ConsolePlace::Tab, alone(6), true);
        assert_eq!(page.place, ConsolePlace::Tab);
        assert_eq!(page.tabs.iter().map(|t| (t.pane, t.on)).collect::<Vec<_>>(), [(5, false), (6, true)]);
        assert_eq!((page.showing, page.beside, page.typing), (Some(6), None, Some(6)));
        let page = console.page(ConsolePlace::Tab, alone(1), true);
        assert_eq!((page.showing, page.typing), (None, None), "a checkout's pane");
    }

    #[test]
    fn the_page_names_both_sides_of_a_split_and_the_one_with_the_keys() {
        let mut console = hidden();
        console.receive(shells(&[5, 6, 7]));
        console.set_beside(PaneId(7));
        let on_screen = OnScreen { first: Some(PaneId(5)), second: Some(PaneId(7)), keys: Some(PaneId(7)) };
        let page = console.page(ConsolePlace::Tab, on_screen, true);
        assert_eq!((page.showing, page.beside, page.typing), (Some(5), Some(7), Some(7)));
        assert_eq!(page.tabs.iter().filter(|t| t.on).map(|t| t.pane).collect::<Vec<_>>(), [5, 7], "a tab per side");
    }

    #[test]
    #[ignore]
    fn dump_console() {
        // The repo's dump-test convention, as text: there is no screen to
        // render a webview to here.
        let mut console = hidden();
        let mut panes = shells(&[5, 6, 7]);
        panes[1].title = "cargo test".into();
        panes[2].status = PaneStatus::Working;
        console.receive(panes);
        console.choose(PaneId(6));
        let tabs = |page: &ConsolePage| -> String {
            page.tabs
                .iter()
                .map(|t| format!("{}{} {}{}", if t.on { "[" } else { " " }, t.title, t.status, if t.on { "]" } else { " " }))
                .collect::<Vec<_>>()
                .join(" ")
        };
        for stage in [Stage::Workspace, Stage::Console, Stage::Composer] {
            // A hidden console lets its panes go: the session closes its sides.
            let place = console.place(stage, true);
            let on_screen = if place == ConsolePlace::Hidden { OnScreen::default() } else { alone(6) };
            let page = console.page(place, on_screen, true);
            println!("{stage:?} stage: console {:?}", page.place);
            if page.place == ConsolePlace::Hidden {
                println!("  sidebar foot: ›_ {} consoles", page.tabs.len());
                continue;
            }
            println!("  ›_ Console  {}  +   [|] split", tabs(&page));
            let under = if page.place == ConsolePlace::Docked { ", in its dock beside the Workspace's pane" } else { "" };
            println!("  screen: {}", page.showing.map_or("none".into(), |p| format!("console {p}{under}")));
        }
        console.set_fullscreen(true);
        println!("fullscreen: {:?}", console.place(Stage::Feature, true));

        console.set_fullscreen(false);
        console.set_beside(PaneId(7));
        let on_screen = OnScreen { first: Some(PaneId(6)), second: Some(PaneId(7)), keys: Some(PaneId(7)) };
        let page = console.page(ConsolePlace::Tab, on_screen, true);
        println!("split, Console stage: console {:?}", page.place);
        println!("  ›_ Console  {}  +   [|] split (on)", tabs(&page));
        let side = |pane: Option<u64>| {
            let pane = pane.expect("both sides");
            let title = &page.tabs.iter().find(|t| t.pane == pane).expect("a tab").title;
            let keys = if page.typing == Some(pane) { "▍keys" } else { "     " };
            format!("{keys} › {title} (console {pane}) ✕")
        };
        println!("  ┌ {:<34}┬ {}", side(page.showing), side(page.beside));
        let (first, second) = ("first side's screen", "second side's screen");
        println!("  │ {first:<34}│ {second}");
    }
}
