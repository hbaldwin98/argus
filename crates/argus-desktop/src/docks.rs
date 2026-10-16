//! Where each panel sits: which dock — left, right or bottom — holds it
//! among that dock's tabs, whether it opens as a page on the stage, or
//! floats over the stage as a dialog, where and how big, in what order;
//! which tab each dock shows, whether each dock is open, and how big;
//! whether that changed since `desktop.json` kept it, and what the page
//! last heard of it — each panel that exists told with what it can do
//! there, so the page never reads that from its name.
//!
//! A panel the person placed stays placed whether or not it shows, or even
//! exists: hidden and shown again it comes back where it was put, and an
//! extension's view gone with its extension finds its place again when the
//! extension comes back. So the docks keep every panel placed, and are told
//! which exist now; a dock shows one of those at a time, as an IDE's tool
//! windows do, and a dock with none of them is closed on screen. The
//! Console stage and fullscreen are not places: they take the console out
//! of its dock without moving it, and what each panel does there is its
//! own module's.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::dragged_width::Bounds;
use crate::page::Event;
use crate::sent::Sent;

/// What was done to the docks.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum DockGesture {
    /// The panel goes to `dock`, shown there.
    Move { panel: Panel, dock: Dock },
    /// A dock's tab picked: its panel on screen there.
    Show { panel: Panel },
    /// A panel's close: its dock closes, and the panel stays in it.
    Hide { panel: Panel },
    /// A dock's sash let go at `size` CSS pixels: the bottom's height, or
    /// a side's width.
    Size { dock: Dock, size: u32 },
    /// The panel out of its dock, floating over the stage as a dialog.
    Float { panel: Panel },
    /// A dialog let go after it was moved or resized.
    Bounds { panel: Panel, bounds: FloatBounds },
    /// A dialog pressed: it comes in front of the others.
    Raise { panel: Panel },
    /// The panel dropped on the stage: where it goes is the session's —
    /// the Console stage, an extension's page. One that cannot go stays.
    Stage { panel: Panel },
    /// A panel's close for good, which only a panel that `closes` has;
    /// any other stays where it is.
    Close { panel: Panel },
}

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

const DOCKS: [Dock; 3] = [Dock::Left, Dock::Right, Dock::Bottom];

/// What can be docked: the console, the views agents showed, each
/// extension's view by its extension and name, and each saved view by its
/// id. Named in the file and to the page as one string — `console`,
/// `views`, `extension:docker/images`, `saved:release-plan` — since the
/// last two are any of an open-ended set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, type = "string"))]
pub enum Panel {
    Console,
    Views,
    Extension(String),
    /// A saved view's panel, named by the view it opened with; the columns
    /// that follow it are `saved_views`'.
    Saved(String),
}

const EXTENSION: &str = "extension:";
const SAVED: &str = "saved:";

impl Panel {
    pub fn extension(extension: &str, view: &str) -> Panel {
        Panel::Extension(format!("{extension}/{view}"))
    }

    /// An extension's view's extension and name.
    pub fn extension_view(&self) -> Option<(&str, &str)> {
        match self {
            Panel::Extension(key) => key.split_once('/'),
            _ => None,
        }
    }

    /// Where a panel goes before the person moves it; an extension's view
    /// goes where its extension says, and a saved view's to the right, the
    /// first time it opens.
    fn home(&self) -> Option<Dock> {
        match self {
            Panel::Console => Some(Dock::Bottom),
            Panel::Views => Some(Dock::Right),
            Panel::Extension(_) | Panel::Saved(_) => None,
        }
    }

    /// Whether the panel can be a page on the stage. A saved view's columns
    /// scroll sideways in a dock; the stage has no use for them.
    fn has_page(&self) -> bool {
        matches!(self, Panel::Extension(_))
    }

    /// Whether the panel can float as a dialog: a saved view, read beside
    /// the one it was reached from without taking a dock from it. The
    /// console's terminal and the panels with a home stay docked.
    fn floats(&self) -> bool {
        matches!(self, Panel::Saved(_))
    }

    fn kind(&self) -> PanelKind {
        match self {
            Panel::Console => PanelKind::Console,
            Panel::Views => PanelKind::Views,
            Panel::Extension(_) => PanelKind::Extension,
            Panel::Saved(_) => PanelKind::Saved,
        }
    }

    /// Whether a drop on the stage takes the panel there. Every panel with
    /// a page does; so does the console, whose stage is the Console stage
    /// rather than a page, so it is never one of `staged`. No `_` arm, so a
    /// new kind of panel is decided here rather than defaulted.
    fn stages(&self) -> bool {
        match self {
            Panel::Console | Panel::Extension(_) => true,
            Panel::Views | Panel::Saved(_) => false,
        }
    }

    /// Whether its close takes it out for good: a saved view's panel goes
    /// with its columns. The console, the views and an extension's view
    /// are there while what they show is.
    fn closes(&self) -> bool {
        match self {
            Panel::Saved(_) => true,
            Panel::Console | Panel::Views | Panel::Extension(_) => false,
        }
    }

    fn told(&self) -> PresentPanel {
        PresentPanel { panel: self.clone(), kind: self.kind(), floats: self.floats(), stage: self.stages(), closes: self.closes() }
    }
}

impl fmt::Display for Panel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Panel::Console => f.write_str("console"),
            Panel::Views => f.write_str("views"),
            Panel::Extension(key) => write!(f, "{EXTENSION}{key}"),
            Panel::Saved(id) => write!(f, "{SAVED}{id}"),
        }
    }
}

impl Serialize for Panel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Panel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Panel, D::Error> {
        Panel::named(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl Panel {
    fn named(name: &str) -> Result<Panel, String> {
        match name {
            "console" => Ok(Panel::Console),
            "views" => Ok(Panel::Views),
            _ => match (name.strip_prefix(EXTENSION), name.strip_prefix(SAVED)) {
                (Some(key), _) if key.split_once('/').is_some_and(|(e, v)| !e.is_empty() && !v.is_empty()) => {
                    Ok(Panel::Extension(key.to_string()))
                }
                (_, Some(id)) if !id.is_empty() => Ok(Panel::Saved(id.to_string())),
                _ => Err(format!("no panel is called {name:?}")),
            },
        }
    }
}

/// Where a panel is: in a dock, a page of its own on the stage, or a
/// dialog floating over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Dock(Dock),
    Stage,
    Float,
}

/// Where a dialog sits over the work area and how big it is, in CSS
/// pixels from the area's top left. The page keeps a dialog on screen when
/// it draws one the window has since shrunk past.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FloatBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A dialog smaller than this shows a header and little under it.
pub const FLOAT_MIN: (u32, u32) = (280, 160);
const FLOAT_FIRST: FloatBounds = FloatBounds { x: 96, y: 64, width: 560, height: 520 };
/// How far each dialog opened sits from the one in front before it, so a
/// new one never hides the last exactly.
const FLOAT_STEP: i32 = 28;

impl FloatBounds {
    fn held(self) -> FloatBounds {
        FloatBounds { width: self.width.max(FLOAT_MIN.0), height: self.height.max(FLOAT_MIN.1), ..self }
    }
}

/// A dialog as it is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Floating {
    pub panel: Panel,
    pub bounds: FloatBounds,
}

/// A bottom dock too short for a prompt, or a side one too narrow for a
/// table, is no use; past the maximum the stage has no room left.
pub const BOTTOM: Bounds = Bounds { min: 120, max: 1200 };
pub const SIDE: Bounds = Bounds { min: 280, max: 1600 };

fn bounds(dock: Dock) -> Bounds {
    match dock {
        Dock::Bottom => BOTTOM,
        Dock::Left | Dock::Right => SIDE,
    }
}

/// One dock as it is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockLayout {
    /// In the order the person put them there, whether or not they exist
    /// now.
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
pub struct DocksLayout {
    #[serde(default = "side")]
    pub left: DockLayout,
    #[serde(default = "side")]
    pub right: DockLayout,
    #[serde(default = "bottom")]
    pub bottom: DockLayout,
    /// The panels the person opens as pages, in no dock.
    #[serde(default)]
    pub staged: Vec<Panel>,
    /// The dialogs, back to front.
    #[serde(default)]
    pub floating: Vec<Floating>,
}

fn side() -> DockLayout {
    DockLayout::sized(560)
}

fn bottom() -> DockLayout {
    DockLayout::sized(280)
}

/// Every dock closed: a window that never opens a panel is not given one.
impl Default for DocksLayout {
    fn default() -> Self {
        DocksLayout { left: side(), right: side(), bottom: bottom(), staged: Vec::new(), floating: Vec::new() }
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

    /// Takes the panel out of wherever it is. A dock it leaves shows the
    /// tab before it, and closes once it holds nothing.
    fn lift(&mut self, panel: &Panel) {
        self.staged.retain(|p| p != panel);
        self.floating.retain(|f| &f.panel != panel);
        for dock in DOCKS {
            let d = self.dock_mut(dock);
            let Some(at) = d.tabs.iter().position(|p| p == panel) else { continue };
            d.tabs.remove(at);
            if d.active.as_ref() == Some(panel) {
                d.active = d.tabs.get(at.saturating_sub(1)).cloned();
            }
            d.open &= d.active.is_some();
        }
    }
}

/// A dock's tab, named as its panel's own header names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DockTab {
    pub panel: Panel,
    pub title: String,
}

/// Which of the page's modules draws a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum PanelKind {
    Console,
    Views,
    Extension,
    Saved,
}

/// A panel that exists now, as the page is told of it: which module draws
/// it and what the person can do with it wherever it sits. The page offers
/// only these, and Rust does nothing for the rest anyway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PresentPanel {
    pub panel: Panel,
    pub kind: PanelKind,
    /// Its dock menu offers to float it as a dialog.
    pub floats: bool,
    /// Dropped on the stage it goes there: an extension's view as its
    /// page, the console as the Console stage.
    pub stage: bool,
    /// Its tab's close takes it out for good; a panel without one is put
    /// away only by its dock's hide.
    pub closes: bool,
}

/// A dock as the page draws it: only the panels that exist now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DockPage {
    pub tabs: Vec<DockTab>,
    /// The tab on screen, while the dock is open.
    pub shown: Option<Panel>,
    pub size: u32,
}

/// A dialog as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FloatPage {
    pub panel: Panel,
    pub title: String,
    pub bounds: FloatBounds,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DocksPage {
    pub left: DockPage,
    pub right: DockPage,
    pub bottom: DockPage,
    /// The dialogs of the panels that exist now, back to front.
    pub floating: Vec<FloatPage>,
    /// Every panel that exists now, docked, floating, a page or not yet
    /// placed: a drag from the sidebar names one no dock lists.
    pub present: Vec<PresentPanel>,
}

pub struct Docks {
    layout: DocksLayout,
    /// The panels that exist now: the console always, the views while an
    /// agent has one up, an extension's view while it is published here.
    present: Vec<Panel>,
    /// The file has not kept the layout since it changed.
    unsaved: bool,
    sent: Sent<DocksPage>,
}

impl Docks {
    /// A hand-edited file is made whole: a panel placed twice keeps its
    /// first place, a panel with a home and no place goes home, a size is
    /// held to its bounds, and a dock with nothing in it is closed.
    pub fn new(saved: DocksLayout) -> Docks {
        let mut layout = saved;
        let mut placed = Vec::new();
        for dock in DOCKS {
            let d = layout.dock_mut(dock);
            d.tabs.retain(|panel| {
                let first = !placed.contains(panel);
                placed.push(panel.clone());
                first
            });
            let bounds = bounds(dock);
            d.size = d.size.clamp(bounds.min, bounds.max);
        }
        layout.staged.retain(|panel| {
            let first = !placed.contains(panel) && panel.has_page();
            placed.push(panel.clone());
            first
        });
        layout.floating.retain(|f| {
            let first = !placed.contains(&f.panel) && f.panel.floats();
            placed.push(f.panel.clone());
            first
        });
        for f in &mut layout.floating {
            f.bounds = f.bounds.held();
        }
        for panel in [Panel::Console, Panel::Views] {
            if !layout.staged.contains(&panel) && !DOCKS.iter().any(|&d| layout.dock(d).tabs.contains(&panel)) {
                let home = panel.home().expect("the console and the views have a home");
                layout.dock_mut(home).tabs.push(panel);
            }
        }
        for dock in DOCKS {
            let d = layout.dock_mut(dock);
            if !d.active.as_ref().is_some_and(|active| d.tabs.contains(active)) {
                d.active = d.tabs.first().cloned();
            }
            d.open &= d.active.is_some();
        }
        Docks { layout, present: vec![Panel::Console], unsaved: false, sent: Sent::default() }
    }

    /// Which panels exist now, as the session finds them.
    pub fn set_present(&mut self, present: Vec<Panel>) {
        self.present = present;
    }

    pub fn place_of(&self, panel: &Panel) -> Option<Place> {
        if self.layout.staged.contains(panel) {
            return Some(Place::Stage);
        }
        if self.layout.floating.iter().any(|f| &f.panel == panel) {
            return Some(Place::Float);
        }
        DOCKS.into_iter().find(|&dock| self.layout.dock(dock).tabs.contains(panel)).map(Place::Dock)
    }

    /// The dock the panel is in; a page is in none.
    pub fn dock_of(&self, panel: &Panel) -> Option<Dock> {
        match self.place_of(panel) {
            Some(Place::Dock(dock)) => Some(dock),
            _ => None,
        }
    }

    /// Puts a panel never placed at `place`, closed: where an extension
    /// says its view opens, until the person moves it.
    pub fn place(&mut self, panel: &Panel, place: Place) {
        if self.place_of(panel).is_some() {
            return;
        }
        match place {
            Place::Dock(dock) => self.change(|layout| layout.dock_mut(dock).tabs.push(panel.clone())),
            Place::Stage => self.change(|layout| layout.staged.push(panel.clone())),
            Place::Float => self.float(panel),
        }
    }

    /// The tab a dock shows: the one chosen while it exists, else the
    /// first that does; none while the dock is closed.
    fn shown_in(&self, dock: Dock) -> Option<&Panel> {
        let d = self.layout.dock(dock);
        if !d.open {
            return None;
        }
        let present = |p: &&Panel| self.present.contains(p);
        d.active.as_ref().filter(present).or_else(|| d.tabs.iter().find(present))
    }

    /// Whether the panel is on screen in its dock, or as a dialog.
    pub fn shows(&self, panel: &Panel) -> bool {
        if self.place_of(panel) == Some(Place::Float) {
            return self.present.contains(panel);
        }
        self.dock_of(panel).is_some_and(|dock| self.shown_in(dock) == Some(panel))
    }

    /// Whether the panel's dock is on screen, showing it or another tab.
    pub fn dock_out(&self, panel: &Panel) -> bool {
        self.dock_of(panel).is_some_and(|dock| self.shown_in(dock).is_some())
    }

    /// Opens the panel's dock on the panel's tab, or brings its dialog in
    /// front; a page has no dock to open.
    pub fn show(&mut self, panel: &Panel) {
        self.raise(panel);
        let Some(dock) = self.dock_of(panel) else { return };
        self.change(|layout| {
            let d = layout.dock_mut(dock);
            d.active = Some(panel.clone());
            d.open = true;
        });
    }

    /// Closes the panel's dock, if it is the tab the dock shows; another
    /// tab on screen there stays.
    pub fn hide(&mut self, panel: &Panel) {
        if let Some(dock) = self.dock_of(panel).filter(|_| self.shows(panel)) {
            self.change(|layout| layout.dock_mut(dock).open = false);
        }
    }

    /// Moves the panel to the end of `to`'s tabs and shows it there.
    pub fn move_to(&mut self, panel: &Panel, to: Dock) {
        if self.dock_of(panel) != Some(to) {
            self.change(|layout| {
                layout.lift(panel);
                layout.dock_mut(to).tabs.push(panel.clone());
            });
        }
        self.show(panel);
    }

    /// Makes the panel a page of its own, out of any dock. Only an
    /// extension's view has a page: the console and the views have a home
    /// dock, and a saved view's columns want one.
    pub fn stage(&mut self, panel: &Panel) {
        if panel.has_page() && self.place_of(panel) != Some(Place::Stage) {
            self.change(|layout| {
                layout.lift(panel);
                layout.staged.push(panel.clone());
            });
        }
    }

    /// Makes the panel a dialog over the stage, in front, a step down and
    /// right of the dialog in front before it. Only a saved view floats.
    pub fn float(&mut self, panel: &Panel) {
        if !panel.floats() || self.place_of(panel) == Some(Place::Float) {
            self.raise(panel);
            return;
        }
        let bounds = match self.layout.floating.last() {
            Some(top) => FloatBounds { x: top.bounds.x + FLOAT_STEP, y: top.bounds.y + FLOAT_STEP, ..top.bounds },
            None => FLOAT_FIRST,
        };
        self.change(|layout| {
            layout.lift(panel);
            layout.floating.push(Floating { panel: panel.clone(), bounds });
        });
    }

    /// Where a dialog was moved or resized to, no smaller than a header
    /// and a little under it.
    pub fn set_bounds(&mut self, panel: &Panel, bounds: FloatBounds) {
        self.change(|layout| {
            if let Some(f) = layout.floating.iter_mut().find(|f| &f.panel == panel) {
                f.bounds = bounds.held();
            }
        });
    }

    /// Brings a dialog in front of the others.
    pub fn raise(&mut self, panel: &Panel) {
        let floating = &self.layout.floating;
        let Some(at) = floating.iter().position(|f| &f.panel == panel) else { return };
        if at + 1 == floating.len() {
            return;
        }
        self.change(|layout| {
            let f = layout.floating.remove(at);
            layout.floating.push(f);
        });
    }

    /// Takes the panel out of wherever it is placed, for good: a saved
    /// view the person closed or the daemon deleted keeps no place.
    pub fn forget(&mut self, panel: &Panel) {
        if self.place_of(panel).is_some() {
            self.change(|layout| layout.lift(panel));
        }
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

    /// What the page has not heard of the docks, each panel named by
    /// `title`.
    pub fn take(&mut self, title: impl Fn(&Panel) -> String) -> Option<Event> {
        let page = self.page(title);
        self.sent.take(Some(page)).map(|page| Event::Docks(Box::new(page)))
    }

    /// The page reloaded and holds nothing: the docks are sent again.
    pub fn reload(&mut self) {
        self.sent.forget();
    }

    /// The docks with the panels that exist now, each named by `title`.
    fn page(&self, title: impl Fn(&Panel) -> String) -> DocksPage {
        let dock = |dock: Dock| {
            let d = self.layout.dock(dock);
            DockPage {
                tabs: d
                    .tabs
                    .iter()
                    .filter(|p| self.present.contains(p))
                    .map(|panel| DockTab { panel: panel.clone(), title: title(panel) })
                    .collect(),
                shown: self.shown_in(dock).cloned(),
                size: d.size,
            }
        };
        let floating = self
            .layout
            .floating
            .iter()
            .filter(|f| self.present.contains(&f.panel))
            .map(|f| FloatPage { panel: f.panel.clone(), title: title(&f.panel), bounds: f.bounds })
            .collect();
        let present = self.present.iter().map(Panel::told).collect();
        DocksPage { left: dock(Dock::Left), right: dock(Dock::Right), bottom: dock(Dock::Bottom), floating, present }
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

    fn images() -> Panel {
        Panel::extension("docker", "images")
    }

    fn docks() -> Docks {
        let mut docks = Docks::new(DocksLayout::default());
        docks.set_present(vec![Panel::Console, Panel::Views, images()]);
        docks
    }

    #[test]
    fn a_panel_starts_hidden_in_its_home_dock() {
        let docks = docks();
        assert_eq!(docks.dock_of(&Panel::Console), Some(Dock::Bottom));
        assert_eq!(docks.dock_of(&Panel::Views), Some(Dock::Right));
        assert!(!docks.shows(&Panel::Console));
        assert_eq!(docks.place_of(&images()), None, "an extension's view waits to be placed");
    }

    #[test]
    fn hiding_a_panel_and_showing_it_again_finds_it_where_it_was_put() {
        let mut docks = docks();
        docks.move_to(&Panel::Console, Dock::Right);
        assert!(docks.shows(&Panel::Console));
        assert_eq!(docks.layout().bottom.tabs, [], "it left the bottom");
        docks.hide(&Panel::Console);
        assert!(!docks.shows(&Panel::Console));
        docks.show(&Panel::Console);
        assert_eq!((docks.dock_of(&Panel::Console), docks.shows(&Panel::Console)), (Some(Dock::Right), true));
    }

    #[test]
    fn panels_in_one_dock_are_tabs_and_one_shows_at_a_time() {
        let mut docks = docks();
        docks.move_to(&Panel::Console, Dock::Right);
        docks.show(&Panel::Views);
        assert!(docks.shows(&Panel::Views));
        assert!(!docks.shows(&Panel::Console), "the other tab");
        docks.hide(&Panel::Console);
        assert!(docks.shows(&Panel::Views), "hiding a tab not shown leaves the dock alone");
        let page = docks.page(|p| p.to_string());
        assert_eq!(page.right.tabs.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["views", "console"]);
        assert_eq!(page.right.shown, Some(Panel::Views));
    }

    #[test]
    fn the_dock_a_panel_leaves_shows_the_tab_before_it_or_closes_once_empty() {
        let mut docks = docks();
        docks.move_to(&Panel::Console, Dock::Right);
        docks.move_to(&Panel::Console, Dock::Left);
        assert!(docks.shows(&Panel::Views), "the right dock shows the tab before it");
        docks.move_to(&Panel::Views, Dock::Left);
        assert!(!docks.layout().right.open, "nothing left in it");
        assert_eq!(docks.layout().right.active, None);
    }

    #[test]
    fn an_extensions_view_is_placed_once_where_it_asks_and_then_where_the_person_puts_it() {
        let mut docks = docks();
        docks.place(&images(), Place::Dock(Dock::Bottom));
        assert_eq!(docks.place_of(&images()), Some(Place::Dock(Dock::Bottom)));
        assert!(!docks.shows(&images()), "placed, not shown");
        docks.stage(&images());
        docks.place(&images(), Place::Dock(Dock::Bottom));
        assert_eq!(docks.place_of(&images()), Some(Place::Stage), "the person's choice stands");
        docks.move_to(&images(), Dock::Left);
        assert_eq!(docks.layout().staged, [], "a page docked is a page no more");
        docks.stage(&Panel::Console);
        assert_eq!(docks.dock_of(&Panel::Console), Some(Dock::Bottom), "the console is never a page");
    }

    #[test]
    fn a_dock_shows_only_panels_that_exist_and_keeps_the_place_of_one_gone() {
        let mut docks = docks();
        docks.place(&images(), Place::Dock(Dock::Bottom));
        docks.show(&images());
        docks.set_present(vec![Panel::Console]);
        assert!(!docks.shows(&images()));
        assert!(docks.shows(&Panel::Console), "the tab still there shows");
        assert_eq!(docks.page(|p| p.to_string()).bottom.tabs.len(), 1);
        docks.set_present(vec![Panel::Console, images()]);
        assert!(docks.shows(&images()), "back where it was");
        docks.set_present(vec![]);
        assert_eq!(docks.page(|p| p.to_string()).bottom.shown, None, "a dock of nothing is closed on screen");
    }

    #[test]
    fn a_layout_is_saved_once_per_change_and_sizes_are_held_to_their_bounds() {
        let mut docks = docks();
        docks.set_size(Dock::Bottom, 10);
        docks.set_size(Dock::Right, 10_000);
        assert_eq!(docks.layout().bottom.size, BOTTOM.min);
        assert_eq!(docks.layout().right.size, SIDE.max);
        assert!(docks.take_unsaved());
        assert!(!docks.take_unsaved());
        docks.set_size(Dock::Right, SIDE.max);
        docks.hide(&Panel::Console);
        docks.set_present(vec![]);
        assert!(!docks.take_unsaved(), "the same size, a panel hidden already, and what exists are not kept");
    }

    fn plan() -> Panel {
        Panel::Saved("release-plan".into())
    }

    #[test]
    fn a_saved_views_panel_is_placed_once_docked_and_never_a_page() {
        let mut docks = docks();
        docks.set_present(vec![Panel::Console, plan()]);
        assert_eq!(docks.place_of(&plan()), None, "a saved view waits to be opened");
        docks.place(&plan(), Place::Dock(Dock::Right));
        docks.show(&plan());
        assert!(docks.shows(&plan()));
        docks.stage(&plan());
        assert_eq!(docks.place_of(&plan()), Some(Place::Dock(Dock::Right)), "it has no page");
        docks.forget(&plan());
        assert_eq!(docks.place_of(&plan()), None);
        assert_eq!(docks.layout().right.active, Some(Panel::Views), "the dock shows the tab before it");
    }

    #[test]
    fn a_saved_views_panel_is_kept_with_its_place_in_the_file_and_one_with_no_id_is_refused() {
        let mut docks = docks();
        docks.place(&plan(), Place::Dock(Dock::Left));
        let json = serde_json::to_value(docks.layout()).unwrap();
        assert_eq!(json["left"]["tabs"], serde_json::json!(["saved:release-plan"]));
        let again = Docks::new(serde_json::from_value(json).unwrap());
        assert_eq!(again.dock_of(&plan()), Some(Dock::Left), "across runs");
        assert!(serde_json::from_str::<Panel>(r#""saved:""#).is_err());
        let staged = Docks::new(DocksLayout { staged: vec![plan()], ..DocksLayout::default() });
        assert_eq!(staged.place_of(&plan()), None, "a hand-edited page of one is dropped");
    }

    #[test]
    fn a_hand_edited_layout_is_made_whole() {
        let docks = Docks::new(DocksLayout {
            left: DockLayout { tabs: vec![Panel::Console], active: None, open: true, size: 1 },
            right: DockLayout { tabs: vec![Panel::Console], active: Some(Panel::Console), open: true, size: 600 },
            bottom: DockLayout { open: true, ..bottom() },
            staged: vec![Panel::Views, images(), images()],
            floating: vec![
                Floating { panel: plan(), bounds: FloatBounds { x: 0, y: 0, width: 1, height: 1 } },
                Floating { panel: images(), bounds: FLOAT_FIRST },
            ],
        });
        assert_eq!(docks.dock_of(&Panel::Console), Some(Dock::Left), "the first dock keeps it");
        assert!(docks.shows(&Panel::Console), "its only tab is the one shown");
        assert_eq!(docks.layout().left.size, SIDE.min);
        assert_eq!(docks.layout().right.tabs, [Panel::Views], "the views are never a page, and go home");
        assert!(!docks.layout().bottom.open, "nothing in it");
        assert_eq!(docks.layout().staged, [images()]);
        assert_eq!(docks.place_of(&plan()), Some(Place::Float));
        assert_eq!(docks.layout().floating.len(), 1, "an extension's view placed twice, and not one that floats");
        assert_eq!(docks.layout().floating[0].bounds, FloatBounds { x: 0, y: 0, width: FLOAT_MIN.0, height: FLOAT_MIN.1 });
    }

    #[test]
    fn a_saved_view_floats_as_a_dialog_each_one_stepped_from_the_last_and_the_one_shown_in_front() {
        let mut docks = docks();
        let other = Panel::Saved("other".into());
        docks.set_present(vec![Panel::Console, Panel::Views, plan(), other.clone()]);
        docks.place(&plan(), Place::Dock(Dock::Right));
        docks.show(&plan());
        docks.float(&plan());
        assert_eq!(docks.place_of(&plan()), Some(Place::Float));
        assert_eq!(docks.layout().right.tabs, [Panel::Views], "it left its dock");
        assert!(docks.shows(&plan()), "a dialog of a view that exists is on screen");
        docks.place(&other, Place::Float);
        let at = |d: &Docks| d.page(|p| p.to_string()).floating.iter().map(|f| (f.panel.clone(), f.bounds.x)).collect::<Vec<_>>();
        assert_eq!(at(&docks), [(plan(), FLOAT_FIRST.x), (other.clone(), FLOAT_FIRST.x + FLOAT_STEP)]);
        docks.show(&plan());
        assert_eq!(at(&docks)[1].0, plan(), "shown, it comes in front");
        docks.set_bounds(&plan(), FloatBounds { x: -5, y: 40, width: 10, height: 900 });
        assert_eq!(docks.layout().floating[1].bounds, FloatBounds { x: -5, y: 40, width: FLOAT_MIN.0, height: 900 });
        docks.move_to(&plan(), Dock::Left);
        assert_eq!((docks.dock_of(&plan()), docks.layout().floating.len()), (Some(Dock::Left), 1), "docked, a dialog no more");
        docks.float(&Panel::Console);
        assert_eq!(docks.dock_of(&Panel::Console), Some(Dock::Bottom), "the console never floats");
        docks.forget(&other);
        assert!(docks.layout().floating.is_empty());
    }

    #[test]
    fn a_panel_is_named_as_one_string_in_the_file() {
        let mut docks = docks();
        docks.place(&images(), Place::Dock(Dock::Right));
        docks.show(&images());
        let json = serde_json::to_value(docks.layout()).unwrap();
        assert_eq!(json["right"]["tabs"], serde_json::json!(["views", "extension:docker/images"]));
        assert_eq!(json["right"]["active"], "extension:docker/images");
        assert_eq!(serde_json::from_value::<DocksLayout>(json).unwrap(), *docks.layout());
        assert_eq!(serde_json::from_str::<DocksLayout>("{}").unwrap(), DocksLayout::default());
        assert!(serde_json::from_str::<Panel>(r#""extension:docker""#).is_err(), "a view needs its name");
        assert_eq!(images().extension_view(), Some(("docker", "images")));
    }

    fn abilities(page: &DocksPage) -> Vec<(Panel, PanelKind, bool, bool, bool)> {
        page.present.iter().map(|p| (p.panel.clone(), p.kind, p.floats, p.stage, p.closes)).collect()
    }

    #[test]
    fn every_panel_that_exists_is_told_with_what_it_can_do() {
        let mut docks = docks();
        docks.set_present(vec![Panel::Console, Panel::Views, images(), plan()]);
        assert_eq!(
            abilities(&docks.page(|p| p.to_string())),
            [
                (Panel::Console, PanelKind::Console, false, true, false),
                (Panel::Views, PanelKind::Views, false, false, false),
                (images(), PanelKind::Extension, false, true, false),
                (plan(), PanelKind::Saved, true, false, true),
            ]
        );
        docks.set_present(vec![Panel::Console]);
        assert_eq!(abilities(&docks.page(|p| p.to_string())), [(Panel::Console, PanelKind::Console, false, true, false)]);
    }

    #[test]
    fn a_panel_no_dock_lists_is_still_told_what_it_can_do() {
        let mut docks = docks();
        let staged = Panel::extension("docker", "containers");
        docks.set_present(vec![Panel::Console, images(), staged.clone()]);
        docks.place(&staged, Place::Stage);
        assert_eq!(docks.place_of(&images()), None, "never placed");
        assert_eq!(docks.place_of(&staged), Some(Place::Stage));
        let page = docks.page(|p| p.to_string());
        let mut tabs = [&page.left, &page.right, &page.bottom].into_iter().flat_map(|d| &d.tabs);
        assert!(!tabs.any(|t| t.panel == images() || t.panel == staged), "in no dock");
        assert!(page.floating.is_empty(), "and no dialog");
        let told = page.present.iter().map(|p| &p.panel).collect::<Vec<_>>();
        assert!(told.contains(&&images()) && told.contains(&&staged), "a sidebar drag of either finds what it can do");
    }

    /// The page draws nothing for a panel it was not told of, so a panel
    /// shown and missing here would be a dock left silently empty.
    #[test]
    fn every_panel_a_dock_or_dialog_shows_is_among_those_told() {
        let mut docks = docks();
        let other = Panel::Saved("other".into());
        docks.set_present(vec![Panel::Console, Panel::Views, images(), plan(), other.clone()]);
        docks.place(&images(), Place::Dock(Dock::Left));
        docks.show(&images());
        docks.move_to(&Panel::Console, Dock::Right);
        docks.place(&other, Place::Dock(Dock::Bottom));
        docks.show(&other);
        docks.float(&plan());
        let page = docks.page(|p| p.to_string());
        let told = page.present.iter().map(|p| &p.panel).collect::<Vec<_>>();
        let docked = [&page.left, &page.right, &page.bottom].into_iter().flat_map(|d| d.tabs.iter().map(|t| &t.panel).chain(&d.shown));
        let shown = docked.chain(page.floating.iter().map(|f| &f.panel)).collect::<Vec<_>>();
        assert_eq!(shown.len(), 8, "four tabs, three of them shown, and a dialog");
        for panel in shown {
            assert!(told.contains(&panel), "{panel} is drawn but not told");
        }
        let mut once = told.clone();
        once.sort();
        once.dedup();
        assert_eq!(once.len(), told.len(), "each one told once");
    }

    #[test]
    fn a_panel_floats_exactly_when_the_page_is_told_it_does() {
        for panel in [Panel::Console, Panel::Views, images(), plan()] {
            let mut docks = docks();
            docks.set_present(vec![Panel::Console, Panel::Views, images(), plan()]);
            docks.float(&panel);
            assert_eq!(docks.place_of(&panel) == Some(Place::Float), panel.told().floats, "{panel} floats");
            docks.stage(&panel);
            if docks.place_of(&panel) == Some(Place::Stage) {
                assert!(panel.told().stage, "{panel} has a page, so a drop on the stage takes it");
            }
        }
    }

    #[test]
    fn a_panel_is_told_to_the_page_as_its_kind_and_abilities() {
        assert_eq!(
            serde_json::to_value(plan().told()).unwrap(),
            serde_json::json!({"panel": "saved:release-plan", "kind": "saved", "floats": true, "stage": false, "closes": true})
        );
        assert_eq!(serde_json::to_value(PanelKind::Extension).unwrap(), "extension");
    }

    #[test]
    fn a_dock_gesture_stages_or_closes_a_panel_by_name() {
        let gesture = |json| serde_json::from_value::<DockGesture>(json).unwrap();
        assert_eq!(gesture(serde_json::json!({"on": "stage", "panel": "console"})), DockGesture::Stage { panel: Panel::Console });
        assert_eq!(gesture(serde_json::json!({"on": "close", "panel": "saved:a"})), DockGesture::Close { panel: Panel::Saved("a".into()) });
    }
}
