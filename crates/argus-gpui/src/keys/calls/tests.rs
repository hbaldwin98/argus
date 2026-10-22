use super::*;
use argus_desktop_core::keys::keymap::{Place, BINDINGS};

fn said(action: Action) -> Option<String> {
    of(action).map(|call| format!("{call:?}"))
}

/// SHELL-KEYS-18: what the page's `run` sends for each key of the Anywhere place.
#[test]
fn each_stage_and_console_key_sends_the_call_the_page_sends() {
    for (action, call) in [
        (Action::Workspace, "ShowFeatures { open: false }"),
        (Action::Feature, "ShowFeatures { open: true }"),
        (Action::Checkouts, "ShowCheckouts { open: true }"),
        (Action::Code, "ShowCode { open: true }"),
        (Action::Compose, "ShowComposer { open: true }"),
        (Action::Console, "Console { gesture: Tab }"),
        (Action::ToggleConsole, "Console { gesture: Toggle }"),
        (Action::ConsoleFullscreen, "Console { gesture: Fullscreen }"),
        (Action::NewConsole, "Console { gesture: Spawn }"),
        (Action::Views, "ToggleViews"),
    ] {
        assert_eq!(said(action).as_deref(), Some(call), "{action:?}");
    }
}

/// SHELL-KEYS-3: the zoom keys are the session's commands.
#[test]
fn the_zoom_keys_are_commands_of_the_session() {
    assert_eq!(said(Action::ZoomIn).as_deref(), Some("Command { action: ZoomIn }"));
    assert_eq!(said(Action::ZoomReset).as_deref(), Some("Command { action: ZoomReset }"));
}

/// The window opens the palette, the sheet and the settings itself, and Escape is the ladder: none
/// of the three is a call.
#[test]
fn what_the_window_does_itself_is_no_call() {
    for action in [Action::Palette, Action::Shortcuts, Action::Settings, Action::CloseLayer] {
        assert!(of(action).is_none(), "{action:?}");
    }
}

/// Every binding of the Anywhere and Window places is either a call here or the window's own
/// (`keys/over`, the ladder), so no key of those places is silently dead.
#[test]
fn every_key_of_the_anywhere_and_window_places_is_answered() {
    for binding in BINDINGS.iter().filter(|binding| matches!(binding.place, Place::Anywhere | Place::Window)) {
        let own = super::super::over::overlay_of(binding.action).is_some() || binding.action == Action::CloseLayer;
        assert!(own || of(binding.action).is_some(), "{:?} in {:?} has no answer", binding.action, binding.place);
    }
}
