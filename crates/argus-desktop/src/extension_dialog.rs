//! The questions extensions put to this window: the one open over the
//! stage, those waiting behind it, the answer or cancellation each sends
//! back, and what the page last heard of them.
//!
//! The daemon puts a dialog to one window only, and withdraws it when its
//! extension goes; this holds what it was put and lets go of a dialog only
//! when it is answered here or withdrawn there.

use argus_protocol::{ClientMsg, ExtAnswer, ExtDialog};
use serde::Serialize;

use crate::connection::page::Event;
use crate::connection::sent::Sent;

/// The question an extension puts to this window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ExtDialogPage {
    pub open: bool,
    pub extension: String,
    pub id: u64,
    pub dialog: Option<argus_protocol::ExtDialog>,
    /// It answers a command the person ran here: it takes the keys. One
    /// raised on its own waits for a click, so a pane being typed in keeps
    /// them.
    pub prompted: bool,
    /// How many more wait behind it.
    pub waiting: usize,
}

struct Asked {
    extension: String,
    id: u64,
    dialog: ExtDialog,
    /// It answers a command the person ran here, so it may take the keys.
    prompted: bool,
}

#[derive(Default)]
pub struct ExtensionDialogs {
    /// Oldest first; the first is the one open.
    asked: Vec<Asked>,
    sent: Sent<ExtDialogPage>,
}

impl ExtensionDialogs {
    pub fn asked(&mut self, extension: String, id: u64, dialog: ExtDialog, prompted: bool) {
        self.asked.retain(|a| a.id != id);
        self.asked.push(Asked { extension, id, dialog, prompted });
    }

    pub fn withdrawn(&mut self, id: u64) {
        self.asked.retain(|a| a.id != id);
    }

    /// A daemon that went cancelled every dialog it had put here.
    pub fn clear(&mut self) {
        self.asked.clear();
    }

    /// The person answered dialog `id`, or put it away with `None`: what
    /// to tell the daemon, if this window holds it.
    pub fn answer(&mut self, id: u64, answer: Option<ExtAnswer>) -> Option<ClientMsg> {
        let at = self.asked.iter().position(|a| a.id == id)?;
        self.asked.remove(at);
        Some(ClientMsg::ExtensionDialogAnswer { dialog: id, answer })
    }

    /// What the page has not heard of the open question.
    pub fn take(&mut self) -> Option<Event> {
        let page = self.page();
        self.sent.take(page).map(|page| Event::ExtensionDialog(Box::new(page)))
    }

    /// The page reloaded and holds nothing: an open question is sent again.
    pub fn reload(&mut self) {
        self.sent.forget();
    }

    fn page(&self) -> Option<ExtDialogPage> {
        let first = self.asked.first()?;
        Some(ExtDialogPage {
            open: true,
            extension: first.extension.clone(),
            id: first.id,
            dialog: Some(first.dialog.clone()),
            prompted: first.prompted,
            waiting: self.asked.len() - 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::ExtDialogKind;

    fn prompt(title: &str) -> ExtDialog {
        ExtDialog { title: title.into(), kind: ExtDialogKind::Prompt { message: "?".into(), initial: String::new() }, answers: None }
    }

    #[test]
    fn the_oldest_question_is_open_and_the_rest_wait() {
        let mut dialogs = ExtensionDialogs::default();
        assert!(dialogs.page().is_none());
        dialogs.asked("docker".into(), 1, prompt("first"), true);
        dialogs.asked("docker".into(), 2, prompt("second"), false);
        let page = dialogs.page().unwrap();
        assert_eq!((page.id, page.waiting, page.prompted), (1, 1, true));
        let sent = dialogs.answer(1, Some(ExtAnswer::Text("prod".into())));
        assert!(matches!(sent, Some(ClientMsg::ExtensionDialogAnswer { dialog: 1, answer: Some(_) })));
        assert_eq!(dialogs.page().unwrap().id, 2);
    }

    #[test]
    fn a_withdrawn_question_goes_and_cannot_be_answered() {
        let mut dialogs = ExtensionDialogs::default();
        dialogs.asked("docker".into(), 1, prompt("first"), false);
        dialogs.withdrawn(1);
        assert!(dialogs.page().is_none());
        assert!(dialogs.answer(1, None).is_none());
    }
}
