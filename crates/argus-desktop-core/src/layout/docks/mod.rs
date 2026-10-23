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

pub mod expand;

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::layout::dragged_width::Bounds;
use crate::connection::page::Event;
use crate::connection::sent::Sent;
use crate::panes::shown::workspace_grid::{Occupant, Zone};

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
    /// The panel dropped on a cell of the Workspace's grid, splitting off
    /// beside it there; only a panel that `cells` may.
    Grid { panel: Panel, onto: Occupant, zone: Zone },
    /// A panel's close for good, which only a panel that `closes` has;
    /// any other stays where it is.
    Close { panel: Panel },
    /// A panel on the stage sent back to where it was put on the stage
    /// from: its dock, or its dialog where it was.
    Collapse { panel: Panel },
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

/// What can be docked: the console, the views agents showed, every agent's
/// browser, each extension's view by its extension and name, and each saved
/// view by its id. Named in the file and to the page as one string —
/// `console`, `views`, `browsers`, `git`, `extension:docker/images`,
/// `saved:release-plan` — since the last two are any of an open-ended set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, type = "string"))]
pub enum Panel {
    Console,
    Views,
    /// The Browsers stage's tiles, every agent's browser, kept beside an
    /// agent's pane rather than in place of it.
    Browsers,
    Extension(String),
    /// A saved view's panel, named by the view it opened with; the columns
    /// that follow it are `views::saved`'.
    Saved(String),
    /// The Git stage's Changes and History, docked or floating beside the
    /// work rather than in place of the pane.
    Git,
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
            Panel::Views | Panel::Browsers => Some(Dock::Right),
            Panel::Extension(_) | Panel::Saved(_) | Panel::Git => None,
        }
    }

    /// Whether the panel can be a page on the stage. A saved view's columns
    /// scroll sideways in a dock; the stage has no use for them. Git's page
    /// is the Git stage, drawn where the pane is.
    fn has_page(&self) -> bool {
        matches!(self, Panel::Extension(_) | Panel::Git)
    }

    /// Whether the panel can float as a dialog: a saved view, read beside
    /// the one it was reached from without taking a dock from it, and the
    /// browsers, watched over whatever the stage shows, and Git, a review
    /// kept over the agent whose work it is. The console's terminal and the
    /// views stay docked.
    fn floats(&self) -> bool {
        matches!(self, Panel::Saved(_) | Panel::Browsers | Panel::Git)
    }

    fn kind(&self) -> PanelKind {
        match self {
            Panel::Console => PanelKind::Console,
            Panel::Views => PanelKind::Views,
            Panel::Browsers => PanelKind::Browsers,
            Panel::Extension(_) => PanelKind::Extension,
            Panel::Saved(_) => PanelKind::Saved,
            Panel::Git => PanelKind::Git,
        }
    }

    /// Whether a drop on the stage takes the panel there. Every panel with
    /// a page does; so do the console and the browsers, whose stages are
    /// the Console and Browsers stages rather than pages, so neither is one
    /// of `staged`. No `_` arm, so a new kind of panel is decided here
    /// rather than defaulted.
    fn stages(&self) -> bool {
        match self {
            Panel::Console | Panel::Browsers | Panel::Extension(_) | Panel::Git => true,
            Panel::Views | Panel::Saved(_) => false,
        }
    }

    /// Whether its close takes it out for good: a saved view's panel goes
    /// with its columns, and Git's ends the review or history it shows,
    /// keeping its place for the next. The console, the views and an
    /// extension's view are there while what they show is.
    fn closes(&self) -> bool {
        match self {
            Panel::Saved(_) | Panel::Git => true,
            Panel::Console | Panel::Views | Panel::Browsers | Panel::Extension(_) => false,
        }
    }

    /// Whether it can split off into a cell of the Workspace's grid, beside
    /// an agent's pane. The browsers only, for now: each panel joins once
    /// its body is drawn well at a cell's size.
    pub fn cells(&self) -> bool {
        match self {
            Panel::Browsers => true,
            Panel::Console | Panel::Views | Panel::Extension(_) | Panel::Saved(_) | Panel::Git => false,
        }
    }

    fn told(&self) -> PresentPanel {
        PresentPanel {
            panel: self.clone(),
            kind: self.kind(),
            floats: self.floats(),
            stage: self.stages(),
            closes: self.closes(),
            cell: self.cells(),
        }
    }
}

impl fmt::Display for Panel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Panel::Console => f.write_str("console"),
            Panel::Views => f.write_str("views"),
            Panel::Browsers => f.write_str("browsers"),
            Panel::Git => f.write_str("git"),
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
            "browsers" => Ok(Panel::Browsers),
            "git" => Ok(Panel::Git),
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
    /// Where each panel on the stage was put there from.
    #[serde(default)]
    pub origins: Vec<expand::Origin>,
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
        DocksLayout { left: side(), right: side(), bottom: bottom(), staged: Vec::new(), floating: Vec::new(), origins: Vec::new() }
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
        self.origins.retain(|o| &o.panel != panel);
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
    Browsers,
    Extension,
    Saved,
    Git,
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
    /// Dropped on a cell of the Workspace's grid it splits off there.
    pub cell: bool,
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
        let staged = layout.staged.clone();
        let mut remembered = Vec::new();
        layout.origins.retain(|o| {
            let first = staged.contains(&o.panel) && !remembered.contains(&o.panel);
            remembered.push(o.panel.clone());
            first
        });
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

    /// Takes the panel off screen wherever the docks show it, to go
    /// somewhere they do not draw: a dialog goes back to its home dock as a
    /// tab, and a dock showing it closes.
    pub fn put_away(&mut self, panel: &Panel) {
        if self.place_of(panel) == Some(Place::Float) {
            let home = panel.home().unwrap_or(Dock::Right);
            self.change(|layout| {
                layout.lift(panel);
                layout.dock_mut(home).tabs.push(panel.clone());
            });
        }
        self.hide(panel);
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

    /// Makes the panel a page of its own, out of any dock, remembering the
    /// dock or dialog it left for `collapse`. Only an extension's view and
    /// Git have a page: the console and the views have a home dock, and a
    /// saved view's columns want one.
    pub fn stage(&mut self, panel: &Panel) {
        if panel.has_page() && self.place_of(panel) != Some(Place::Stage) {
            let origin = self.origin(panel);
            self.change(|layout| {
                layout.lift(panel);
                layout.staged.push(panel.clone());
                layout.origins.extend(origin);
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
mod tests;
