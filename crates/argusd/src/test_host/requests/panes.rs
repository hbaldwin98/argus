//! Requests about panes: starting them, watching them, typing into them,
//! sizing, naming and ending them, and opening a file in an editor pane.

use argus_protocol::ClientMsg;
use proptest::prelude::*;

use super::values::{checkout, checkout_path, maybe_text, pane, request_id, text};
use super::Known;

/// The templates every daemon without its own `[[agent]]` blocks offers,
/// and one it does not.
const TEMPLATES: &[&str] = &["claude", "codex", "opencode", "agent", "no-such-template"];

/// An editor no machine has, and no GUI editor by name, so an editor
/// request opens a pane, which the host's processes are scripted for.
/// Left to itself the daemon would take the editor from the environment,
/// and `external`, or a GUI editor, starts a real program on the machine.
const EDITOR: &str = "argus-fuzz-editor";

pub fn subscribe(k: &Known) -> BoxedStrategy<ClientMsg> {
    pane(k).prop_map(|pane| ClientMsg::Subscribe { pane }).boxed()
}

pub fn unsubscribe(k: &Known) -> BoxedStrategy<ClientMsg> {
    pane(k).prop_map(|pane| ClientMsg::Unsubscribe { pane }).boxed()
}

pub fn input(k: &Known) -> BoxedStrategy<ClientMsg> {
    (pane(k), proptest::collection::vec(any::<u8>(), 0..32))
        .prop_map(|(pane, bytes)| ClientMsg::Input { pane, bytes })
        .boxed()
}

pub fn paste(k: &Known) -> BoxedStrategy<ClientMsg> {
    (pane(k), text()).prop_map(|(pane, text)| ClientMsg::Paste { pane, text }).boxed()
}

pub fn paste_file(k: &Known) -> BoxedStrategy<ClientMsg> {
    (pane(k), text(), proptest::collection::vec(any::<u8>(), 0..64))
        .prop_map(|(pane, name, bytes)| ClientMsg::PasteFile { pane, name, bytes })
        .boxed()
}

pub fn scrollback(k: &Known) -> BoxedStrategy<ClientMsg> {
    (pane(k), any::<u32>(), proptest::option::of(any::<u64>()))
        .prop_map(|(pane, offset, top)| ClientMsg::Scrollback { pane, offset, top })
        .boxed()
}

pub fn resize(k: &Known) -> BoxedStrategy<ClientMsg> {
    let side = prop_oneof![4 => 1..300u16, 1 => Just(0u16), 1 => any::<u16>()];
    (pane(k), side.clone(), side)
        .prop_map(|(pane, rows, cols)| ClientMsg::Resize { pane, rows, cols })
        .boxed()
}

pub fn spawn_shell(k: &Known) -> BoxedStrategy<ClientMsg> {
    (checkout(k), request_id())
        .prop_map(|(checkout, request_id)| ClientMsg::SpawnShell { checkout, request_id })
        .boxed()
}

pub fn spawn_agent(k: &Known) -> BoxedStrategy<ClientMsg> {
    (checkout(k), proptest::sample::select(TEMPLATES), maybe_text(), request_id())
        .prop_map(|(checkout, template, prompt, request_id)| ClientMsg::SpawnAgent {
            checkout,
            template: template.to_owned(),
            prompt,
            request_id,
        })
        .boxed()
}

pub fn spawn_console() -> BoxedStrategy<ClientMsg> {
    request_id().prop_map(|request_id| ClientMsg::SpawnConsole { request_id }).boxed()
}

pub fn kill(k: &Known) -> BoxedStrategy<ClientMsg> {
    pane(k).prop_map(|pane| ClientMsg::Kill { pane }).boxed()
}

pub fn rename_pane(k: &Known) -> BoxedStrategy<ClientMsg> {
    (pane(k), text()).prop_map(|(pane, title)| ClientMsg::RenamePane { pane, title }).boxed()
}

pub fn move_to_checkout(k: &Known) -> BoxedStrategy<ClientMsg> {
    (pane(k), checkout(k)).prop_map(|(pane, checkout)| ClientMsg::MoveToCheckout { pane, checkout }).boxed()
}

pub fn open_in_editor(k: &Known) -> BoxedStrategy<ClientMsg> {
    (checkout(k), checkout_path(k), proptest::option::of(0..200u32), request_id())
        .prop_map(|(checkout, path, line, request_id)| ClientMsg::OpenInEditor {
            checkout,
            path,
            line,
            external: false,
            command: Some(EDITOR.to_owned()),
            request_id,
        })
        .boxed()
}
