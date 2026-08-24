//! What Argus knows about an agent CLI, beyond the command that starts it.
//!
//! Status used to come from Claude Code's hook dialect and nothing else
//! (DESIGN.md §8b, §11), which meant every other harness — herdr, codex,
//! opencode, anything a user writes — sat at `Idle` until it exited. A
//! harness here is a description of *how a particular CLI can be asked to
//! report*, so adding one is a config block rather than a code change.
//!
//! Two mechanisms, and a harness may use either or both:
//!
//! 1. **Environment.** Every agent pane is handed `ARGUS_HOOK_URL`,
//!    `ARGUS_HOOK_TOKEN` and the path to the helper binary, so a harness
//!    that can run *any* command at *any* point in its lifecycle can report
//!    without Argus knowing the shape of its config file. This is the
//!    universal floor: it costs nothing and works for a harness that has no
//!    settings file at all.
//!
//! 2. **A settings file.** For harnesses whose hooks live in JSON in the
//!    checkout, Argus writes a managed block into it at spawn and takes it
//!    out again afterwards. [`Harness::settings`] says where the file is,
//!    [`Shape`] says how an entry is nested, and [`events`] maps that
//!    harness's event names onto the statuses Argus draws.
//!
//! **A settings block is per-boot and must not outlive its daemon.** It
//! names an ephemeral port and a per-boot token, so the moment the daemon
//! that wrote it exits it points at nobody. A checkout is a directory the
//! user also runs agents in by hand, so a stale block doesn't just go quiet
//! — it fires on every turn of every unrelated agent started there
//! afterwards. Hence [`uninstall`], called when the last agent pane in a
//! checkout goes away and again for every configured checkout at daemon
//! startup, and hence a helper command that never reports failure even when
//! nothing answers.

use std::path::{Path, PathBuf};

use argus_protocol::{PaneId, PaneStatus};
use serde::Deserialize;
use serde_json::{json, Value};

/// The statuses a harness can report. Deliberately not [`PaneStatus`]:
/// that has an `Exited` variant, which is the daemon's to decide from the
/// process, never a hook's to claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Report {
    Working,
    Idle,
    Waiting,
    Failed,
}

impl Report {
    pub const ALL: [Report; 4] = [
        Report::Working,
        Report::Idle,
        Report::Waiting,
        Report::Failed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Report::Working => "working",
            Report::Idle => "idle",
            Report::Waiting => "waiting",
            Report::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Report> {
        Report::ALL.into_iter().find(|r| r.as_str() == s)
    }

    pub fn status(self) -> PaneStatus {
        match self {
            Report::Working => PaneStatus::Working,
            Report::Idle => PaneStatus::Idle,
            Report::Waiting => PaneStatus::Waiting,
            Report::Failed => PaneStatus::Failed,
        }
    }
}

/// One of the harness's own lifecycle events, and what it means for the row.
#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    /// The harness's name for it, e.g. Claude Code's `UserPromptSubmit`.
    pub name: String,
    pub reports: Report,
    /// Whether this event hands the hook command a message on stdin worth
    /// showing as the pane's note. Claude Code's `Notification` does: it is
    /// the text saying what it is waiting for.
    #[serde(default)]
    pub note_from_stdin: bool,
}

/// How a hook entry is nested inside the harness's settings file.
///
/// Claude Code groups entries under matcher objects; simpler harnesses
/// take a flat list. Anything stranger than these two is better served by
/// the environment mechanism than by growing a template language here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// `"Event": [{ "hooks": [entry] }]`
    Matcher,
    /// `"Event": [entry]`
    Flat,
}

#[derive(Debug, Clone)]
pub struct Harness {
    pub name: String,
    /// Where the harness reads hooks from, relative to the checkout.
    /// `None` for a harness that only takes the environment.
    pub settings: Option<PathBuf>,
    /// The key under the settings root that holds the event map.
    pub hooks_key: String,
    pub shape: Shape,
    pub events: Vec<Event>,
    /// An event whose command's stdout the harness injects into the model's
    /// context. Where Argus tells an agent it can rename its own pane.
    pub context_event: Option<String>,
}

impl Harness {
    /// The harness for an agent template that names none, and for any
    /// template whose named harness has gone missing from the config. It
    /// installs nothing and touches no file in the checkout; the pane still
    /// gets the environment, so an agent that knows about Argus can still
    /// report.
    pub fn generic() -> Harness {
        Harness {
            name: "generic".to_string(),
            settings: None,
            hooks_key: "hooks".to_string(),
            shape: Shape::Flat,
            events: Vec::new(),
            context_event: None,
        }
    }

    pub fn claude() -> Harness {
        Harness {
            name: "claude".to_string(),
            settings: Some(PathBuf::from(".claude").join("settings.local.json")),
            hooks_key: "hooks".to_string(),
            shape: Shape::Matcher,
            events: vec![
                Event {
                    name: "UserPromptSubmit".into(),
                    reports: Report::Working,
                    note_from_stdin: false,
                },
                Event {
                    name: "Stop".into(),
                    reports: Report::Idle,
                    note_from_stdin: false,
                },
                Event {
                    name: "Notification".into(),
                    reports: Report::Waiting,
                    // Carries the text of what it is asking for.
                    note_from_stdin: true,
                },
            ],
            context_event: Some("SessionStart".to_string()),
        }
    }

    /// Harnesses Argus ships with. A `[[harness]]` block of the same name
    /// in the user's config replaces the built-in entirely.
    pub fn builtins() -> Vec<Harness> {
        vec![Harness::claude(), Harness::generic()]
    }

    fn settings_path(&self, checkout: &Path) -> Option<PathBuf> {
        self.settings.as_ref().map(|rel| checkout.join(rel))
    }

    /// Every event name this harness has Argus write, context event
    /// included — the set [`uninstall`] has to consider.
    fn managed_events(&self) -> impl Iterator<Item = &str> {
        self.events
            .iter()
            .map(|e| e.name.as_str())
            .chain(self.context_event.as_deref())
    }

    /// Writes the managed block into the checkout's settings file.
    ///
    /// A no-op for a harness with no settings file — that is the normal
    /// case, not a failure.
    pub fn install(
        &self,
        checkout: &Path,
        pane: PaneId,
        port: u16,
        token: &str,
    ) -> anyhow::Result<()> {
        let Some(path) = self.settings_path(checkout) else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut root = read_settings(&path);
        let root_obj = root.as_object_mut().expect("just normalized to an object");

        let hooks = root_obj
            .entry(self.hooks_key.clone())
            .or_insert_with(|| json!({}));
        if !hooks.is_object() {
            *hooks = json!({});
        }
        let hooks_obj = hooks.as_object_mut().expect("just normalized to an object");

        let command = helper_path();
        for event in &self.events {
            let entry = status_entry(&command, pane, port, token, event);
            hooks_obj.insert(event.name.clone(), self.shape.wrap(entry));
        }
        if let Some(name) = &self.context_event {
            let entry = say_entry(&command, &instructions());
            hooks_obj.insert(name.clone(), self.shape.wrap(entry));
        }

        std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
        Ok(())
    }

    /// Removes Argus's managed block, leaving anything the user put in the
    /// same file untouched. Cleans up after itself as it goes: an emptied
    /// hooks key is dropped, and a settings file left with nothing in it at
    /// all is deleted rather than left behind as `{}`.
    ///
    /// Idempotent, and a no-op when there's nothing there — it runs at
    /// startup across every configured checkout, most of which never hosted
    /// an agent.
    pub fn uninstall(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(path) = self.settings_path(checkout) else {
            return Ok(());
        };
        if !path.exists() {
            return Ok(());
        }

        let mut root = read_settings(&path);
        let root_obj = root.as_object_mut().expect("just normalized to an object");

        let mut removed = false;
        if let Some(hooks) = root_obj
            .get_mut(&self.hooks_key)
            .and_then(Value::as_object_mut)
        {
            for event in self.managed_events() {
                // Only drop an entry we recognize as ours. A user who wrote
                // their own Stop hook keeps it.
                if hooks.get(event).is_some_and(is_managed_entry) {
                    hooks.remove(event);
                    removed = true;
                }
            }
            if hooks.is_empty() {
                root_obj.remove(&self.hooks_key);
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
}

impl Shape {
    fn wrap(self, entry: Value) -> Value {
        match self {
            Shape::Matcher => json!([{ "hooks": [entry] }]),
            Shape::Flat => json!([entry]),
        }
    }
}

/// Environment handed to every agent pane, whatever its harness.
///
/// The universal floor: a harness with no settings file Argus understands
/// can still report, and an agent that has been told these exist can rename
/// its own pane from inside a turn.
pub fn env(pane: PaneId, port: u16, token: &str) -> Vec<(String, String)> {
    vec![
        (URL_VAR.into(), pane_url(pane, port)),
        (TOKEN_VAR.into(), token.to_string()),
        ("ARGUS_PANE".into(), pane.0.to_string()),
        ("ARGUS_HOOK".into(), helper_path()),
        (INSTRUCTIONS_VAR.into(), instructions()),
    ]
}

pub const URL_VAR: &str = "ARGUS_HOOK_URL";
pub const TOKEN_VAR: &str = "ARGUS_HOOK_TOKEN";
pub const INSTRUCTIONS_VAR: &str = "ARGUS_INSTRUCTIONS";

/// The base every endpoint for this pane hangs off.
fn pane_url(pane: PaneId, port: u16) -> String {
    format!("http://127.0.0.1:{port}/pane/{}", pane.0)
}

/// What an agent is told about Argus, once, at the start of its session.
///
/// Kept to what is actionable. An agent that reads this should come away
/// knowing it can name its own row, which is the whole point: a column of
/// four panes all called "claude" tells you nothing about which one is
/// worth looking at.
pub fn instructions() -> String {
    let hook = helper_path();
    format!(
        "You are running inside Argus, which shows this session as one pane in a list.\n\
         The pane is currently named after the agent, which is not useful when several \
         are running. Rename it to whatever you are actually working on, as a short \
         noun phrase of a few words, by running:\n\
         \n\
         \x20 {hook} title \"fixing the pty deadlock\"\n\
         \n\
         Do that as soon as you know what the task is, and again whenever you move on \
         to something clearly different.\n\
         \n\
         If you get blocked and need the human, say so in one line so they can see \
         why from the pane list without opening it:\n\
         \n\
         \x20 {hook} status waiting \"needs the staging database password\"\n\
         \x20 {hook} status failed \"cargo test is failing on a dependency I can't fix\"\n\
         \n\
         Report `working` again once you are unblocked. These write nothing and cost \
         nothing. Do not mention having run them."
    )
}

/// The helper that actually posts to the daemon (`src/bin/argus-hook.rs`),
/// resolved next to the running daemon rather than trusted to `PATH` —
/// nothing installs these binaries system-wide. Falls back to the bare name
/// if the daemon's own path can't be read.
pub fn helper_path() -> String {
    let exe = if cfg!(windows) { "argus-hook.exe" } else { "argus-hook" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe)))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| exe.to_string())
}

fn status_entry(command: &str, pane: PaneId, port: u16, token: &str, event: &Event) -> Value {
    let mut args = vec![
        format!("{}/status/{}", pane_url(pane, port), event.reports.as_str()),
        token.to_string(),
    ];
    if event.note_from_stdin {
        args.push(NOTE_FLAG.to_string());
    }
    json!({
        "type": "command",
        "command": command,
        "args": args,
        "timeout": 5
    })
}

/// Tells the helper to read the harness's message off stdin and send it as
/// the pane's note. Only passed on events that actually supply one — the
/// helper must never block on a stdin nobody is writing to.
pub const NOTE_FLAG: &str = "--note-from-stdin";

/// A hook that only prints. `say` needs no daemon and no network, so the
/// context an agent starts with doesn't depend on the status port being up.
fn say_entry(command: &str, text: &str) -> Value {
    json!({
        "type": "command",
        "command": command,
        "args": ["say", text],
        "timeout": 5
    })
}

/// Parses a settings file into an object, normalizing anything unexpected
/// (missing, corrupt, or not a JSON object) to `{}` — this file is the
/// user's, so a broken one must not stop an agent from spawning.
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

/// Whether an event's value is a block Argus wrote: every command in it
/// names our helper. Anything else — a user's own hook, or a block we only
/// partly recognize — is left alone. Accepts either [`Shape`], so changing
/// a harness's shape between runs still leaves its old block removable.
fn is_managed_entry(value: &Value) -> bool {
    let Some(items) = value.as_array() else {
        return false;
    };
    if items.is_empty() {
        return false;
    }
    items.iter().all(|item| match item.get("hooks") {
        Some(Value::Array(inner)) => !inner.is_empty() && inner.iter().all(names_helper),
        _ => names_helper(item),
    })
}

fn names_helper(entry: &Value) -> bool {
    entry
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(is_hook_helper)
}

/// Matches our helper by file name, so a block written by a daemon that
/// lived somewhere else on disk — an older build, a different target dir —
/// is still recognized as ours and cleaned up.
fn is_hook_helper(command: &str) -> bool {
    Path::new(command).file_stem().is_some_and(|s| {
        // `orion-hook` is the pre-rename name. A block naming it is still
        // ours, and still fires on every turn until something removes it.
        s.eq_ignore_ascii_case("argus-hook") || s.eq_ignore_ascii_case("orion-hook")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_of(dir: &Path, h: &Harness) -> Value {
        let raw = std::fs::read_to_string(h.settings_path(dir).unwrap()).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    fn flat_harness() -> Harness {
        Harness {
            name: "herdr".to_string(),
            settings: Some(PathBuf::from("herdr.json")),
            hooks_key: "on".to_string(),
            shape: Shape::Flat,
            events: vec![
                Event {
                    name: "turn_start".into(),
                    reports: Report::Working,
                    note_from_stdin: false,
                },
                Event {
                    name: "turn_end".into(),
                    reports: Report::Idle,
                    note_from_stdin: false,
                },
            ],
            context_event: None,
        }
    }

    #[test]
    fn a_harness_with_no_settings_file_touches_nothing() {
        // The default for an unknown CLI. It still gets the environment;
        // what it must not do is scribble in the user's checkout.
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::generic();
        h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
        h.uninstall(dir.path()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_flat_harness_installs_where_its_config_says() {
        // The point of the feature: a harness Argus has never heard of is
        // described, not coded.
        let dir = tempfile::tempdir().unwrap();
        let h = flat_harness();
        h.install(dir.path(), PaneId(9), 4242, "tok").unwrap();

        let root = settings_of(dir.path(), &h);
        let entry = &root["on"]["turn_start"][0];
        assert!(is_hook_helper(entry["command"].as_str().unwrap()));
        let args: Vec<String> = serde_json::from_value(entry["args"].clone()).unwrap();
        assert_eq!(args[0], "http://127.0.0.1:4242/pane/9/status/working");
    }

    #[test]
    fn each_event_reports_the_status_its_harness_assigned_it() {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::claude();
        h.install(dir.path(), PaneId(3), 5555, "tok").unwrap();

        let hooks = settings_of(dir.path(), &h)["hooks"].clone();
        for event in &h.events {
            let args: Vec<String> =
                serde_json::from_value(hooks[&event.name][0]["hooks"][0]["args"].clone()).unwrap();
            assert!(
                args[0].ends_with(&format!("/status/{}", event.reports.as_str())),
                "{} should report {:?}, got {}",
                event.name,
                event.reports,
                args[0]
            );
        }
    }

    #[test]
    fn the_url_carries_the_pane_and_the_token_follows() {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::claude();
        h.install(dir.path(), PaneId(42), 5555, "sekrit").unwrap();

        let entry = settings_of(dir.path(), &h)["hooks"]["Stop"][0]["hooks"][0].clone();
        assert_eq!(entry["type"], "command");
        assert!(
            is_hook_helper(entry["command"].as_str().unwrap()),
            "must run our own helper, not a general-purpose HTTP client: {entry:?}"
        );
        let args: Vec<String> = serde_json::from_value(entry["args"].clone()).unwrap();
        assert_eq!(
            args,
            vec![
                "http://127.0.0.1:5555/pane/42/status/idle".to_string(),
                "sekrit".to_string()
            ]
        );
    }

    #[test]
    fn the_context_hook_carries_the_instructions_and_calls_nothing() {
        // An agent's starting context must not depend on the status port
        // being up, so this hook only prints.
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::claude();
        h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();

        let entry = settings_of(dir.path(), &h)["hooks"]["SessionStart"][0]["hooks"][0].clone();
        let args: Vec<String> = serde_json::from_value(entry["args"].clone()).unwrap();
        assert_eq!(args[0], "say");
        assert!(args[1].contains("title"), "should teach renaming: {}", args[1]);
        assert!(
            !args[1].contains("http://"),
            "no network in the instruction hook"
        );
    }

    #[test]
    fn the_helper_is_an_absolute_path_next_to_the_daemon() {
        // Nothing installs these binaries on PATH, and the exec form has no
        // shell to resolve a bare name for it.
        let cmd = helper_path();
        assert!(Path::new(&cmd).is_absolute(), "got {cmd:?}");
        assert!(is_hook_helper(&cmd));
    }

    #[test]
    fn the_helper_is_recognized_however_it_was_spelled() {
        assert!(is_hook_helper("argus-hook"));
        assert!(is_hook_helper("argus-hook.exe"));
        assert!(is_hook_helper(r"C:\old\target\debug\argus-hook.exe"));
        assert!(is_hook_helper("/usr/local/bin/argus-hook"));
        assert!(
            is_hook_helper("orion-hook.exe"),
            "a block from before the rename is still ours to clean up"
        );
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
            r#"{"permissions":{"allow":["Bash"]},"hooks":{"PreToolUse":[{"hooks":[]}]}}"#,
        )
        .unwrap();

        let h = Harness::claude();
        h.install(dir.path(), PaneId(1), 1234, "tok").unwrap();

        let root = settings_of(dir.path(), &h);
        assert_eq!(root["permissions"]["allow"][0], "Bash");
        assert!(root["hooks"]["PreToolUse"].is_array());
        assert!(root["hooks"]["Stop"].is_array());
    }

    #[test]
    fn reinstalling_replaces_rather_than_appends() {
        // Every agent spawn rewrites these; a stale entry pointing at a dead
        // port/pane must not accumulate across spawns.
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::claude();
        h.install(dir.path(), PaneId(1), 1111, "a").unwrap();
        h.install(dir.path(), PaneId(2), 2222, "b").unwrap();

        let stop = settings_of(dir.path(), &h)["hooks"]["Stop"].clone();
        assert_eq!(stop.as_array().unwrap().len(), 1, "no duplicate matchers");
        let args: Vec<String> = serde_json::from_value(stop[0]["hooks"][0]["args"].clone()).unwrap();
        assert!(args[0].contains("/pane/2/"));
        assert!(args[0].contains("2222"));
    }

    #[test]
    fn recovers_from_a_corrupt_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("settings.local.json"), "not json at all {{{").unwrap();

        let h = Harness::claude();
        h.install(dir.path(), PaneId(1), 1234, "tok").unwrap();
        assert!(settings_of(dir.path(), &h)["hooks"]["Stop"].is_array());
    }

    #[test]
    fn normalizes_a_non_object_hooks_key() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("settings.local.json"), r#"{"hooks":[]}"#).unwrap();

        let h = Harness::claude();
        h.install(dir.path(), PaneId(1), 1234, "tok").unwrap();
        assert!(settings_of(dir.path(), &h)["hooks"].is_object());
    }

    #[test]
    fn creates_the_settings_directory_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join(".claude").exists());
        Harness::claude()
            .install(dir.path(), PaneId(1), 1234, "tok")
            .unwrap();
        assert!(dir.path().join(".claude").join("settings.local.json").is_file());
    }

    // --- uninstall ----------------------------------------------------------

    #[test]
    fn uninstall_removes_every_managed_event_including_the_context_one() {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::claude();
        h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
        h.uninstall(dir.path()).unwrap();
        assert!(
            !h.settings_path(dir.path()).unwrap().exists(),
            "a file holding nothing but our hooks should be gone, not left as {{}}"
        );
    }

    #[test]
    fn a_daemon_that_exits_leaves_no_hooks_behind_for_the_next_agent() {
        // Regression: hooks naming a dead daemon's ephemeral port stayed in
        // the checkout forever, so every later agent run in that directory
        // — Argus-managed or not — failed its Stop hook on every turn.
        for h in [Harness::claude(), flat_harness()] {
            let dir = tempfile::tempdir().unwrap();
            h.install(dir.path(), PaneId(4), 65140, "tok").unwrap();
            h.uninstall(dir.path()).unwrap();

            let leftover =
                std::fs::read_to_string(h.settings_path(dir.path()).unwrap()).unwrap_or_default();
            assert!(
                !leftover.contains("65140"),
                "{}: the dead daemon's port must not survive: {leftover}",
                h.name
            );
        }
    }

    #[test]
    fn uninstall_keeps_the_users_own_settings_and_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join("settings.local.json"),
            r#"{"permissions":{"allow":["Bash"]},
                "hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
        )
        .unwrap();

        let h = Harness::claude();
        h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
        h.uninstall(dir.path()).unwrap();

        let root = settings_of(dir.path(), &h);
        assert_eq!(root["permissions"]["allow"][0], "Bash");
        assert!(root["hooks"]["PreToolUse"].is_array(), "the user's hook survives");
        for event in h.managed_events() {
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

        Harness::claude().uninstall(dir.path()).unwrap();

        let root = settings_of(dir.path(), &Harness::claude());
        assert_eq!(
            root["hooks"]["Stop"][0]["hooks"][0]["command"], "my-own-script.sh",
            "someone else's Stop hook must survive"
        );
    }

    #[test]
    fn uninstall_is_idempotent_and_safe_on_a_checkout_that_never_had_an_agent() {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::claude();
        h.uninstall(dir.path()).unwrap();
        assert!(!dir.path().join(".claude").exists(), "must not create anything");

        h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
        h.uninstall(dir.path()).unwrap();
        h.uninstall(dir.path()).unwrap();
    }

    #[test]
    fn uninstall_survives_a_corrupt_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("settings.local.json"), "not json {{{").unwrap();
        Harness::claude().uninstall(dir.path()).unwrap();
    }

    #[test]
    fn install_then_uninstall_round_trips_to_the_original_file() {
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        let original = json!({ "permissions": { "allow": ["Bash"] } });
        std::fs::write(
            claude.join("settings.local.json"),
            serde_json::to_string_pretty(&original).unwrap(),
        )
        .unwrap();

        let h = Harness::claude();
        h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
        h.uninstall(dir.path()).unwrap();

        assert_eq!(settings_of(dir.path(), &h), original, "no residue left behind");
    }

    #[test]
    fn a_block_written_under_the_other_shape_is_still_removable() {
        // A harness whose shape changed in config between runs would
        // otherwise leave a live block pointing at a dead port.
        let dir = tempfile::tempdir().unwrap();
        let mut h = flat_harness();
        h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();
        h.shape = Shape::Matcher;
        h.uninstall(dir.path()).unwrap();
        assert!(!h.settings_path(dir.path()).unwrap().exists());
    }

    // --- environment --------------------------------------------------------

    #[test]
    fn every_agent_is_handed_a_url_and_token_whatever_its_harness() {
        // The universal floor: reporting must not require Argus to
        // understand a harness's config file.
        let env = env(PaneId(7), 4242, "tok");
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get(URL_VAR).unwrap(), "http://127.0.0.1:4242/pane/7");
        assert_eq!(get(TOKEN_VAR).unwrap(), "tok");
        assert_eq!(get("ARGUS_PANE").unwrap(), "7");
        assert!(is_hook_helper(&get("ARGUS_HOOK").unwrap()));
    }

    #[test]
    fn the_environment_also_carries_the_instructions() {
        // For a harness with no context event of its own, this is the only
        // way an agent learns it can rename its pane.
        let env = env(PaneId(1), 1, "t");
        let text = env
            .iter()
            .find(|(n, _)| n == INSTRUCTIONS_VAR)
            .map(|(_, v)| v.clone())
            .unwrap();
        assert!(text.contains("title"));
    }

    #[test]
    fn a_report_round_trips_through_its_wire_name() {
        for r in Report::ALL {
            assert_eq!(Report::parse(r.as_str()), Some(r));
        }
        assert_eq!(Report::parse("exited"), None, "only the daemon decides that");
        assert_eq!(Report::parse(""), None);
    }
}
