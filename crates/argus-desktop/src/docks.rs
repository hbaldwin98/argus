//! Where each panel sits: which dock — left, right or bottom — holds it
//! among that dock's tabs, which tab each dock shows, whether each dock is
//! open, and how big; and whether that changed since `desktop.json` kept it.
//!
//! A panel is always in exactly one dock, shown or not, so hiding one and
//! showing it again finds it where the person last put it. A dock shows one
//! of its tabs at a time, as an IDE's tool windows do. The stages and
//! fullscreen are not docks: a panel the Console stage or fullscreen takes
//! keeps its dock to go back to, and what each panel does there is its own
//! module's.

use serde::{Deserialize, Serialize};

use crate::dragged_width::Bounds;

/// The edges of the stage a panel docks against. The sidebar keeps the
/// window's own left edge, so the left dock is between it and the stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Dock {
    Left,
    Right,
    Bottom,
}

/// What can be docked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Panel {
    Console,
}

impl Panel {
    const ALL: [Panel; 1] = [Panel::Console];

    /// Where a panel goes before the person moves it.
    fn home(self) -> Dock {
        match self {
            Panel::Console => Dock::Bottom,
        }
    }
}

/// A bottom dock too short for a prompt, or a side one too narrow for a
/// shell's line, is no use; past the maximum the stage has no room left.
pub const BOTTOM: Bounds = Bounds { min: 120, max: 1200 };
pub const SIDE: Bounds = Bounds { min: 280, max: 1600 };

fn bounds(dock: Dock) -> Bounds {
    match dock {
        Dock::Bottom => BOTTOM,
        Dock::Left | Dock::Right => SIDE,
    }
}

/// One dock as it is kept and as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DockLayout {
    /// In the order the person put them there.
    #[serde(default)]
    pub tabs: Vec<Panel>,
    /// The tab the dock shows, one of `tabs`.
    #[serde(default)]
    pub active: Option<Panel>,
    #[serde(default)]
    pub open: bool,
    /// The bottom dock's height or a side dock's width, in CSS pixels.
    #[serde(default)]
    pub size: u32,
}

impl DockLayout {
    fn sized(size: u32) -> DockLayout {
        DockLayout { tabs: Vec::new(), active: None, open: false, size }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DocksLayout {
    #[serde(default = "side")]
    pub left: DockLayout,
    #[serde(default = "side")]
    pub right: DockLayout,
    #[serde(default = "bottom")]
    pub bottom: DockLayout,
}

fn side() -> DockLayout {
    DockLayout::sized(520)
}

fn bottom() -> DockLayout {
    DockLayout::sized(280)
}

/// Every dock closed: a window that never opens a panel is not given one.
impl Default for DocksLayout {
    fn default() -> Self {
        DocksLayout { left: side(), right: side(), bottom: bottom() }
    }
}

impl DocksLayout {
    pub fn dock(&self, dock: Dock) -> &DockLayout {
        match dock {
            Dock::Left => &self.left,
            Dock::Right => &self.right,
            Dock::Bottom => &self.bottom,
        }
    }

    pub fn dock_mut(&mut self, dock: Dock) -> &mut DockLayout {
        match dock {
            Dock::Left => &mut self.left,
            Dock::Right => &mut self.right,
            Dock::Bottom => &mut self.bottom,
        }
    }
}

const DOCKS: [Dock; 3] = [Dock::Left, Dock::Right, Dock::Bottom];

pub struct Docks {
    layout: DocksLayout,
    /// The file has not kept the layout since it changed.
    unsaved: bool,
}

impl Docks {
    /// A hand-edited file is made whole: a panel in two docks stays in the
    /// first, a panel in none goes home, a size is held to its bounds, and
    /// a dock with nothing in it is closed.
    pub fn new(saved: DocksLayout) -> Docks {
        let mut layout = saved;
        let mut placed = Vec::new();
        for dock in DOCKS {
            let d = layout.dock_mut(dock);
            d.tabs.retain(|panel| {
                let first = !placed.contains(panel);
                placed.push(*panel);
                first
            });
            let bounds = bounds(dock);
            d.size = d.size.clamp(bounds.min, bounds.max);
        }
        for panel in Panel::ALL.into_iter().filter(|p| !placed.contains(p)) {
            layout.dock_mut(panel.home()).tabs.push(panel);
        }
        for dock in DOCKS {
            let d = layout.dock_mut(dock);
            if !d.active.is_some_and(|active| d.tabs.contains(&active)) {
                d.active = d.tabs.first().copied();
            }
            d.open &= d.active.is_some();
        }
        Docks { layout, unsaved: false }
    }

    pub fn dock_of(&self, panel: Panel) -> Dock {
        DOCKS.into_iter().find(|&dock| self.layout.dock(dock).tabs.contains(&panel)).unwrap_or(panel.home())
    }

    /// Whether the panel is on screen in its dock: the dock open and
    /// showing its tab.
    pub fn shows(&self, panel: Panel) -> bool {
        let dock = self.layout.dock(self.dock_of(panel));
        dock.open && dock.active == Some(panel)
    }

    /// Opens the panel's dock on the panel's tab.
    pub fn show(&mut self, panel: Panel) {
        let dock = self.dock_of(panel);
        self.change(|layout| {
            let d = layout.dock_mut(dock);
            d.active = Some(panel);
            d.open = true;
        });
    }

    /// Closes the panel's dock, if it is the tab the dock shows; another
    /// tab on screen there stays.
    pub fn hide(&mut self, panel: Panel) {
        if self.shows(panel) {
            let dock = self.dock_of(panel);
            self.change(|layout| layout.dock_mut(dock).open = false);
        }
    }

    /// Moves the panel to the end of `to`'s tabs and shows it there. The
    /// dock it leaves shows the tab before it, or closes once empty.
    pub fn move_to(&mut self, panel: Panel, to: Dock) {
        let from = self.dock_of(panel);
        if from != to {
            self.change(|layout| {
                let d = layout.dock_mut(from);
                let at = d.tabs.iter().position(|&p| p == panel).unwrap_or(0);
                d.tabs.retain(|&p| p != panel);
                if d.active == Some(panel) {
                    d.active = d.tabs.get(at.saturating_sub(1)).copied();
                }
                d.open &= d.active.is_some();
                layout.dock_mut(to).tabs.push(panel);
            });
        }
        self.show(panel);
    }

    pub fn set_size(&mut self, dock: Dock, size: u32) {
        let bounds = bounds(dock);
        self.change(|layout| layout.dock_mut(dock).size = size.clamp(bounds.min, bounds.max));
    }

    pub fn layout(&self) -> &DocksLayout {
        &self.layout
    }

    /// Whether the layout changed since last asked, so it is saved.
    pub fn take_unsaved(&mut self) -> bool {
        std::mem::take(&mut self.unsaved)
    }

    fn change(&mut self, f: impl FnOnce(&mut DocksLayout)) {
        let before = self.layout.clone();
        f(&mut self.layout);
        self.unsaved |= self.layout != before;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_starts_hidden_in_its_home_dock() {
        let docks = Docks::new(DocksLayout::default());
        assert_eq!(docks.dock_of(Panel::Console), Dock::Bottom);
        assert!(!docks.shows(Panel::Console));
        assert_eq!(docks.layout().bottom.active, Some(Panel::Console), "the tab it opens on");
    }

    #[test]
    fn hiding_a_panel_and_showing_it_again_finds_it_where_it_was_put() {
        let mut docks = Docks::new(DocksLayout::default());
        docks.move_to(Panel::Console, Dock::Right);
        assert!(docks.shows(Panel::Console));
        assert_eq!(docks.layout().bottom.tabs, [], "it left the bottom");
        docks.hide(Panel::Console);
        assert!(!docks.shows(Panel::Console));
        assert!(!docks.layout().right.open);
        docks.show(Panel::Console);
        assert_eq!((docks.dock_of(Panel::Console), docks.shows(Panel::Console)), (Dock::Right, true));
    }

    #[test]
    fn the_dock_a_panel_leaves_closes_once_empty() {
        let mut docks = Docks::new(DocksLayout::default());
        docks.show(Panel::Console);
        docks.move_to(Panel::Console, Dock::Left);
        assert!(!docks.layout().bottom.open);
        assert_eq!(docks.layout().bottom.active, None);
        assert!(docks.layout().left.open);
    }

    #[test]
    fn a_layout_is_saved_once_per_change_and_sizes_are_held_to_their_bounds() {
        let mut docks = Docks::new(DocksLayout::default());
        docks.set_size(Dock::Bottom, 10);
        docks.set_size(Dock::Right, 10_000);
        assert_eq!(docks.layout().bottom.size, BOTTOM.min);
        assert_eq!(docks.layout().right.size, SIDE.max);
        assert!(docks.take_unsaved());
        assert!(!docks.take_unsaved());
        docks.set_size(Dock::Right, SIDE.max);
        docks.hide(Panel::Console);
        assert!(!docks.take_unsaved(), "the same size, and a panel hidden already");
    }

    #[test]
    fn a_hand_edited_layout_is_made_whole() {
        let docks = Docks::new(DocksLayout {
            left: DockLayout { tabs: vec![Panel::Console], active: None, open: true, size: 1 },
            right: DockLayout { tabs: vec![Panel::Console], active: Some(Panel::Console), open: true, size: 600 },
            bottom: DockLayout { open: true, ..bottom() },
        });
        assert_eq!(docks.dock_of(Panel::Console), Dock::Left, "the first dock keeps it");
        assert!(docks.shows(Panel::Console), "its only tab is the one shown");
        assert_eq!(docks.layout().left.size, SIDE.min);
        assert_eq!(docks.layout().right, DockLayout { tabs: vec![], active: None, open: false, size: 600 });
        assert!(!docks.layout().bottom.open, "nothing in it");

        let docks = Docks::new(DocksLayout { left: side(), right: side(), bottom: DockLayout::sized(280) });
        assert_eq!(docks.dock_of(Panel::Console), Dock::Bottom, "a panel in no dock goes home");
    }

    #[test]
    fn the_layout_reads_as_the_page_and_the_file_name_it() {
        let mut docks = Docks::new(DocksLayout::default());
        docks.move_to(Panel::Console, Dock::Right);
        let json = serde_json::to_value(docks.layout()).unwrap();
        assert_eq!(json["right"]["tabs"][0], "console");
        assert_eq!(json["right"]["active"], "console");
        assert_eq!(json["right"]["open"], true);
        let back: DocksLayout = serde_json::from_value(json).unwrap();
        assert_eq!(&back, docks.layout());
        let older: DocksLayout = serde_json::from_str("{}").unwrap();
        assert_eq!(older, DocksLayout::default());
    }
}
