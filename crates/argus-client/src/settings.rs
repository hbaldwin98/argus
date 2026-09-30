//! Client-side preferences, and where they live on disk.
//!
//! The daemon's `projects.toml` is about what exists; this is about how one
//! client draws and behaves, so it is a file of its own and is never
//! rewritten by the daemon.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where an editor opens (DESIGN.md §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMode {
    /// A floating window over the columns. The default: a terminal editor
    /// in a 38%-wide column is unusable.
    Overlay,
    /// A pane in the rightmost column, alongside the tree.
    Column,
    /// Launched outside Argus entirely, with no pty — for editors that
    /// bring their own window.
    External,
}

/// How this client calls attention to a background agent transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationMode {
    Off,
    Bell,
}

impl NotificationMode {
    pub const ALL: &'static [NotificationMode] = &[NotificationMode::Off, NotificationMode::Bell];

    pub fn label(self) -> &'static str {
        match self {
            NotificationMode::Off => "off",
            NotificationMode::Bell => "terminal bell",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            NotificationMode::Off => "state changes stay inside the Argus window",
            NotificationMode::Bell => "ring when a background agent needs attention",
        }
    }

    pub fn step(self, delta: isize) -> Self {
        let here = NotificationMode::ALL
            .iter()
            .position(|mode| *mode == self)
            .unwrap_or(0) as isize;
        let n = NotificationMode::ALL.len() as isize;
        NotificationMode::ALL[(((here + delta) % n + n) % n) as usize]
    }
}

impl EditorMode {
    pub const ALL: &'static [EditorMode] = &[
        EditorMode::Overlay,
        EditorMode::Column,
        EditorMode::External,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EditorMode::Overlay => "floating window",
            EditorMode::Column => "in the column",
            EditorMode::External => "outside argus",
        }
    }

    /// What choosing this actually does, for the settings panel — a label
    /// alone leaves the reader guessing.
    pub fn detail(self) -> &'static str {
        match self {
            EditorMode::Overlay => "a large panel above the tree; best for vim, helix, emacs",
            EditorMode::Column => "shares the rightmost column with the live pane",
            EditorMode::External => "spawn and forget; for editors with their own window",
        }
    }

    pub fn next(self) -> Self {
        let i = EditorMode::ALL.iter().position(|m| *m == self).unwrap_or(0);
        EditorMode::ALL[(i + 1) % EditorMode::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let i = EditorMode::ALL.iter().position(|m| *m == self).unwrap_or(0);
        EditorMode::ALL[(i + EditorMode::ALL.len() - 1) % EditorMode::ALL.len()]
    }

    /// Whether the daemon should spawn this without a pty.
    pub fn is_external(self) -> bool {
        self == EditorMode::External
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub editor: EditorMode,
    /// The command to run, flags and all — `nvim`, `code -w`, or a full
    /// path. Empty means fall back to `$VISUAL`/`$EDITOR`, then to
    /// whichever terminal editor is installed.
    pub editor_cmd: String,
    /// A preset name from `theme::THEMES`.
    pub theme: String,
    /// Whether review pairs the two sides of a change side by side instead
    /// of stacking them. Unified by default: it is the shape git itself
    /// prints, and it reads at any width.
    pub review_split: bool,
    /// Audible attention signal. Off by default: attaching another client
    /// must not make an existing session unexpectedly noisy.
    pub notifications: NotificationMode,
    /// Machines this client has reached over ssh, offered by the host
    /// picker beside the ones the user's ssh config names. Written only by
    /// [`remember_host`]; see [`save`].
    pub hosts: Vec<String>,
    /// The rail's width as last dragged, `None` for the default. The frame
    /// still clamps it, so a width chosen on a wide terminal cannot push the
    /// stage off a narrow one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rail_width: Option<u16>,
    /// How `argus web` serves, when its flags do not say.
    #[serde(skip_serializing_if = "WebSettings::is_empty")]
    pub web: WebSettings,
}

/// `[web]` in `client.toml`: the defaults `argus web` falls back on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebSettings {
    /// The port to bind, `7420` when unset.
    pub port: Option<u16>,
    /// The address to bind, loopback when unset.
    pub listen: Option<String>,
    /// The address a phone opens, when a proxy puts another in front.
    pub url: Option<String>,
}

impl WebSettings {
    fn is_empty(&self) -> bool {
        *self == WebSettings::default()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            editor: EditorMode::Overlay,
            editor_cmd: String::new(),
            theme: crate::theme::THEMES[0].to_string(),
            review_split: false,
            notifications: NotificationMode::Off,
            hosts: Vec::new(),
            rail_width: None,
            web: WebSettings::default(),
        }
    }
}

pub fn path() -> PathBuf {
    argus_protocol::config_dir().join("client.toml")
}

/// Missing or unreadable settings fall back to the defaults rather than
/// stopping the client — a corrupt preference file should cost you your
/// preferences, not your session.
pub fn load() -> Settings {
    load_from(&path())
}

/// Best-effort: failing to remember a preference is not worth failing the
/// change the user just made.
pub fn save(settings: &Settings) {
    save_to(&path(), settings);
}

/// Adds `host` to the hosts the picker offers, if it is not there yet.
pub fn remember_host(host: &str) {
    remember_host_in(&path(), host);
}

fn load_from(p: &Path) -> Settings {
    let Ok(raw) = std::fs::read_to_string(p) else {
        return Settings::default();
    };
    match toml::from_str(&raw) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("ignoring {}: {e}", p.display());
            Settings::default()
        }
    }
}

fn save_to(p: &Path, settings: &Settings) {
    // Each host's app holds a copy of the settings loaded when it started,
    // and a copy's hosts are only as fresh as that. The file's own list is
    // kept, so a theme changed on one host cannot forget a host another
    // one remembered. `[web]` is kept for the same reason, and because no
    // client ever edits it: it is written by hand.
    let on_disk = load_from(p);
    let settings = Settings {
        hosts: on_disk.hosts,
        web: on_disk.web,
        ..settings.clone()
    };
    write(p, &settings);
}

fn remember_host_in(p: &Path, host: &str) {
    let mut settings = load_from(p);
    if settings.hosts.iter().any(|known| known == host) {
        return;
    }
    settings.hosts.push(host.to_string());
    write(p, &settings);
}

fn write(p: &Path, settings: &Settings) {
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match toml::to_string_pretty(settings) {
        Ok(text) => {
            if let Err(e) = std::fs::write(p, text) {
                tracing::warn!("could not save settings to {}: {e}", p.display());
            }
        }
        Err(e) => tracing::warn!("could not serialize settings: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_editor_is_the_floating_window() {
        // The column is too narrow for a terminal editor, which is the
        // whole reason the overlay exists.
        assert_eq!(Settings::default().editor, EditorMode::Overlay);
    }

    #[test]
    fn cycling_the_editor_mode_visits_every_option_and_comes_back() {
        let mut m = EditorMode::Overlay;
        for _ in 0..EditorMode::ALL.len() {
            m = m.next();
        }
        assert_eq!(m, EditorMode::Overlay);
    }

    #[test]
    fn next_and_prev_are_inverses() {
        for m in EditorMode::ALL {
            assert_eq!(m.next().prev(), *m);
        }
    }

    #[test]
    fn only_the_external_mode_skips_the_pty() {
        assert!(EditorMode::External.is_external());
        assert!(!EditorMode::Overlay.is_external());
        assert!(!EditorMode::Column.is_external());
    }

    #[test]
    fn every_mode_explains_itself() {
        // The panel exists to be read; a bare enum name would not help.
        for m in EditorMode::ALL {
            assert!(!m.label().is_empty());
            assert!(!m.detail().is_empty());
        }
    }

    #[test]
    fn an_unset_command_means_look_at_the_environment() {
        // Not a default of "vi": the daemon's own resolution is better
        // informed than any guess made here.
        assert!(Settings::default().editor_cmd.is_empty());
    }

    #[test]
    fn settings_survive_a_round_trip_through_toml() {
        let s = Settings {
            editor: EditorMode::External,
            editor_cmd: "code -w".to_string(),
            theme: "latte".to_string(),
            review_split: true,
            notifications: NotificationMode::Bell,
            hosts: vec!["devbox".to_string()],
            rail_width: Some(40),
            web: WebSettings {
                port: Some(7500),
                ..Default::default()
            },
        };
        let back: Settings = toml::from_str(&toml::to_string_pretty(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    /// A settings file of its own, so a test never reads or writes the
    /// user's.
    fn scratch_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("argus-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("client.toml")
    }

    #[test]
    fn a_remembered_host_is_offered_once() {
        let file = scratch_file("remember");
        remember_host_in(&file, "devbox");
        remember_host_in(&file, "devbox");
        remember_host_in(&file, "buildbox");
        assert_eq!(load_from(&file).hosts, ["devbox", "buildbox"]);
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn saving_another_preference_keeps_the_hosts_on_disk() {
        // Each host's app has its own copy of the settings; one saving a
        // theme must not forget a host another remembered since.
        let file = scratch_file("keep");
        let copy = load_from(&file);
        remember_host_in(&file, "devbox");
        save_to(
            &file,
            &Settings {
                theme: "latte".to_string(),
                ..copy
            },
        );
        let saved = load_from(&file);
        assert_eq!(saved.theme, "latte");
        assert_eq!(saved.hosts, ["devbox"]);
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn a_file_missing_a_key_keeps_the_default_for_it() {
        // Settings files outlive the versions that wrote them.
        let s: Settings = toml::from_str(r#"theme = "frappe""#).unwrap();
        assert_eq!(s.theme, "frappe");
        assert_eq!(s.editor, Settings::default().editor);
        assert!(!s.review_split);
        assert_eq!(s.notifications, NotificationMode::Off);
    }

    #[test]
    fn a_file_from_the_column_layout_still_loads() {
        // Column widths, folds, dragged feature panels and the pane
        // column's grouping were the five-column spine's; a file that
        // remembers them is read for everything else it says.
        let s: Settings = toml::from_str(
            "theme = \"latte\"\ncolumn_widths = [12, 16, 18, 24, 46]\nfolded_columns = 1\n\
             feature_panel_heights = [6, 12, 10]\npane_view = \"flat\"",
        )
        .unwrap();
        assert_eq!(s.theme, "latte");
    }

    #[test]
    fn an_unparseable_value_is_an_error_rather_than_a_silent_default() {
        // `load` turns this into a warning; the parse itself must not
        // quietly invent a mode.
        assert!(toml::from_str::<Settings>(r#"editor = "telepathy""#).is_err());
    }

    #[test]
    fn notification_modes_cycle_in_both_directions() {
        assert_eq!(NotificationMode::Off.step(1), NotificationMode::Bell);
        assert_eq!(NotificationMode::Off.step(-1), NotificationMode::Bell);
        assert_eq!(NotificationMode::Bell.step(1), NotificationMode::Off);
    }
}
