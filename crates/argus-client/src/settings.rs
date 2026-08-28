//! Client-side preferences, and where they live on disk.
//!
//! The daemon's `projects.toml` is about what exists; this is about how one
//! client draws and behaves, so it is a file of its own and is never
//! rewritten by the daemon.

use std::path::PathBuf;

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

/// Which panes the panes column lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaneView {
    /// Only panes in the selected checkout.
    Checkout,
    /// Every pane in the open workspace, with its checkout shown on the row.
    Flat,
}

impl PaneView {
    pub fn label(self) -> &'static str {
        match self {
            PaneView::Checkout => "by checkout",
            PaneView::Flat => "all panes",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            PaneView::Checkout => "the panes column follows the selected checkout",
            PaneView::Flat => "one list across every checkout in the open workspace",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            PaneView::Checkout => PaneView::Flat,
            PaneView::Flat => PaneView::Checkout,
        }
    }
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
    /// Preferred outer widths for projects, repositories, checkouts, panes,
    /// and content. A vector lets older four-column files deserialize; the
    /// renderer discards lengths that do not match the current layout.
    /// Absent until the user first drags a column separator.
    pub column_widths: Option<Vec<u16>>,
    /// Whether the projects column is folded away to a left-edge tab, ceding
    /// its width to the other four columns. Remembered so the layout a user
    /// settled on survives a restart.
    pub projects_collapsed: bool,
    /// Whether panes are grouped by the selected checkout or listed across
    /// the whole workspace.
    pub pane_view: PaneView,
    /// Audible attention signal. Off by default: attaching another client
    /// must not make an existing session unexpectedly noisy.
    pub notifications: NotificationMode,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            editor: EditorMode::Overlay,
            editor_cmd: String::new(),
            theme: crate::theme::THEMES[0].to_string(),
            column_widths: None,
            projects_collapsed: false,
            pane_view: PaneView::Checkout,
            notifications: NotificationMode::Off,
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
    let Ok(raw) = std::fs::read_to_string(path()) else {
        return Settings::default();
    };
    match toml::from_str(&raw) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("ignoring {}: {e}", path().display());
            Settings::default()
        }
    }
}

/// Best-effort: failing to remember a preference is not worth failing the
/// change the user just made.
pub fn save(settings: &Settings) {
    let p = path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match toml::to_string_pretty(settings) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&p, text) {
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
            column_widths: Some(vec![12, 16, 18, 24, 46]),
            projects_collapsed: true,
            pane_view: PaneView::Flat,
            notifications: NotificationMode::Bell,
        };
        let back: Settings = toml::from_str(&toml::to_string_pretty(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn a_file_missing_a_key_keeps_the_default_for_it() {
        // Settings files outlive the versions that wrote them.
        let s: Settings = toml::from_str(r#"theme = "frappe""#).unwrap();
        assert_eq!(s.theme, "frappe");
        assert_eq!(s.editor, Settings::default().editor);
        assert_eq!(s.column_widths, None);
        assert_eq!(s.notifications, NotificationMode::Off);
        assert_eq!(s.pane_view, PaneView::Checkout);
    }

    #[test]
    fn old_four_column_widths_deserialize_for_safe_runtime_migration() {
        let s: Settings = toml::from_str("column_widths = [12, 18, 24, 46]").unwrap();
        assert_eq!(s.column_widths, Some(vec![12, 18, 24, 46]));
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
