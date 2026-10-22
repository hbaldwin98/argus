//! Which extension command a key is, and running it: the page's `extensionCommand`
//! (`keys/keymap.ts`), for the layers whose keys it falls to when they bind nothing (SHELL-KEYS-16,
//! 17, 21).
//!
//! The session lists the commands (`Event::Extensions`); the daemon refused any key the keymap
//! holds, so none shadows one of the window's own, and a layer asks only after its own bindings
//! found nothing. Which command a key is, in which scopes, is core's (`extensions/keys`). A
//! command's `view` scope is heard in its view's layer (`extensions/keys`), so the scopes asked
//! here are `anywhere`, `checkout` and `pane`. Where the person is (the rail's picked row) is the
//! caller's to say, as the page's `runExtensionCommand` reads it, and the session resolves the rest.

use std::cell::RefCell;
use std::rc::Rc;

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::extensions::keys::{command_in, ExtCommandPage};
use argus_desktop_core::keys::page_key::PageKey;
use gpui_kit::{App, Global};

use super::layer::Layers;

pub const ANYWHERE: &str = "anywhere";
pub const CHECKOUT: &str = "checkout";
pub const PANE: &str = "pane";

/// The commands the session last listed. A handle: clones share one list.
#[derive(Clone, Default)]
pub struct Commands(Rc<RefCell<Vec<ExtCommandPage>>>);

impl Global for Commands {}

impl Commands {
    pub fn of(cx: &mut App) -> Self {
        cx.default_global::<Commands>().clone()
    }

    pub fn set(&self, commands: Vec<ExtCommandPage>) {
        *self.0.borrow_mut() = commands;
    }

    /// The extension and name of the first command heard in `scopes` whose key is `page`.
    fn find(&self, scopes: &[&str], page: &PageKey) -> Option<(String, String)> {
        command_in(&self.0.borrow(), scopes, None, page).map(|c| (c.extension.clone(), c.command.clone()))
    }
}

/// Runs the command in `scopes` that `page` is, asking the session with the row the rail has
/// picked (`checkout`, `pane`); whether there was one.
pub fn run(scopes: &[&str], page: &PageKey, checkout: Option<u64>, pane: Option<u64>, cx: &mut App) -> bool {
    let Some((extension, command)) = Commands::of(cx).find(scopes, page) else { return false };
    send(extension, command, checkout, pane, cx)
}

/// Asks the session to run `extension`'s `command` where the person is; whether there was somewhere to ask.
pub fn send(extension: String, command: String, checkout: Option<u64>, pane: Option<u64>, cx: &mut App) -> bool {
    Layers::of(cx).call(Call::ExtensionCommand { extension, command, checkout, pane })
}

#[cfg(test)]
pub(crate) mod tests;
