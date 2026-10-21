//! The page's editor and the daemon, joined: what the editor asks put to
//! the daemon as it is, and the daemon's answers handed back the same way.
//! The editor's buffer is the page's — a key there must not wait on this
//! process, let alone the daemon — so nothing here is kept, and none of it
//! passes through the session: it would rebuild every page it holds for
//! each change typed.

use argus_protocol::{CheckoutId, ClientMsg, ServerMsg, TextChange};
use serde::{Deserialize, Serialize};

/// What the page's editor asks of the daemon about one file.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum EditGesture {
    Open { checkout: u64, path: String },
    /// Changes made on version `base`, to be written.
    Change { checkout: u64, path: String, base: u64, changes: Vec<TextChange> },
    Close { checkout: u64, path: String },
}

impl EditGesture {
    pub fn message(self) -> ClientMsg {
        match self {
            EditGesture::Open { checkout, path } => ClientMsg::OpenEdit { checkout: CheckoutId(checkout), path },
            EditGesture::Change { checkout, path, base, changes } => {
                ClientMsg::Edit { checkout: CheckoutId(checkout), path, base, changes }
            }
            EditGesture::Close { checkout, path } => ClientMsg::CloseEdit { checkout: CheckoutId(checkout), path },
        }
    }
}

/// What the daemon said of a file the page's editor has open, as the
/// protocol's messages say it; `Lost` when the connection went, taking
/// any answer still owed with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "on")]
pub enum EditEvent {
    Opened { checkout: u64, path: String, version: u64, text: String },
    Changed { checkout: u64, path: String, from: u64, version: u64, changes: Vec<TextChange> },
    Saved { checkout: u64, path: String, base: u64, version: u64 },
    Refused { checkout: u64, path: String, base: u64, reason: Option<String> },
    Closed { checkout: u64, path: String, reason: String },
    Lost,
}

/// Whether `msg` is the editor's, to be passed to the page by `event`.
pub fn is_edit(msg: &ServerMsg) -> bool {
    matches!(
        msg,
        ServerMsg::EditOpened(_)
            | ServerMsg::EditChanged(_)
            | ServerMsg::EditSaved { .. }
            | ServerMsg::EditRefused { .. }
            | ServerMsg::EditClosed { .. }
    )
}

/// `msg` as the editor's, or `None` when it is not.
pub fn event(msg: ServerMsg) -> Option<EditEvent> {
    Some(match msg {
        ServerMsg::EditOpened(o) => EditEvent::Opened { checkout: o.checkout.0, path: o.path, version: o.version, text: o.text },
        ServerMsg::EditChanged(c) => EditEvent::Changed {
            checkout: c.checkout.0,
            path: c.path,
            from: c.from,
            version: c.version,
            changes: c.changes,
        },
        ServerMsg::EditSaved { checkout, path, base, version } => EditEvent::Saved { checkout: checkout.0, path, base, version },
        ServerMsg::EditRefused { checkout, path, base, reason } => EditEvent::Refused { checkout: checkout.0, path, base, reason },
        ServerMsg::EditClosed { checkout, path, reason } => EditEvent::Closed { checkout: checkout.0, path, reason },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editors_answers_are_taken_and_every_other_message_left() {
        let saved = ServerMsg::EditSaved { checkout: CheckoutId(2), path: "a.rs".into(), base: 4, version: 5 };
        assert!(is_edit(&saved));
        assert_eq!(event(saved), Some(EditEvent::Saved { checkout: 2, path: "a.rs".into(), base: 4, version: 5 }));
        assert!(!is_edit(&ServerMsg::Stopping));
    }

    #[test]
    fn a_change_from_the_page_reaches_the_daemon_as_it_was_made() {
        let change = TextChange { from: 1, to: 2, insert: "x".into() };
        let msg = EditGesture::Change { checkout: 3, path: "a.rs".into(), base: 7, changes: vec![change.clone()] }.message();
        assert!(matches!(msg, ClientMsg::Edit { checkout: CheckoutId(3), base: 7, changes, .. } if changes == [change]));
    }
}
