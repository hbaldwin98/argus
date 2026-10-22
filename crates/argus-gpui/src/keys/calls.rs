//! What a key of the window asks of the session: the call the page makes for each action of the
//! Anywhere and Window places (`run` in `page/src/keys/keymap.ts`).
//!
//! The stage switches, the console's keys and the composer each have a call of their own, as the
//! page sends them; the zoom keys and the extension view's mode are the session's commands. An
//! action no key of the window sends a call for (a table's rows, a feature's lenses) belongs to a
//! stage that is not in this window yet.

use argus_desktop_core::connection::page::Call;
use argus_desktop_core::keys::keymap::Action;
use argus_desktop_core::panes::console::ConsoleGesture;

pub fn of(action: Action) -> Option<Call> {
    Some(match action {
        Action::Workspace => Call::ShowFeatures { open: false },
        Action::Feature => Call::ShowFeatures { open: true },
        Action::Checkouts => Call::ShowCheckouts { open: true },
        Action::Code => Call::ShowCode { open: true },
        Action::Compose => Call::ShowComposer { open: true },
        Action::Console => Call::Console { gesture: ConsoleGesture::Tab },
        Action::ToggleConsole => Call::Console { gesture: ConsoleGesture::Toggle },
        Action::ConsoleFullscreen => Call::Console { gesture: ConsoleGesture::Fullscreen },
        Action::NewConsole => Call::Console { gesture: ConsoleGesture::Spawn },
        Action::Views => Call::ToggleViews,
        // The page falls to the stage's own table for the Browsers key and finds nothing there; the
        // session goes to the stage, as the sheet says the key does.
        Action::Browsers | Action::ExtensionPage | Action::ExtensionDock => Call::Command { action },
        Action::ZoomIn | Action::ZoomOut | Action::ZoomReset => Call::Command { action },
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
