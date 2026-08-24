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

#[cfg(test)]
mod tests {
    use super::*;

    fn read_settings(dir: &Path) -> Value {
        let raw = std::fs::read_to_string(dir.join(".claude").join("settings.local.json")).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn installs_one_entry_per_managed_event() {
        let dir = tempfile::tempdir().unwrap();
        install_claude_hooks(dir.path(), PaneId(3), 5555, "tok").unwrap();

        let hooks = read_settings(dir.path())["hooks"].clone();
        for event in MANAGED_EVENTS {
            assert!(hooks.get(event).is_some(), "{event} hook should be installed");
        }
        assert_eq!(hooks.as_object().unwrap().len(), MANAGED_EVENTS.len());
    }

    #[test]
    fn hook_command_targets_this_pane_on_loopback_with_the_token() {
        let dir = tempfile::tempdir().unwrap();
        install_claude_hooks(dir.path(), PaneId(42), 5555, "sekrit").unwrap();

        let entry = read_settings(dir.path())["hooks"]["Stop"][0]["hooks"][0].clone();
        assert_eq!(entry["command"], CURL);
        assert_eq!(entry["type"], "command");
        let args: Vec<String> = serde_json::from_value(entry["args"].clone()).unwrap();
        assert!(
            args.contains(&"http://127.0.0.1:5555/hook/42/Stop".to_string()),
            "url must carry pane id and event: {args:?}"
        );
        assert!(
            args.contains(&"Authorization: Bearer sekrit".to_string()),
            "bearer token must be sent: {args:?}"
        );
        // `args` present means exec form — no shell involved, which is why
        // CURL must name the executable exactly (`curl.exe` on Windows).
        assert!(entry["args"].is_array());
    }

    #[test]
    fn preserves_unrelated_settings_and_unmanaged_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("settings.local.json"),
            r#"{"permissions":{"allow":["Bash"]},"hooks":{"SessionStart":[{"hooks":[]}]}}"#,
        )
        .unwrap();

        install_claude_hooks(dir.path(), PaneId(1), 1234, "tok").unwrap();

        let root = read_settings(dir.path());
        assert_eq!(root["permissions"]["allow"][0], "Bash", "user settings must survive");
        assert!(
            root["hooks"]["SessionStart"].is_array(),
            "hooks we don't manage must survive"
        );
        assert!(root["hooks"]["Stop"].is_array());
    }

    #[test]
    fn reinstalling_replaces_rather_than_appends() {
        // Every agent spawn rewrites these; a stale entry pointing at a dead
        // port/pane must not accumulate across spawns.
        let dir = tempfile::tempdir().unwrap();
        install_claude_hooks(dir.path(), PaneId(1), 1111, "a").unwrap();
        install_claude_hooks(dir.path(), PaneId(2), 2222, "b").unwrap();

        let stop = read_settings(dir.path())["hooks"]["Stop"].clone();
        assert_eq!(stop.as_array().unwrap().len(), 1, "no duplicate matcher blocks");
        let args: Vec<String> = serde_json::from_value(stop[0]["hooks"][0]["args"].clone()).unwrap();
        assert!(args.contains(&"http://127.0.0.1:2222/hook/2/Stop".to_string()));
    }

    #[test]
    fn recovers_from_a_corrupt_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("settings.local.json"), "not json at all {{{").unwrap();

        install_claude_hooks(dir.path(), PaneId(1), 1234, "tok").unwrap();
        assert!(read_settings(dir.path())["hooks"]["Stop"].is_array());
    }

    #[test]
    fn normalizes_a_non_object_hooks_key() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("settings.local.json"), r#"{"hooks":[]}"#).unwrap();

        install_claude_hooks(dir.path(), PaneId(1), 1234, "tok").unwrap();
        assert!(read_settings(dir.path())["hooks"].is_object());
    }

    #[test]
    fn creates_the_claude_dir_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join(".claude").exists());
        install_claude_hooks(dir.path(), PaneId(1), 1234, "tok").unwrap();
        assert!(dir.path().join(".claude").join("settings.local.json").is_file());
    }
}
