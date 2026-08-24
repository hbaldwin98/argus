//! Agent status via CLI hooks instead of output scraping (DESIGN.md §8b,
//! §11). Currently understands Claude Code's hook dialect only; other
//! templates fall back to coarse process-state status. At spawn, merges a
//! managed `UserPromptSubmit`/`Stop`/`Notification` block into the
//! checkout's `.claude/settings.local.json`, each pointed at the daemon's
//! loopback status receiver (see `state::start_hook_server`) tagged with
//! this pane's id so the response updates the right row.
//!
//! **These hooks are per-boot and must not outlive their daemon.** They
//! name an ephemeral port and a per-boot token, so the moment the daemon
//! that wrote them exits they are pointing at nobody. A checkout is a
//! directory the user also runs agents in by hand, so a stale block there
//! doesn't just go quiet — it fires on every turn of every unrelated agent
//! started in that directory afterwards. Hence `uninstall_claude_hooks`,
//! called when the last agent pane in a checkout goes away and again for
//! every configured checkout at daemon startup, and hence the helper
//! command below, which never reports failure even when nothing answers.

use std::path::{Path, PathBuf};

use orion_protocol::PaneId;
use serde_json::{json, Value};

/// The helper that actually posts to the daemon (`src/bin/orion-hook.rs`),
/// resolved next to the running daemon rather than trusted to `PATH` —
/// nothing installs these binaries system-wide. Falls back to the bare name
/// if the daemon's own path can't be read.
fn hook_command() -> String {
    let exe = if cfg!(windows) { "orion-hook.exe" } else { "orion-hook" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe)))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| exe.to_string())
}

/// Only the events the daemon's hook receiver understands (`state.rs`'s
/// `parse_hook_path`). Keep in sync with that match.
const MANAGED_EVENTS: [&str; 3] = ["UserPromptSubmit", "Stop", "Notification"];

fn settings_path(checkout: &Path) -> PathBuf {
    checkout.join(".claude").join("settings.local.json")
}

/// Parses the checkout's settings into an object, normalizing anything
/// unexpected (missing, corrupt, or not a JSON object) to `{}` — this file
/// is the user's, so a broken one must not stop an agent from spawning.
fn read_settings(path: &Path) -> Value {
    let mut root: Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}));
    if !root.is_object() {
        root = json!({});
    }
    root
}

pub fn install_claude_hooks(checkout: &Path, pane: PaneId, port: u16, token: &str) -> anyhow::Result<()> {
    let path = settings_path(checkout);
    std::fs::create_dir_all(path.parent().expect("settings path always has a .claude parent"))?;

    let mut root = read_settings(&path);
    let root_obj = root.as_object_mut().expect("just normalized to an object");

    let hooks = root_obj.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks_obj = hooks.as_object_mut().expect("just normalized to an object");

    let command = hook_command();
    for event in MANAGED_EVENTS {
        hooks_obj.insert(
            event.to_string(),
            json!([{ "hooks": [hook_entry(&command, pane, port, token, event)] }]),
        );
    }

    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

/// Removes Orion's managed hook block from a checkout, leaving anything the
/// user put in the same file untouched. Cleans up after itself as it goes:
/// an emptied `hooks` key is dropped, and a settings file left with nothing
/// in it at all is deleted rather than left behind as `{}`.
///
/// Idempotent, and a no-op when there's nothing there — it runs at startup
/// across every configured checkout, most of which never hosted an agent.
pub fn uninstall_claude_hooks(checkout: &Path) -> anyhow::Result<()> {
    let path = settings_path(checkout);
    if !path.exists() {
        return Ok(());
    }

    let mut root = read_settings(&path);
    let root_obj = root.as_object_mut().expect("just normalized to an object");

    let mut removed = false;
    if let Some(hooks) = root_obj.get_mut("hooks").and_then(Value::as_object_mut) {
        for event in MANAGED_EVENTS {
            // Only drop an entry we recognize as ours. A user who wrote
            // their own Stop hook keeps it.
            let ours = hooks.get(event).is_some_and(is_managed_entry);
            if ours {
                hooks.remove(event);
                removed = true;
            }
        }
        if hooks.is_empty() {
            root_obj.remove("hooks");
        }
    }
    if !removed {
        return Ok(());
    }

    if root_obj.is_empty() {
        std::fs::remove_file(&path)?;
    } else {
        std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    }
    Ok(())
}

/// Whether an event's value is a block Orion wrote: every command in it
/// names our helper. Anything else — a user's own hook, or a block we only
/// partly recognize — is left alone.
fn is_managed_entry(value: &Value) -> bool {
    let Some(matchers) = value.as_array() else { return false };
    if matchers.is_empty() {
        return false;
    }
    matchers.iter().all(|m| {
        let Some(hooks) = m.get("hooks").and_then(Value::as_array) else {
            return false;
        };
        !hooks.is_empty()
            && hooks.iter().all(|h| {
                h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(is_hook_helper)
            })
    })
}

/// Matches our helper by file name, so a block written by a daemon that
/// lived somewhere else on disk — an older build, a different target dir —
/// is still recognized as ours and cleaned up.
fn is_hook_helper(command: &str) -> bool {
    Path::new(command)
        .file_stem()
        .is_some_and(|s| s.eq_ignore_ascii_case("orion-hook"))
}

fn hook_entry(command: &str, pane: PaneId, port: u16, token: &str, event: &str) -> Value {
    json!({
        "type": "command",
        "command": command,
        "args": [
            format!("http://127.0.0.1:{port}/hook/{}/{event}", pane.0),
            token,
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
        assert_eq!(entry["type"], "command");
        assert!(
            is_hook_helper(entry["command"].as_str().unwrap()),
            "must run our own helper, not a general-purpose HTTP client: {entry:?}"
        );
        let args: Vec<String> = serde_json::from_value(entry["args"].clone()).unwrap();
        assert_eq!(
            args,
            vec!["http://127.0.0.1:5555/hook/42/Stop".to_string(), "sekrit".to_string()],
            "url carries pane id and event, token follows"
        );
    }

    #[test]
    fn the_hook_command_is_an_absolute_path_next_to_the_daemon() {
        // Nothing installs these binaries on PATH, and the exec form has no
        // shell to resolve a bare name for it.
        let cmd = hook_command();
        assert!(
            Path::new(&cmd).is_absolute(),
            "should resolve beside the running daemon, got {cmd:?}"
        );
        assert!(is_hook_helper(&cmd));
    }

    #[test]
    fn the_helper_is_recognized_however_it_was_spelled() {
        // A block written by an older build living in another target dir
        // must still be recognized as ours so it can be cleaned up.
        assert!(is_hook_helper("orion-hook"));
        assert!(is_hook_helper("orion-hook.exe"));
        assert!(is_hook_helper(r"C:\old\target\debug\orion-hook.exe"));
        assert!(is_hook_helper("/usr/local/bin/orion-hook"));
        assert!(!is_hook_helper("curl.exe"));
        assert!(!is_hook_helper("/bin/sh"));
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

    // --- uninstall ----------------------------------------------------------

    #[test]
    fn uninstall_removes_every_managed_event() {
        let dir = tempfile::tempdir().unwrap();
        install_claude_hooks(dir.path(), PaneId(1), 5555, "tok").unwrap();
        uninstall_claude_hooks(dir.path()).unwrap();
        assert!(
            !settings_path(dir.path()).exists(),
            "a file holding nothing but our hooks should be gone, not left as {{}}"
        );
    }

    #[test]
    fn a_daemon_that_exits_leaves_no_hooks_behind_for_the_next_agent() {
        // Regression: hooks naming a dead daemon's ephemeral port stayed in
        // the checkout forever, so every later agent run in that directory
        // — Orion-managed or not — failed its Stop hook on every turn.
        let dir = tempfile::tempdir().unwrap();
        install_claude_hooks(dir.path(), PaneId(4), 65140, "tok").unwrap();
        uninstall_claude_hooks(dir.path()).unwrap();

        let leftover = std::fs::read_to_string(settings_path(dir.path())).unwrap_or_default();
        assert!(
            !leftover.contains("65140"),
            "the dead daemon's port must not survive: {leftover}"
        );
    }

    #[test]
    fn uninstall_keeps_the_users_own_settings_and_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("settings.local.json"),
            r#"{"permissions":{"allow":["Bash"]},
                "hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
        )
        .unwrap();

        install_claude_hooks(dir.path(), PaneId(1), 5555, "tok").unwrap();
        uninstall_claude_hooks(dir.path()).unwrap();

        let root = read_settings(dir.path());
        assert_eq!(root["permissions"]["allow"][0], "Bash");
        assert!(root["hooks"]["SessionStart"].is_array(), "the user's hook survives");
        for event in MANAGED_EVENTS {
            assert!(root["hooks"].get(event).is_none(), "{event} should be gone");
        }
    }

    #[test]
    fn uninstall_will_not_touch_a_users_hook_on_a_managed_event_name() {
        // The user is entitled to their own Stop hook. Ours is identified by
        // the command it runs, not by the event it sits on.
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("settings.local.json"),
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"my-own-script.sh"}]}]}}"#,
        )
        .unwrap();

        uninstall_claude_hooks(dir.path()).unwrap();

        let root = read_settings(dir.path());
        assert_eq!(
            root["hooks"]["Stop"][0]["hooks"][0]["command"], "my-own-script.sh",
            "someone else's Stop hook must survive"
        );
    }

    #[test]
    fn uninstall_is_idempotent_and_safe_on_a_checkout_that_never_had_an_agent() {
        let dir = tempfile::tempdir().unwrap();
        // No .claude at all — the common case at startup.
        uninstall_claude_hooks(dir.path()).unwrap();
        assert!(!dir.path().join(".claude").exists(), "must not create anything");

        install_claude_hooks(dir.path(), PaneId(1), 5555, "tok").unwrap();
        uninstall_claude_hooks(dir.path()).unwrap();
        uninstall_claude_hooks(dir.path()).unwrap();
    }

    #[test]
    fn uninstall_survives_a_corrupt_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("settings.local.json"), "not json {{{").unwrap();
        uninstall_claude_hooks(dir.path()).unwrap();
    }

    #[test]
    fn install_then_uninstall_round_trips_to_the_original_file() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        let original = serde_json::json!({ "permissions": { "allow": ["Bash"] } });
        std::fs::write(
            claude.join("settings.local.json"),
            serde_json::to_string_pretty(&original).unwrap(),
        )
        .unwrap();

        install_claude_hooks(dir.path(), PaneId(1), 5555, "tok").unwrap();
        uninstall_claude_hooks(dir.path()).unwrap();

        assert_eq!(read_settings(dir.path()), original, "no residue left behind");
    }
}
