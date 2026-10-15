//! What an extension publishes for Argus to draw — its views, commands and
//! dialogs — and what the person does with them, on its way back.
//!
//! An extension is a program outside Argus that the daemon starts. It speaks
//! JSON to the pane API (`hook::ExtEndpoint`), and the daemon carries the
//! same types to every client that greeted with `EXTENSIONS`, so the shapes
//! an extension writes, the ones the daemon forwards and the ones each client
//! draws are one set. Everything is declarative: Argus draws a table, a
//! command or a dialog from data and never runs an extension's markup.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::agent_view::{Chip, Column, Value};
use crate::view_check::{self, MAX_TABLE_ROWS, MAX_VIEW_ID_BYTES, MAX_VIEW_TITLE_BYTES};
use crate::focus::Focus;
use crate::ids::{CheckoutId, PaneId, RepositoryId};

/// The longest an extension's name may be. It names the extension in a URL
/// and on a tab, so it is kept to something a person would type.
pub const MAX_EXTENSION_NAME: usize = 32;

/// How many views, commands, actions and dialog items one extension may
/// hold. A bound rather than trust: the daemon forwards every one to every
/// client, and a runaway loop in an extension should cost it its view rather
/// than every client its memory. A view's rows are bounded as an agent
/// view's table is (`view_check::MAX_TABLE_ROWS`), for the same reason.
pub const MAX_VIEWS: usize = 16;
pub const MAX_COMMANDS: usize = 64;
pub const MAX_ACTIONS: usize = 32;
pub const MAX_ITEMS: usize = 500;

/// The longest request an extension may send. Room for a few views each as
/// large as an agent's may be; past it the extension is dumping, not
/// choosing what to show.
pub const MAX_EXTENSION_BODY: usize = 4 * view_check::MAX_VIEW_BYTES;

/// The longest notice, in characters: the status bar has one line for it,
/// so a longer one is cut rather than refused.
pub const MAX_NOTICE_CHARS: usize = 300;

/// The longest line an extension may say what it is in, and what a command
/// or an action does: a line, as a palette's subtitle is.
pub const MAX_SUMMARY_BYTES: usize = 200;
pub const MAX_HELP_BYTES: usize = 200;

/// The longest guide an extension may give to using it: a page or two of
/// Markdown, which `argus extension show` prints whole.
pub const MAX_ABOUT_GUIDE_BYTES: usize = 16 * 1024;

/// Whether `name` may name an extension: lowercase letters, digits and
/// dashes, starting with a letter or digit, so it is a URL segment and a
/// config key as it stands.
pub fn is_extension_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_EXTENSION_NAME
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// One view an extension publishes: a table in the agent views' vocabulary
/// — `Column`s, and cells that are `Value`s — so the window draws it with
/// the renderers an agent's table uses, and what only an extension adds: a
/// scope, a key per row, and actions on them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtView {
    /// The extension that published it. Filled by the daemon from the URL
    /// it was put to, whatever the body said, so one extension cannot
    /// publish under another's name.
    #[serde(default)]
    pub extension: String,
    /// Names the view among its extension's, in what the clients send back.
    pub name: String,
    /// What its tab says.
    pub title: String,
    #[serde(default)]
    pub scope: ExtScope,
    /// Where the desktop window first opens it: docked against one edge of
    /// the stage, or as a page of its own. Only a default: the person may
    /// move any view to another dock or a page, and the window keeps their
    /// choice.
    #[serde(default)]
    pub place: ExtPlace,
    /// One of [`EXT_ICONS`], by name, for its entry in the sidebar. A name
    /// the window does not draw is drawn as a generic icon, so an extension
    /// written for a newer window still shows; never an extension's own
    /// image, which would be markup crossing.
    #[serde(default)]
    pub icon: Option<String>,
    /// A table's columns; none when the view is a console.
    #[serde(default)]
    pub columns: Vec<Column>,
    #[serde(default)]
    pub rows: Vec<ExtRow>,
    /// The view is this console rather than a table. It may still carry
    /// view-wide actions, a Restart say, but no columns, rows or row
    /// actions.
    #[serde(default)]
    pub console: Option<crate::ext_console::ExtConsole>,
    #[serde(default)]
    pub actions: Vec<ExtAction>,
    /// The id of the row action a double-click or Enter on a row takes, as
    /// its key would; the extension decides what it means for the row.
    #[serde(default)]
    pub default: Option<String>,
}

impl ExtView {
    /// The view if it keeps every limit, or what is wrong with it named by
    /// its path (`rows[3][1]`), so the extension can find and fix it — the
    /// way an agent's view is checked.
    pub fn checked(self) -> Result<ExtView, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("name: a view needs a name".into());
        }
        if name.len() > MAX_VIEW_ID_BYTES {
            return Err(format!("name: at most {MAX_VIEW_ID_BYTES} bytes"));
        }
        if self.title.trim().is_empty() || self.title.len() > MAX_VIEW_TITLE_BYTES {
            return Err(format!("title: a view needs one, of at most {MAX_VIEW_TITLE_BYTES} bytes"));
        }
        if let Some(icon) = &self.icon {
            let named = !icon.is_empty()
                && icon.len() <= MAX_ICON_NAME
                && icon.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !named {
                return Err(format!("icon: a name such as {:?}, one of {EXT_ICONS:?}", EXT_ICONS[0]));
            }
        }
        if let Some(console) = &self.console {
            console.check("console")?;
            if !self.columns.is_empty() || !self.rows.is_empty() {
                return Err("console: a console view has no columns or rows".into());
            }
            if let Some(a) = self.actions.iter().position(|a| !a.view_wide) {
                return Err(format!("actions[{a}]: a console view's actions are view-wide"));
            }
        } else if self.columns.is_empty() {
            return Err("columns: a view needs a column, or a console".into());
        }
        if self.rows.len() > MAX_TABLE_ROWS {
            return Err(format!("rows: at most {MAX_TABLE_ROWS}; show the ones that matter"));
        }
        for (r, row) in self.rows.iter().enumerate() {
            if row.key.is_empty() || self.rows[..r].iter().any(|other| other.key == row.key) {
                return Err(format!("rows[{r}].key: each row needs a key of its own, not {:?}", row.key));
            }
            if row.cells.len() != self.columns.len() {
                return Err(format!(
                    "rows[{r}] has {} cells, the view has {} columns",
                    row.cells.len(),
                    self.columns.len()
                ));
            }
            for (c, cell) in row.cells.iter().enumerate() {
                view_check::value_ok(cell, &format!("rows[{r}][{c}]"))?;
            }
        }
        if self.actions.len() > MAX_ACTIONS {
            return Err(format!("actions: at most {MAX_ACTIONS}"));
        }
        for (a, action) in self.actions.iter().enumerate() {
            if action.id.is_empty() || self.actions[..a].iter().any(|other| other.id == action.id) {
                return Err(format!("actions[{a}].id: each action needs an id of its own, not {:?}", action.id));
            }
            help_ok(&action.help, &format!("actions[{a}].help"))?;
        }
        if let Some(default) = &self.default {
            if !self.actions.iter().any(|a| &a.id == default && !a.view_wide) {
                return Err(format!("default: the id of one of the view's row actions, not {default:?}"));
            }
        }
        Ok(ExtView { name: name.to_string(), ..self })
    }
}

/// Where the desktop window first opens a view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtPlace {
    /// Docked on the right, where the agents' views open: the place before
    /// there were docks, kept so an older extension opens where it did.
    #[default]
    Panel,
    /// Docked against the stage's left edge, between it and the sidebar.
    Left,
    /// Docked against the stage's right edge; the same as `Panel`.
    Right,
    /// Docked under the stage.
    Bottom,
    /// A page of its own, filling the stage.
    Stage,
}

/// The icons a view may name, which the desktop window draws itself.
pub const EXT_ICONS: &[&str] = &["box", "server", "database", "globe", "list", "rocket", "terminal", "gear"];

/// The longest an icon's name may be.
pub const MAX_ICON_NAME: usize = 32;

/// Where a view's tab shows: always, or only while a repository or a
/// checkout is the one chosen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtScope {
    #[default]
    Global,
    Repository(RepositoryId),
    Checkout(CheckoutId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtRow {
    /// Names the row in an action sent back, so an action means the row it
    /// was taken on however the rows were reordered since.
    pub key: String,
    /// One per column, in their order.
    pub cells: Vec<Value>,
    /// What state the row's thing is in, in the extension's own words with a
    /// tone. Not the pane API's status words: those say where an agent's
    /// turn is, and a container or a dev server is `running`, `exited` or
    /// `restarting` — the extension knows the word, Argus only the meaning.
    #[serde(default)]
    pub status: Option<Chip>,
}

/// Something the person can do in a view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtAction {
    /// Names the action in what is sent back.
    pub id: String,
    pub label: String,
    /// One character, heard while its view is out in the side panel and
    /// the panel has the keys.
    #[serde(default)]
    pub key: Option<String>,
    /// Acts on the view rather than on its selected row.
    #[serde(default)]
    pub view_wide: bool,
    /// The question asked before it is sent, when it cannot be taken back.
    #[serde(default)]
    pub confirm: Option<String>,
    /// What it does, in a line, for whoever reads the extension through
    /// `argus extension show`.
    #[serde(default)]
    pub help: Option<String>,
}

/// A command an extension adds to the palette, and to the keys when it
/// names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtCommand {
    /// Filled by the daemon, as a view's is.
    #[serde(default)]
    pub extension: String,
    pub name: String,
    /// What the palette and the shortcuts sheet call it.
    pub title: String,
    #[serde(default)]
    pub key: Option<ExtKey>,
    #[serde(default)]
    pub scope: CommandScope,
    /// What it does, in a line, as an action's help says.
    #[serde(default)]
    pub help: Option<String>,
}

impl ExtCommand {
    /// The command if it keeps the limits, or what is wrong with it by its
    /// field. Whether its name is another command's is the daemon's to say,
    /// which holds the whole list.
    pub fn checked(self) -> Result<ExtCommand, String> {
        if self.name.is_empty() {
            return Err("name: a command needs a name".into());
        }
        help_ok(&self.help, "help")?;
        Ok(self)
    }
}

/// Whether a command's or an action's help is one line within its limit.
fn help_ok(help: &Option<String>, path: &str) -> Result<(), String> {
    match help {
        Some(help) if help.len() > MAX_HELP_BYTES || help.contains('\n') => {
            Err(format!("{path}: one line of at most {MAX_HELP_BYTES} bytes"))
        }
        _ => Ok(()),
    }
}

/// What an extension says it is: a line for a listing, and a Markdown guide
/// to using it. Held by the daemon, in memory, for `argus extension show`
/// and the agent reading it there; no window draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtAbout {
    pub summary: String,
    #[serde(default)]
    pub guide: Option<String>,
}

impl ExtAbout {
    pub fn checked(self) -> Result<ExtAbout, String> {
        let summary = self.summary.trim();
        if summary.is_empty() || summary.len() > MAX_SUMMARY_BYTES || summary.contains('\n') {
            return Err(format!("summary: one line of at most {MAX_SUMMARY_BYTES} bytes"));
        }
        if self.guide.as_ref().is_some_and(|guide| guide.len() > MAX_ABOUT_GUIDE_BYTES) {
            return Err(format!("guide: at most {MAX_ABOUT_GUIDE_BYTES} bytes"));
        }
        Ok(ExtAbout { summary: summary.to_string(), ..self })
    }
}

/// Where a command's key is heard, and so what it acts on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandScope {
    /// Wherever the window's own keys are, and through the leader from
    /// inside a pane.
    #[default]
    Anywhere,
    /// Only in the extension's view of this name.
    View(String),
    /// On a checkout row: the Checkouts stage's, or one picked on the rail.
    Checkout,
    /// On a pane row picked on the rail, and through the leader in a pane.
    Pane,
}

/// A key an extension asks for. A character carries its own shift (`D` is
/// shift-d); `shift` is asked only of a named key such as `F5`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtKey {
    /// `KeyboardEvent.key`'s spelling: `d`, `D`, `F5`, `Enter`.
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
}

impl ExtKey {
    pub fn plain(key: &str) -> ExtKey {
        ExtKey { key: key.to_string(), ctrl: false, alt: false, shift: false }
    }

    /// Whether the key is one character, which is typed with its own shift.
    pub fn is_character(&self) -> bool {
        self.key.chars().count() == 1
    }

    /// Two keys are the same press: a character's shift is in the
    /// character, so only a named key's is compared.
    pub fn same_press(&self, other: &ExtKey) -> bool {
        self.key == other.key
            && self.ctrl == other.ctrl
            && self.alt == other.alt
            && (self.is_character() || self.shift == other.shift)
    }

    /// How a person reads it: `Ctrl D`, `Alt Shift F5`, `t`.
    pub fn spoken(&self) -> String {
        let mut words = Vec::new();
        if self.ctrl {
            words.push("Ctrl".to_string());
        }
        if self.alt {
            words.push("Alt".to_string());
        }
        if self.shift && !self.is_character() {
            words.push("Shift".to_string());
        }
        let modified = self.ctrl || self.alt;
        words.push(if modified && self.is_character() { self.key.to_uppercase() } else { self.key.clone() });
        words.join(" ")
    }
}

/// A question an extension asks the person.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtDialog {
    pub title: String,
    pub kind: ExtDialogKind,
    /// The id of the command this answers, from the command's event. A
    /// dialog in answer to a command the person just ran opens at once and
    /// takes the keys; one raised on its own waits to be clicked rather than
    /// taking the keys from a pane being typed in.
    #[serde(default)]
    pub answers: Option<u64>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtDialogKind {
    /// Yes or no; `yes` is what the yes button says.
    Confirm {
        message: String,
        #[serde(default)]
        yes: Option<String>,
    },
    /// One line of text.
    Prompt {
        message: String,
        #[serde(default)]
        initial: String,
    },
    /// One of a list.
    Pick { message: String, items: Vec<ExtItem> },
    /// A few named lines of text, sent back together.
    Form { fields: Vec<ExtField> },
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtItem {
    pub key: String,
    pub label: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtField {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub initial: String,
}

/// What the person answered. Cancelling is not an answer: it reaches the
/// extension as [`ExtEvent::Cancel`].
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtAnswer {
    Yes,
    Text(String),
    Picked(String),
    Form(BTreeMap<String, String>),
}

impl ExtAnswer {
    /// Whether this answers `kind`: a client sending a picked key to a
    /// prompt is refused rather than forwarded as if it had asked.
    pub fn fits(&self, kind: &ExtDialogKind) -> bool {
        match (self, kind) {
            (ExtAnswer::Yes, ExtDialogKind::Confirm { .. }) => true,
            (ExtAnswer::Text(_), ExtDialogKind::Prompt { .. }) => true,
            (ExtAnswer::Picked(key), ExtDialogKind::Pick { items, .. }) => items.iter().any(|i| &i.key == key),
            (ExtAnswer::Form(values), ExtDialogKind::Form { fields }) => {
                values.keys().all(|name| fields.iter().any(|f| &f.name == name))
            }
            _ => false,
        }
    }
}

/// A pane an extension asks for: a shell on a checkout, or a console when
/// it names none, with `run` typed into it — `docker logs -f web`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtPane {
    #[serde(default)]
    pub checkout: Option<CheckoutId>,
    #[serde(default)]
    pub run: Option<String>,
}

/// One running extension as `argus extension show` reads it: what it says
/// it is, and what it publishes. Its views come without their rows, which
/// are what it shows this second rather than what it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtDescription {
    pub name: String,
    pub about: Option<ExtAbout>,
    pub commands: Vec<ExtCommand>,
    pub views: Vec<ExtView>,
}

/// What the daemon answers a dialog with: the id its answer or its
/// cancellation will name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtDialogRaised {
    pub dialog: u64,
}

/// What the daemon answers a pane with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtPaneOpened {
    pub pane: PaneId,
}

/// What the daemon answers a put of commands with: the keys it would not
/// bind, each with why. The command itself is kept, in the palette without
/// its key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refusals {
    pub refused: Vec<Refused>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused {
    /// The command's name, or for a view the action's id.
    pub name: String,
    pub reason: String,
}

/// One thing the person did, on the extension's event stream as one JSON
/// value per server-sent event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ExtEvent {
    /// An action in one of its views, on `row` unless it is view-wide.
    Action { view: String, action: String, row: Option<String> },
    /// A command run, with where the person was: the daemon's own reading
    /// of what the client showed. `id` is what a dialog answering it names.
    Command { command: String, id: u64, context: Option<Box<Focus>> },
    Answer { dialog: u64, answer: ExtAnswer },
    /// A dialog closed without an answer: by the person, or because the
    /// window asking went.
    Cancel { dialog: u64 },
    /// A console view's command ended; `code` is `None` when a signal
    /// ended it. Publishing the view again starts it anew.
    ConsoleEnded { view: String, code: Option<i32> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_is_a_table_or_a_console_but_not_both() {
        let json = r#"{"name": "dev", "title": "Dev server", "console": {"command": ["npm", "run", "dev"]},
                       "actions": [{"id": "restart", "label": "Restart", "view_wide": true}]}"#;
        let view: ExtView = serde_json::from_str(json).unwrap();
        assert!(view.clone().checked().is_ok());
        let mut both = view.clone();
        both.columns = vec![Column { title: "x".into(), numeric: false, optional: false }];
        assert!(both.checked().unwrap_err().starts_with("console:"));
        let mut row_action = view.clone();
        row_action.actions[0].view_wide = false;
        assert!(row_action.checked().unwrap_err().contains("view-wide"));
        let mut neither = view;
        neither.console = None;
        assert!(neither.checked().unwrap_err().starts_with("columns:"));
    }

    #[test]
    fn a_console_ending_is_an_event() {
        let event: ExtEvent = serde_json::from_str(r#"{"event": "console_ended", "view": "dev", "code": 1}"#).unwrap();
        assert_eq!(event, ExtEvent::ConsoleEnded { view: "dev".into(), code: Some(1) });
    }

    #[test]
    fn an_extensions_name_is_a_url_segment() {
        for good in ["docker", "dev-servers", "k8s"] {
            assert!(is_extension_name(good), "{good}");
        }
        for bad in ["", "Docker", "-x", "a/b", "a b", "ü", &"x".repeat(33)] {
            assert!(!is_extension_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_view_reads_from_the_json_an_extension_writes() {
        let json = r#"{
            "name": "containers", "title": "Containers",
            "scope": {"repository": 4},
            "columns": [{"title": "Name"}, {"title": "CPU", "numeric": true}],
            "rows": [{"key": "web",
                      "cells": [{"kind": "text", "text": "web"}, {"kind": "number", "number": 3.5, "unit": "%"}],
                      "status": {"label": "running", "tone": "good"}}],
            "actions": [{"id": "restart", "label": "Restart", "key": "r", "confirm": "Restart web?"}]
        }"#;
        let view: ExtView = serde_json::from_str(json).unwrap();
        let view = view.checked().unwrap();
        assert_eq!(view.scope, ExtScope::Repository(RepositoryId(4)));
        assert_eq!(view.rows[0].status.as_ref().unwrap().tone, crate::agent_view::Tone::Good);
        assert!(view.columns[1].numeric);
        assert!(!view.actions[0].view_wide);
        assert_eq!(view.extension, "", "the daemon names it");
        let bare: ExtView = serde_json::from_str(r#"{"name":"a","title":"A","columns":[{"title":"x"}]}"#).unwrap();
        assert_eq!(bare.scope, ExtScope::Global);
    }

    fn text(t: &str) -> Value {
        Value::Text { text: t.into() }
    }

    fn containers(rows: Vec<ExtRow>) -> ExtView {
        ExtView {
            extension: "docker".into(),
            name: "containers".into(),
            title: "Containers".into(),
            scope: ExtScope::Checkout(CheckoutId(9)),
            place: ExtPlace::Stage,
            icon: Some("box".into()),
            columns: vec![Column { title: "Name".into(), numeric: false, optional: false }],
            rows,
            console: None,
            actions: vec![],
            default: None,
        }
    }

    #[test]
    fn a_views_default_names_one_of_its_row_actions() {
        let action = |id: &str, view_wide: bool| ExtAction {
            id: id.into(),
            label: id.into(),
            key: None,
            view_wide,
            confirm: None,
            help: None,
        };
        let with = |default: &str| ExtView {
            actions: vec![action("start", false), action("prune", true)],
            default: Some(default.into()),
            ..containers(vec![])
        };
        assert!(with("start").checked().is_ok());
        assert!(with("prune").checked().unwrap_err().starts_with("default:"), "view-wide");
        assert!(with("nope").checked().unwrap_err().starts_with("default:"), "no such action");
    }

    fn row(key: &str, cells: Vec<Value>) -> ExtRow {
        ExtRow { key: key.into(), cells, status: None }
    }

    #[test]
    fn a_view_that_breaks_a_limit_is_refused_by_the_path_to_what_broke_it() {
        let short = containers(vec![row("web", vec![])]);
        assert_eq!(short.checked().unwrap_err(), "rows[0] has 0 cells, the view has 1 columns");
        let twice = containers(vec![row("web", vec![text("a")]), row("web", vec![text("b")])]);
        assert!(twice.checked().unwrap_err().starts_with("rows[1].key"));
        let link = Value::Link { label: "x".into(), url: "file:///etc/passwd".into() };
        assert!(containers(vec![row("web", vec![link])]).checked().unwrap_err().starts_with("rows[0][0]"));
        let many = containers((0..=MAX_TABLE_ROWS).map(|i| row(&i.to_string(), vec![text("a")])).collect());
        assert!(many.checked().unwrap_err().starts_with("rows: at most"));
    }

    #[test]
    fn a_view_docks_on_the_right_unless_it_asks_for_another_dock_or_a_stage() {
        let bare: ExtView = serde_json::from_str(r#"{"name":"a","title":"A","columns":[{"title":"x"}]}"#).unwrap();
        assert_eq!((bare.place, bare.icon), (ExtPlace::Panel, None), "an older publisher keeps its place");
        let staged: ExtView =
            serde_json::from_str(r#"{"name":"a","title":"A","place":"stage","icon":"server","columns":[{"title":"x"}]}"#)
                .unwrap();
        assert_eq!(staged.place, ExtPlace::Stage);
        assert!(staged.checked().is_ok());
        let docked: ExtView =
            serde_json::from_str(r#"{"name":"a","title":"A","place":"bottom","columns":[{"title":"x"}]}"#).unwrap();
        assert_eq!(docked.place, ExtPlace::Bottom);
    }

    #[test]
    fn an_icon_is_a_name_and_one_the_window_does_not_draw_is_still_taken() {
        let with = |icon: &str| ExtView { icon: Some(icon.into()), ..containers(vec![]) };
        assert!(with("box").checked().is_ok());
        assert!(with("kubernetes").checked().is_ok(), "drawn as the generic icon");
        for bad in ["", "<svg/>", "Box", &"x".repeat(33)] {
            assert!(with(bad).checked().unwrap_err().starts_with("icon:"), "{bad}");
        }
    }

    #[test]
    fn a_view_survives_the_daemons_wire() {
        let view = containers(vec![row("web", vec![text("web")])]);
        let bytes = rmp_serde::to_vec_named(&view).unwrap();
        assert_eq!(rmp_serde::from_slice::<ExtView>(&bytes).unwrap(), view);
        let dialog = ExtDialog {
            title: "Deploy".into(),
            kind: ExtDialogKind::Pick {
                message: "where to?".into(),
                items: vec![ExtItem { key: "prod".into(), label: "Production".into() }],
            },
            answers: Some(3),
        };
        let bytes = rmp_serde::to_vec_named(&dialog).unwrap();
        assert_eq!(rmp_serde::from_slice::<ExtDialog>(&bytes).unwrap(), dialog);
    }

    #[test]
    fn an_event_reads_as_the_json_an_extension_parses() {
        let action = ExtEvent::Action { view: "containers".into(), action: "restart".into(), row: Some("web".into()) };
        assert_eq!(
            serde_json::to_string(&action).unwrap(),
            r#"{"event":"action","view":"containers","action":"restart","row":"web"}"#
        );
        let answer = ExtEvent::Answer { dialog: 2, answer: ExtAnswer::Picked("prod".into()) };
        assert_eq!(serde_json::to_string(&answer).unwrap(), r#"{"event":"answer","dialog":2,"answer":{"picked":"prod"}}"#);
        let yes = ExtEvent::Answer { dialog: 2, answer: ExtAnswer::Yes };
        assert_eq!(serde_json::to_string(&yes).unwrap(), r#"{"event":"answer","dialog":2,"answer":"yes"}"#);
    }

    #[test]
    fn an_answer_fits_only_the_dialog_that_asked_it() {
        let pick = ExtDialogKind::Pick { message: String::new(), items: vec![ExtItem { key: "a".into(), label: "A".into() }] };
        assert!(ExtAnswer::Picked("a".into()).fits(&pick));
        assert!(!ExtAnswer::Picked("b".into()).fits(&pick), "not one of its items");
        assert!(!ExtAnswer::Yes.fits(&pick));
        let form = ExtDialogKind::Form { fields: vec![ExtField { name: "env".into(), label: "Env".into(), initial: String::new() }] };
        let mut values = BTreeMap::new();
        values.insert("env".to_string(), "prod".to_string());
        assert!(ExtAnswer::Form(values.clone()).fits(&form));
        values.insert("other".to_string(), String::new());
        assert!(!ExtAnswer::Form(values).fits(&form));
    }

    #[test]
    fn help_is_one_line_within_its_limit() {
        let command = |help: &str| ExtCommand {
            extension: String::new(),
            name: "up".into(),
            title: "Up".into(),
            key: None,
            scope: CommandScope::Anywhere,
            help: Some(help.into()),
        };
        assert!(command("starts a compose service").checked().is_ok());
        assert_eq!(command("two\nlines").checked().unwrap_err(), "help: one line of at most 200 bytes");
        assert!(command(&"x".repeat(MAX_HELP_BYTES + 1)).checked().is_err());
        let action = ExtAction {
            id: "restart".into(),
            label: "Restart".into(),
            key: None,
            view_wide: false,
            confirm: None,
            help: Some("x".repeat(MAX_HELP_BYTES + 1)),
        };
        let view = ExtView { actions: vec![action], ..containers(vec![]) };
        assert!(view.checked().unwrap_err().starts_with("actions[0].help:"));
    }

    #[test]
    fn about_is_a_line_and_a_guide_within_their_limits() {
        let about: ExtAbout = serde_json::from_str(r#"{"summary": " Docker containers "}"#).unwrap();
        assert_eq!(about.checked().unwrap(), ExtAbout { summary: "Docker containers".into(), guide: None });
        let blank = ExtAbout { summary: " ".into(), guide: None };
        assert!(blank.checked().unwrap_err().starts_with("summary:"));
        let long = ExtAbout { summary: "s".into(), guide: Some("g".repeat(MAX_ABOUT_GUIDE_BYTES + 1)) };
        assert!(long.checked().unwrap_err().starts_with("guide:"));
    }

    #[test]
    fn a_characters_shift_is_its_own_and_a_named_keys_is_asked() {
        let upper = ExtKey::plain("D");
        assert!(upper.same_press(&ExtKey { shift: true, ..ExtKey::plain("D") }));
        assert!(!ExtKey::plain("F5").same_press(&ExtKey { shift: true, ..ExtKey::plain("F5") }));
        assert_eq!(ExtKey { ctrl: true, ..ExtKey::plain("d") }.spoken(), "Ctrl D");
        assert_eq!(ExtKey::plain("t").spoken(), "t");
    }
}
