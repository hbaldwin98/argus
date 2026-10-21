//! What the window remembers between runs: how each repository's feature
//! index is laid out, which agent it last started, whether an agent coming
//! to need you chimes, the side and layout a review opens on, where each
//! panel is docked and how big each dock is, where each extension view
//! opens, and what the Settings dialog sets.
//!
//! Its own file beside the config, `desktop.json`, rather than a section
//! of the terminal client's `client.toml` (DESIGN.md §Configuration says
//! why): the two clients overlap only in the editor and the theme's name,
//! and each writes its whole file from the copy it loaded, so sharing one
//! would have each quietly undo the other's changes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout::docks::{Dock, DocksLayout, Panel};
use crate::board::features::Layout;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// `list` or `board`, by the path of a repository's primary checkout:
    /// the path outlives the daemon's ids, which are handed out anew each run.
    #[serde(default)]
    pub layouts: BTreeMap<String, String>,
    /// The ssh host whose daemon the window reaches, as `ssh <host>` names
    /// it; absent for this machine's. Kept so the next run opens where this
    /// one ended.
    #[serde(default)]
    pub host: Option<String>,
    /// Every host the window has been switched to, for the switcher to offer
    /// beside the ssh config's.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// The agent template this window last started, by name. Remembered
    /// rather than read off the tree: the tree knows only the agents still
    /// running, and a window whose agents have all closed, or that was
    /// restarted, would forget which one the person reaches for.
    #[serde(default)]
    pub last_agent: Option<String>,
    /// On unless turned off, unlike the terminal client's opt-in bell: a
    /// window is not where a person is already typing, so it is the place
    /// they are least likely to see an agent finish.
    #[serde(default = "on")]
    pub chime: bool,
    /// The review's side and layout as last chosen, which the next review
    /// opens on: as in the terminal client, a setting rather than a choice
    /// made again on each visit.
    #[serde(default)]
    pub review: ReviewLayout,
    /// Which dock holds each panel, or which open as pages, which tab
    /// each dock shows, whether it is open, and how big: kept here rather
    /// than in the webview's storage, which every window of one instance
    /// shares and no other file of the window's is in. Absent only in a
    /// file from before the docks, which `load` makes it from.
    #[serde(default)]
    pub docks: Option<DocksLayout>,
    /// Whether the console was docked under the stages, and how tall, as a
    /// file from before the docks says: read once into `docks`, never
    /// written.
    #[serde(default, skip_serializing)]
    pub console: Option<ConsoleLayout>,
    /// The rail's width as the person dragged it, in CSS pixels; absent
    /// until they do.
    #[serde(default)]
    pub rail_width: Option<u32>,
    /// The views panel's width as a file from before the docks says: the
    /// right dock's, read once into `docks`, never written.
    #[serde(default, skip_serializing)]
    pub views_width: Option<u32>,
    /// An open feature's task outline's width, likewise.
    #[serde(default)]
    pub outline_width: Option<u32>,
    /// An open feature's brief's height, likewise; past it the brief scrolls.
    #[serde(default)]
    pub brief_height: Option<u32>,
    /// The Code stage's file tree's and outline's widths, likewise.
    #[serde(default)]
    pub code_tree_width: Option<u32>,
    #[serde(default)]
    pub code_outline_width: Option<u32>,
    /// Whether the sidebar's Extensions are unfolded, and where the person
    /// chose each extension's view opens.
    #[serde(default)]
    pub extensions: ExtensionsLayout,
    /// What the Settings dialog sets, flat in the file beside the rest.
    #[serde(flatten)]
    pub preferences: Preferences,
}

/// The Settings dialog's choices. Each field keeps its default when the
/// file predates it, so an older file loses nothing it held.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default)]
    pub theme: Theme,
    /// How solid the window's background is, in percent: the desktop shows
    /// through the rest, and text and borders stay solid. Never under
    /// [`MIN_OPACITY`], so a window cannot be made to vanish.
    #[serde(default = "opaque")]
    pub opacity: u8,
    #[serde(default)]
    pub backdrop: Backdrop,
    #[serde(default)]
    pub editor: EditorMode,
    /// The editor to run, flags and all. Empty leaves the daemon to use
    /// `$VISUAL`, then `$EDITOR`.
    #[serde(default)]
    pub editor_command: String,
    /// Vim's keys in the Code stage's editor. On unless turned off: the
    /// stage's own keys are Vim's, and so are the person's.
    #[serde(default = "on")]
    pub vim: bool,
    /// An OS notification naming the pane when an agent comes to need you:
    /// the window's version of the terminal client's bell.
    #[serde(default = "on")]
    pub notify: bool,
    /// Only while the window is in the background: a window in front is
    /// already where the person is looking.
    #[serde(default = "on")]
    pub notify_in_background: bool,
    /// The index a repository opens as before it remembers its own.
    #[serde(default)]
    pub feature_layout: Layout,
    #[serde(default)]
    pub saved_links: SavedLinks,
    /// The family of an installed font the panes and every monospaced
    /// line are drawn in, in front of the bundled JetBrains Mono and Nerd
    /// Font symbols, which still draw what it lacks. Empty draws the
    /// bundled ones alone.
    #[serde(default)]
    pub font: String,
    /// The family of an installed font every other line is drawn in, in
    /// front of the system's own. Empty draws the system's alone.
    #[serde(default)]
    pub ui_font: String,
    /// How large the whole window is drawn, panes and all, in percent: set
    /// by the zoom keys rather than the dialog, and kept like its choices.
    #[serde(default = "actual_size")]
    pub zoom: u16,
}

pub const MIN_OPACITY: u8 = 30;

/// The window's colours: a Catppuccin flavour, Argus Night, one of the dark
/// palettes borrowed from other tools (Ledger onward), or whichever of Mocha
/// and Latte the system's light or dark says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    /// What the window did before it had a setting, so a file from before
    /// it looks as it always did.
    #[default]
    System,
    Mocha,
    Macchiato,
    Frappe,
    Latte,
    /// Argus's own: a cool navy ground lit by one periwinkle accent.
    Night,
    /// Near-black greys, one indigo, hairlines at a single step (after Linear).
    Ledger,
    /// IBM Carbon's Gray 100: neutral greys, Carbon blue, square and sober.
    Carbon,
    /// Steph Ango's Flexoki dark: warm ink on black paper, cyan for what you can act on.
    Flexoki,
    /// Kanagawa Wave: sumi ink greys, aged-paper text, a wave-blue selection.
    Kanagawa,
    /// Rosé Pine: dusky violet surfaces, nothing near black, rose as the one warm accent.
    RosePine,
    /// Nord: Polar Night greys, Snow Storm text, Frost blue as the accent.
    Nord,
    /// Radix Colors' slate and blue, each step in the role its scale gives it.
    Radix,
}

impl Theme {
    /// The flavour drawn, given whether the system is dark.
    pub fn resolve(self, system_dark: bool) -> Theme {
        match self {
            Theme::System if system_dark => Theme::Mocha,
            Theme::System => Theme::Latte,
            flavour => flavour,
        }
    }

    pub fn is_dark(self, system_dark: bool) -> bool {
        self.resolve(system_dark) != Theme::Latte
    }
}

/// What shows through the window's background where it is see-through.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Backdrop {
    /// The desktop itself.
    #[default]
    Clear,
    /// Windows 11's Mica: the wallpaper, blurred and tinted.
    Mica,
    /// Windows' Acrylic: whatever is behind the window, blurred.
    Acrylic,
}

/// Where a link followed inside a saved view opens its target.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum SavedLinks {
    /// A dialog over the stage, so the view it came from keeps its dock.
    #[default]
    Dialog,
    /// The next column of the panel it was followed in.
    Column,
}

/// Where a file opened from the window opens: the terminal client's three
/// editor modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum EditorMode {
    #[default]
    Floating,
    /// A pane in the checkout. An editor is never a listed pane, so the
    /// daemon floats it as the first mode does (DESIGN.md §Editors).
    Checkout,
    /// Launched on its own with no pty, for an editor with its own window.
    Separately,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            theme: Theme::default(),
            opacity: opaque(),
            backdrop: Backdrop::default(),
            editor: EditorMode::default(),
            editor_command: String::new(),
            vim: true,
            notify: true,
            notify_in_background: true,
            feature_layout: Layout::default(),
            saved_links: SavedLinks::default(),
            font: String::new(),
            ui_font: String::new(),
            zoom: actual_size(),
        }
    }
}

fn actual_size() -> u16 {
    100
}

fn opaque() -> u8 {
    100
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewLayout {
    #[serde(default)]
    pub staged: bool,
    #[serde(default)]
    pub split: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionsLayout {
    #[serde(default)]
    pub open: bool,
    /// Docked or a page, by `extension/view`, as a file from before the
    /// docks says: read once into `docks`, never written.
    #[serde(default, skip_serializing)]
    pub modes: BTreeMap<String, argus_protocol::ExtPlace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleLayout {
    #[serde(default)]
    pub docked: bool,
    #[serde(default = "console_height")]
    pub height: u32,
}

fn console_height() -> u32 {
    280
}

/// The docks a file from before them describes: the console docked under
/// the stages is the bottom dock showing it, the views panel is the right
/// dock, and a view docked in it is one of its tabs.
fn older_docks(settings: &mut Settings) -> DocksLayout {
    let mut docks = DocksLayout::default();
    if let Some(console) = settings.console.take() {
        let bottom = docks.dock_mut(Dock::Bottom);
        bottom.tabs = vec![Panel::Console];
        bottom.active = Some(Panel::Console);
        bottom.open = console.docked;
        bottom.size = console.height;
    }
    let right = docks.dock_mut(Dock::Right);
    right.tabs.push(Panel::Views);
    if let Some(width) = settings.views_width.take() {
        right.size = width;
    }
    for (key, place) in std::mem::take(&mut settings.extensions.modes) {
        let panel = Panel::Extension(key);
        match place {
            argus_protocol::ExtPlace::Stage => docks.staged.push(panel),
            _ => docks.right.tabs.push(panel),
        }
    }
    docks
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            layouts: BTreeMap::new(),
            host: None,
            hosts: Vec::new(),
            last_agent: None,
            chime: true,
            review: ReviewLayout::default(),
            docks: None,
            console: None,
            rail_width: None,
            views_width: None,
            outline_width: None,
            brief_height: None,
            code_tree_width: None,
            code_outline_width: None,
            extensions: ExtensionsLayout::default(),
            preferences: Preferences::default(),
        }
    }
}

fn on() -> bool {
    true
}

fn path() -> PathBuf {
    argus_protocol::config_dir().join("desktop.json")
}

/// The remembered settings, or none at all: a missing or unreadable file
/// is a window that remembers nothing yet, never one that will not open.
pub fn load() -> Settings {
    load_from(&path())
}

/// Best effort, written beside the file and moved over it, so a window
/// closed mid-write leaves the old settings rather than half of new ones.
pub fn save(settings: &Settings) {
    if let Err(err) = save_to(&path(), settings) {
        tracing::warn!("could not save the window's settings: {err}");
    }
}

fn load_from(path: &Path) -> Settings {
    let mut settings: Settings = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    if settings.docks.is_none() {
        settings.docks = Some(older_docks(&mut settings));
    }
    // Never written, so only an older window's file has them, and only for
    // `older_docks`; one written by an older window since keeps no docks.
    settings.console = None;
    settings.views_width = None;
    settings.host = settings.host.take().filter(|host| !host.trim().is_empty());
    settings.extensions.modes.clear();
    settings
}

fn save_to(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, serde_json::to_vec_pretty(settings)?)?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("argus-desktop-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("desktop.json")
    }

    #[test]
    fn settings_saved_are_the_settings_loaded() {
        let path = scratch("round");
        let mut settings = Settings::default();
        settings.layouts.insert("/src/argus".into(), "board".into());
        settings.last_agent = Some("codex".into());
        settings.chime = false;
        settings.review = ReviewLayout { staged: true, split: true };
        let mut docks = DocksLayout::default();
        docks.bottom.open = true;
        docks.staged.push(Panel::extension("docker", "containers"));
        settings.docks = Some(docks);
        settings.preferences = Preferences {
            theme: Theme::Frappe,
            opacity: 70,
            backdrop: Backdrop::Mica,
            editor: EditorMode::Separately,
            editor_command: "code -w".into(),
            vim: false,
            notify: false,
            notify_in_background: false,
            feature_layout: Layout::Board,
            saved_links: SavedLinks::Column,
            font: "Iosevka Term".into(),
            ui_font: "Inter".into(),
            zoom: 125,
        };
        save_to(&path, &settings).unwrap();
        assert_eq!(load_from(&path), settings);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_file_from_before_vim_keys_has_them_on() {
        let path = scratch("vim");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{ "preferences": { "theme": "frappe" } }"#).unwrap();
        assert!(load_from(&path).preferences.vim);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_blank_host_is_this_machine() {
        let path = scratch("host");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{ "host": " " }"#).unwrap();
        assert_eq!(load_from(&path).host, None);
        std::fs::write(&path, br#"{ "host": "devbox" }"#).unwrap();
        assert_eq!(load_from(&path).host.as_deref(), Some("devbox"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_missing_or_broken_file_remembers_nothing() {
        let path = scratch("broken");
        assert_eq!(load_from(&path), Settings { docks: Some(older_docks(&mut Settings::default())), ..Settings::default() });
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(load_from(&path), Settings { docks: Some(older_docks(&mut Settings::default())), ..Settings::default() });
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_file_from_before_the_last_agent_and_the_chime_keeps_its_layouts_and_chimes() {
        let path = scratch("older");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{ "layouts": { "/src/argus": "board" } }"#).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.layouts.get("/src/argus").map(String::as_str), Some("board"));
        assert_eq!(loaded.last_agent, None);
        assert!(loaded.chime);
        assert_eq!(loaded.review, ReviewLayout::default(), "unstaged and unified, as before");
        let docks = loaded.docks.expect("made from what the file had");
        assert!(!docks.bottom.open, "no console until one is asked for");
        assert_eq!(loaded.preferences, Preferences::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_file_from_before_the_docks_keeps_its_console_views_panel_and_extension_views_where_they_were() {
        let path = scratch("console");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let older = br#"{ "console": { "docked": true, "height": 412 }, "views_width": 700,
            "extensions": { "open": true, "modes": { "docker/containers": "stage", "docker/images": "panel" } } }"#;
        std::fs::write(&path, older).unwrap();
        let loaded = load_from(&path);
        assert_eq!((loaded.console, loaded.views_width), (None, None), "read once, never written");
        let docks = loaded.docks.clone().expect("made from them");
        let bottom = docks.dock(Dock::Bottom);
        assert_eq!((bottom.open, bottom.active.clone(), bottom.size), (true, Some(Panel::Console), 412));
        assert_eq!(docks.right.tabs, [Panel::Views, Panel::extension("docker", "images")]);
        assert_eq!(docks.right.size, 700);
        assert_eq!(docks.staged, [Panel::extension("docker", "containers")]);
        assert!(loaded.extensions.open, "the fold stays unfolded");
        let json = serde_json::to_value(&loaded).unwrap();
        assert!(json.get("console").is_none() && json.get("views_width").is_none());
        assert!(json["extensions"].get("modes").is_none());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_file_from_before_the_settings_dialog_looks_as_the_window_always_did() {
        let p = Preferences::default();
        assert_eq!(p.theme, Theme::System, "the system's light or dark");
        assert_eq!(p.opacity, 100, "solid");
        assert_eq!(p.backdrop, Backdrop::Clear);
        assert_eq!(p.feature_layout, Layout::List);
        assert!(p.notify && p.notify_in_background);
    }

    #[test]
    fn following_the_system_is_mocha_in_the_dark_and_latte_in_the_light() {
        assert_eq!(Theme::System.resolve(true), Theme::Mocha);
        assert_eq!(Theme::System.resolve(false), Theme::Latte);
        assert_eq!(Theme::Frappe.resolve(false), Theme::Frappe, "a flavour chosen is kept");
        assert!(!Theme::Latte.is_dark(true));
        assert!(Theme::Macchiato.is_dark(false));
        assert_eq!(Theme::Night.resolve(false), Theme::Night, "Night is chosen, never the system's");
        assert!(Theme::Night.is_dark(false));
    }

    #[test]
    fn the_settings_file_names_each_choice_as_the_page_does() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(json["theme"], "system");
        assert_eq!(json["backdrop"], "clear");
        assert_eq!(json["editor"], "floating");
        assert_eq!(json["editorCommand"], "");
        assert_eq!(json["notifyInBackground"], true);
        assert_eq!(json["featureLayout"], "list");
    }
}
