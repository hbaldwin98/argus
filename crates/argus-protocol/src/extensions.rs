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

use crate::focus::Focus;
use crate::hook::Report;
use crate::ids::{CheckoutId, RepositoryId};

/// The longest an extension's name may be. It names the extension in a URL
/// and on a tab, so it is kept to something a person would type.
pub const MAX_EXTENSION_NAME: usize = 32;

/// How many views, rows, commands and dialog items one extension may hold.
/// A bound rather than trust: the daemon forwards every one to every
/// client, and a runaway loop in an extension should cost it its view rather
/// than every client its memory.
pub const MAX_VIEWS: usize = 16;
pub const MAX_ROWS: usize = 2000;
pub const MAX_COMMANDS: usize = 64;
pub const MAX_ITEMS: usize = 500;

/// Whether `name` may name an extension: lowercase letters, digits and
/// dashes, starting with a letter or digit, so it is a URL segment and a
/// config key as it stands.
pub fn is_extension_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_EXTENSION_NAME
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// One tab an extension publishes: a table of rows, and what can be done to
/// them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub rows: Vec<ExtRow>,
    #[serde(default)]
    pub actions: Vec<ExtAction>,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtRow {
    /// Names the row in an action sent back, so an action means the row it
    /// was taken on however the rows were reordered since.
    pub key: String,
    /// One per column, in their order.
    #[serde(default)]
    pub cells: Vec<String>,
    /// Drawn as a pane's status is, in the pane API's words.
    #[serde(default)]
    pub status: Option<Report>,
}

/// Something the person can do in a view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtAction {
    /// Names the action in what is sent back.
    pub id: String,
    pub label: String,
    /// One character, heard only while the view is the stage up.
    #[serde(default)]
    pub key: Option<String>,
    /// Acts on the view rather than on its selected row.
    #[serde(default)]
    pub view_wide: bool,
    /// The question asked before it is sent, when it cannot be taken back.
    #[serde(default)]
    pub confirm: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtItem {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtField {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub initial: String,
}

/// What the person answered. Cancelling is not an answer: it reaches the
/// extension as [`ExtEvent::Cancel`].
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "columns": ["name", "state"],
            "rows": [{"key": "web", "cells": ["web", "up"], "status": "working"}],
            "actions": [{"id": "restart", "label": "Restart", "key": "r", "confirm": "Restart web?"}]
        }"#;
        let view: ExtView = serde_json::from_str(json).unwrap();
        assert_eq!(view.scope, ExtScope::Repository(RepositoryId(4)));
        assert_eq!(view.rows[0].status, Some(Report::Working));
        assert!(!view.actions[0].view_wide);
        assert_eq!(view.extension, "", "the daemon names it");
        let bare: ExtView = serde_json::from_str(r#"{"name":"a","title":"A"}"#).unwrap();
        assert_eq!(bare.scope, ExtScope::Global);
    }

    #[test]
    fn a_view_survives_the_daemons_wire() {
        let view = ExtView {
            extension: "docker".into(),
            name: "containers".into(),
            title: "Containers".into(),
            scope: ExtScope::Checkout(CheckoutId(9)),
            columns: vec!["name".into()],
            rows: vec![ExtRow { key: "web".into(), cells: vec!["web".into()], status: None }],
            actions: vec![],
        };
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
    fn a_characters_shift_is_its_own_and_a_named_keys_is_asked() {
        let upper = ExtKey::plain("D");
        assert!(upper.same_press(&ExtKey { shift: true, ..ExtKey::plain("D") }));
        assert!(!ExtKey::plain("F5").same_press(&ExtKey { shift: true, ..ExtKey::plain("F5") }));
        assert_eq!(ExtKey { ctrl: true, ..ExtKey::plain("d") }.spoken(), "Ctrl D");
        assert_eq!(ExtKey::plain("t").spoken(), "t");
    }
}
