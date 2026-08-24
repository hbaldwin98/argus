//! Agent status via CLI hooks instead of output scraping (DESIGN.md §8b,
//! §11). Currently understands Claude Code's hook dialect only; other
//! templates fall back to coarse process-state status. At spawn, merges a
//! managed `UserPromptSubmit`/`Stop`/`Notification` block into the
//! checkout's `.claude/settings.local.json`, each pointed at the daemon's
//! loopback status receiver (see `state::start_hook_server`) tagged with
//! this pane's id so the response updates the right row.

use std::path::Path;

use orion_protocol::PaneId;
use serde_json::{json, Value};

#[cfg(windows)]
const CURL: &str = "curl.exe";
#[cfg(unix)]
const CURL: &str = "curl";

/// Only the events the daemon's hook receiver understands (`state.rs`'s
/// `parse_hook_path`). Keep in sync with that match.
const MANAGED_EVENTS: [&str; 3] = ["UserPromptSubmit", "Stop", "Notification"];

pub fn install_claude_hooks(checkout: &Path, pane: PaneId, port: u16, token: &str) -> anyhow::Result<()> {
    let dir = checkout.join(".claude");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("settings.local.json");

    let mut root: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}));
    if !root.is_object() {
        root = json!({});
    }
    let root_obj = root.as_object_mut().expect("just normalized to an object");

    let hooks = root_obj.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks_obj = hooks.as_object_mut().expect("just normalized to an object");

    for event in MANAGED_EVENTS {
        hooks_obj.insert(event.to_string(), json!([{ "hooks": [hook_entry(pane, port, token, event)] }]));
    }

    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

fn hook_entry(pane: PaneId, port: u16, token: &str, event: &str) -> Value {
    json!({
        "type": "command",
        "command": CURL,
        "args": [
            "-s", "-X", "POST",
            format!("http://127.0.0.1:{port}/hook/{}/{event}", pane.0),
            "-H", format!("Authorization: Bearer {token}"),
        ],
        "timeout": 5
    })
}
