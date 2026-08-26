//! What Argus knows about an agent CLI, beyond the command that starts it.
//!
//! Status used to come from Claude Code's hook dialect and nothing else
//! (DESIGN.md §8b, §11), which meant every other harness — herdr, codex,
//! opencode, anything a user writes — sat at `Idle` until it exited. A
//! harness here is a description of *how a particular CLI can be asked to
//! report*, so adding one is a config block rather than a code change.
//!
//! Three mechanisms, and a harness may use any combination of them:
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
//! 3. **A plugin file.** Some harnesses have no hook table at all: opencode
//!    extends through a JavaScript module, so there is no JSON to write and
//!    nothing a `[[harness]]` block could describe. [`Plugin`] is a file
//!    Argus drops into the checkout and takes out again on the same
//!    schedule. It reads the environment from mechanism 1 rather than
//!    baking a pane in, so one file serves every pane in the checkout.
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
    #[serde(rename = "needs-review")]
    NeedsReview,
    Done,
    Failed,
}

impl Report {
    pub const ALL: [Report; 6] = [
        Report::Working,
        Report::Idle,
        Report::Waiting,
        Report::NeedsReview,
        Report::Done,
        Report::Failed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Report::Working => "working",
            Report::Idle => "idle",
            Report::Waiting => "waiting",
            Report::NeedsReview => "needs-review",
            Report::Done => "done",
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
            Report::NeedsReview => PaneStatus::NeedsReview,
            Report::Done => PaneStatus::Done,
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
    /// Optional event-specific matcher, used by matcher-shaped harnesses.
    #[serde(default)]
    pub matcher: Option<String>,
    /// Whether this event hands the hook command a message on stdin worth
    /// showing as the pane's note. Claude Code's `Notification` does: it is
    /// the text saying what it is waiting for.
    #[serde(default)]
    pub note_from_stdin: bool,
    /// Optional top-level JSON key whose string value identifies the session.
    /// Every event that has one tags its report with it, which is how a
    /// report from an agent spawned inside the pane is told apart from the
    /// pane's own.
    #[serde(default)]
    pub session_id_key: Option<String>,
    /// Whether this event is the harness announcing that *its own* session
    /// started, and so may claim the identity Argus resumes the pane with.
    #[serde(default)]
    pub owns_session: bool,
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

/// A file Argus writes into the checkout whole, for a harness whose
/// extension point is a program rather than a table of commands.
///
/// The source is shipped rather than configured: a hook dialect fits in
/// TOML, and a program does not. A `[[harness]]` block that replaces a
/// built-in by name therefore also gives up its plugin — which is what
/// "replaces the built-in entirely" already meant.
#[derive(Debug, Clone)]
pub struct Plugin {
    /// Where it goes, relative to the checkout.
    pub path: PathBuf,
    pub source: &'static str,
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
    /// A module to drop into the checkout, for a harness that extends
    /// through code rather than through JSON.
    pub plugin: Option<Plugin>,
    /// What turns this CLI's start command into "pick up where we left
    /// off", appended to the template's `cmd` — `["--continue"]` and the
    /// like. Only ever used when Argus restarts a pane it recorded, never
    /// when the user asks for an agent: a new pane is a new conversation.
    ///
    /// Empty means Argus has no way to ask this CLI to resume, and the
    /// pane comes back the old way: running, with nothing behind it.
    pub resume: Vec<String>,
    /// Exact resume argv template. `{session_id}` is expanded without a shell.
    pub resume_id: Vec<String>,
    /// Command hooks accept one shell command string, unlike Claude's
    /// `command` plus `args` shape. These use the pane environment rather
    /// than baked-in routing so content-hash trust survives reinstalls.
    pub command_string: bool,
    /// Optional workspace rule markdown file to install into checkout.
    pub rule_file: Option<PathBuf>,
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
            plugin: None,
            resume: Vec::new(),
            resume_id: Vec::new(),
            command_string: false,
            rule_file: None,
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
                    matcher: None,
                    note_from_stdin: false,
                    session_id_key: Some("session_id".into()),
                    owns_session: false,
                },
                Event {
                    name: "Stop".into(),
                    reports: Report::Idle,
                    matcher: None,
                    note_from_stdin: false,
                    session_id_key: Some("session_id".into()),
                    owns_session: false,
                },
                Event {
                    name: "Notification".into(),
                    reports: Report::Waiting,
                    matcher: None,
                    // Carries the text of what it is asking for.
                    note_from_stdin: true,
                    session_id_key: Some("session_id".into()),
                    owns_session: false,
                },
                Event {
                    name: "SessionStart".into(),
                    reports: Report::Idle,
                    // Compaction starts a fresh context while the same turn
                    // is still running, so it must not make the pane idle.
                    matcher: Some("startup|resume|clear|fork".into()),
                    note_from_stdin: false,
                    session_id_key: Some("session_id".into()),
                    owns_session: true,
                },
            ],
            context_event: Some("SessionStart".to_string()),
            plugin: None,
            // Picks up the most recent conversation in the checkout, which
            // is the one the pane had: Argus starts each agent in its own
            // checkout's directory.
            resume: vec!["--continue".to_string()],
            resume_id: vec!["--resume".to_string(), "{session_id}".to_string()],
            command_string: false,
            rule_file: None,
        }
    }

    /// Codex discovers project hooks in `.codex/hooks.json`. Its command
    /// handler is a string rather than Claude's command-plus-args object.
    pub fn codex() -> Harness {
        Harness {
            name: "codex".to_string(),
            settings: Some(PathBuf::from(".codex").join("hooks.json")),
            hooks_key: "hooks".to_string(),
            shape: Shape::Matcher,
            events: vec![Event {
                name: "SessionStart".into(),
                reports: Report::Idle,
                matcher: Some("startup|resume|clear".into()),
                note_from_stdin: false,
                session_id_key: Some("session_id".into()),
                owns_session: true,
            }],
            context_event: None,
            plugin: None,
            resume: vec!["resume".to_string(), "--last".to_string()],
            resume_id: vec!["resume".to_string(), "{session_id}".to_string()],
            command_string: true,
            rule_file: None,
        }
    }

    /// opencode reports through a plugin module rather than a hook table:
    /// it has no JSON hooks at all, so mechanism 2 has nothing to write and
    /// a template named `opencode` used to fall all the way through to
    /// [`Harness::generic`] and sit at `Idle` for its whole life.
    ///
    /// The module carries the whole dialect — which of opencode's events
    /// mean what — because that mapping lives in JavaScript here rather
    /// than in [`events`]. `.opencode/plugin/` is one of the two directories
    /// opencode scans in a project.
    pub fn opencode() -> Harness {
        Harness {
            name: "opencode".to_string(),
            settings: None,
            hooks_key: "hooks".to_string(),
            shape: Shape::Flat,
            events: Vec::new(),
            context_event: None,
            plugin: Some(Plugin {
                path: PathBuf::from(".opencode").join("plugin").join(PLUGIN_FILE),
                source: include_str!("opencode-plugin.js"),
            }),
            resume: vec!["--continue".to_string()],
            resume_id: vec!["--session".to_string(), "{session_id}".to_string()],
            command_string: false,
            rule_file: None,
        }
    }

    /// Google Antigravity (AGY) discovers workspace hooks in `.agents/hooks.json`
    /// under the named hook object and rules in `.agents/rules/`. PreInvocation
    /// marks the pane working, supplies conversationId, and injects instructions;
    /// Stop marks the pane idle.
    pub fn agy() -> Harness {
        Harness {
            name: "agy".to_string(),
            settings: Some(PathBuf::from(".agents").join("hooks.json")),
            hooks_key: "argus".to_string(),
            shape: Shape::Flat,
            events: vec![
                Event {
                    name: "PreInvocation".into(),
                    reports: Report::Working,
                    matcher: None,
                    note_from_stdin: false,
                    session_id_key: Some("conversationId".into()),
                    owns_session: true,
                },
                Event {
                    name: "Stop".into(),
                    reports: Report::Idle,
                    matcher: None,
                    note_from_stdin: false,
                    session_id_key: Some("conversationId".into()),
                    owns_session: false,
                },
            ],
            context_event: None,
            plugin: None,
            resume: vec!["--continue".to_string()],
            resume_id: vec!["--conversation".to_string(), "{session_id}".to_string()],
            command_string: false,
            rule_file: Some(PathBuf::from(".agents").join("rules").join("argus.md")),
        }
    }

    /// Harnesses Argus ships with. A `[[harness]]` block of the same name
    /// in the user's config replaces the built-in entirely.
    pub fn builtins() -> Vec<Harness> {
        vec![
            Harness::claude(),
            Harness::codex(),
            Harness::opencode(),
            Harness::agy(),
            Harness::generic(),
        ]
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

    /// Puts whatever this harness needs into the checkout: a managed block
    /// in its settings file, its plugin module, its rules file, or neither.
    ///
    /// A harness that needs nothing is the normal case, not a failure.
    pub fn install(
        &self,
        checkout: &Path,
        pane: PaneId,
        port: u16,
        token: &str,
    ) -> anyhow::Result<()> {
        let settings = self.install_settings(checkout, pane, port, token);
        let plugin = self.install_plugin(checkout);
        let rule = self.install_rule(checkout);
        settings.and(plugin).and(rule)
    }

    fn install_rule(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(rule_path) = &self.rule_file else {
            return Ok(());
        };
        let path = checkout.join(rule_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = format!(
            "---\ndescription: Argus pair-programming environment integration\nalways_on: true\n---\n\n{}",
            instructions()
        );
        std::fs::write(&path, content)?;
        Ok(())
    }

    fn install_plugin(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(plugin) = &self.plugin else {
            return Ok(());
        };
        let path = checkout.join(&plugin.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, plugin.source)?;
        Ok(())
    }

    fn install_settings(
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
            let entry = status_entry(&command, pane, port, token, event, self.command_string);
            match hooks_obj.get_mut(&event.name) {
                Some(existing) => {
                    remove_managed(existing);
                    self.shape.append(existing, entry, event.matcher.as_deref());
                }
                None => {
                    hooks_obj.insert(
                        event.name.clone(),
                        self.shape.wrap(entry, event.matcher.as_deref()),
                    );
                }
            }
        }
        if let Some(name) = &self.context_event {
            let entry = say_entry(&command, &instructions());
            match hooks_obj.get_mut(name) {
                Some(existing) => {
                    if !self.events.iter().any(|event| &event.name == name) {
                        remove_managed(existing);
                    }
                    self.shape.append(existing, entry, None)
                }
                None => {
                    hooks_obj.insert(name.clone(), self.shape.wrap(entry, None));
                }
            }
        }

        std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
        Ok(())
    }

    /// Removes everything [`install`] put in the checkout, leaving anything
    /// the user put there untouched.
    pub fn uninstall(&self, checkout: &Path) -> anyhow::Result<()> {
        let settings = self.uninstall_settings(checkout);
        let plugin = self.uninstall_plugin(checkout);
        let rule = self.uninstall_rule(checkout);
        settings.and(plugin).and(rule)
    }

    fn uninstall_rule(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(rule_path) = &self.rule_file else {
            return Ok(());
        };
        let path = checkout.join(rule_path);
        if path.exists() {
            let _ = std::fs::remove_file(&path);
            prune_empty_dirs(checkout, &path);
        }
        Ok(())
    }

    /// Deletes the plugin module, and any directory Argus made only to hold
    /// it. Only ever removes a file that still carries our marker: a user
    /// who replaced it with one of their own keeps it.
    fn uninstall_plugin(&self, checkout: &Path) -> anyhow::Result<()> {
        let Some(plugin) = &self.plugin else {
            return Ok(());
        };
        let path = checkout.join(&plugin.path);
        match std::fs::read_to_string(&path) {
            Ok(body) if body.contains(PLUGIN_MARKER) => std::fs::remove_file(&path)?,
            // Missing, unreadable, or someone else's. Nothing to do either
            // way, and none of it is worth failing startup over.
            _ => return Ok(()),
        }
        prune_empty_dirs(checkout, &path);
        Ok(())
    }

    /// Removes Argus's managed hook block, leaving anything the user put in
    /// the same file untouched. Cleans up after itself as it goes: an
    /// emptied hooks key is dropped, and a settings file left with nothing
    /// in it at all is deleted rather than left behind as `{}`.
    fn uninstall_settings(&self, checkout: &Path) -> anyhow::Result<()> {
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
                if let Some(value) = hooks.get_mut(event) {
                    removed |= remove_managed(value);
                    if value.as_array().is_some_and(Vec::is_empty) {
                        hooks.remove(event);
                    }
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
            prune_empty_dirs(checkout, &path);
        } else {
            std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
        }
        Ok(())
    }
}

/// Removes the directories a just-deleted file was the last thing in,
/// walking up until something stops it or the checkout itself is reached.
///
/// `remove_dir` refuses a directory with anything in it, which is the whole
/// safety argument: a `.claude/` the user keeps their own settings in, or an
/// `.opencode/` holding their agents, fails on the first step and stays.
fn prune_empty_dirs(checkout: &Path, file: &Path) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == checkout || std::fs::remove_dir(d).is_err() {
            return;
        }
        dir = d.parent();
    }
}

impl Shape {
    fn wrap(self, entry: Value, matcher: Option<&str>) -> Value {
        match self {
            Shape::Matcher => {
                let mut group = json!({ "hooks": [entry] });
                if let Some(matcher) = matcher {
                    group["matcher"] = Value::String(matcher.to_string());
                }
                json!([group])
            }
            Shape::Flat => json!([entry]),
        }
    }

    fn append(self, existing: &mut Value, entry: Value, matcher: Option<&str>) {
        let Value::Array(mut addition) = self.wrap(entry, matcher) else {
            unreachable!()
        };
        if let Some(items) = existing.as_array_mut() {
            items.append(&mut addition);
        } else {
            *existing = Value::Array(addition);
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
         Other agents may be running in the same checkout. Never run `git switch` or \
         `git checkout` in the checkout you were started in, because that changes the \
         branch for every agent sharing it. If you need another branch, create a new \
         linked worktree and branch there with `git worktree add <new-path> -b \
         <new-branch>`. Do all subsequent work from that new path.\n\
         \n\
         After you start working in another checkout, run this from that checkout so the \
         pane moves under it in Argus:\n\
         \n\
         \x20 {hook} checkout\n\
         \n\
         If you get blocked and need the human, say so in one line so they can see \
         why from the pane list without opening it:\n\
         \n\
         \x20 {hook} status waiting \"needs the staging database password\"\n\
         \x20 {hook} status failed \"cargo test is failing on a dependency I can't fix\"\n\
         \n\
         When your changes are ready for the human to inspect, report `needs-review`. \
         After they are reviewed and the task is complete, report `done`:\n\
         \n\
         \x20 {hook} status needs-review \"ready for review\"\n\
         \x20 {hook} status done \"reviewed and complete\"\n\
         \n\
         Report `working` again when you resume work. These write nothing and cost \
         nothing. Do not mention having run them."
    )
}

/// The helper that actually posts to the daemon (`src/bin/argus-hook.rs`),
/// resolved next to the running daemon rather than trusted to `PATH` —
/// nothing installs these binaries system-wide. Falls back to the bare name
/// if the daemon's own path can't be read.
pub fn helper_path() -> String {
    let exe = if cfg!(windows) {
        "argus-hook.exe"
    } else {
        "argus-hook"
    };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe)))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| exe.to_string())
}

fn status_entry(
    command: &str,
    pane: PaneId,
    port: u16,
    token: &str,
    event: &Event,
    command_string: bool,
) -> Value {
    let mut args = vec![
        format!("{}/status/{}", pane_url(pane, port), event.reports.as_str()),
        token.to_string(),
    ];
    if event.note_from_stdin {
        args.push(NOTE_FLAG.to_string());
    }
    if let Some(key) = &event.session_id_key {
        args.push(SESSION_KEY_FLAG.to_string());
        args.push(key.clone());
    }
    if event.owns_session {
        args.push(OWNS_SESSION_FLAG.to_string());
    }
    if command_string {
        return json!({
            "type": "command",
            "command": env_command_line(event, false),
            "commandWindows": env_command_line(event, true),
            "timeout": 5
        });
    }
    json!({
        "type": "command",
        "command": command,
        "args": args,
        "timeout": 5
    })
}

/// What a harness's plugin module is called in the checkout. Prefixed so
/// it sorts and reads as ours in a directory the user also keeps their own
/// plugins in.
const PLUGIN_FILE: &str = "argus-status.js";

/// The string that identifies a module as one Argus wrote, on its first
/// line. Only a file still carrying it is ours to delete, so a user who
/// replaces it with their own keeps it through an uninstall.
const PLUGIN_MARKER: &str = "argus:managed-plugin";

/// Tells the helper to read the harness's message off stdin and send it as
/// the pane's note. Only passed on events that actually supply one — the
/// helper must never block on a stdin nobody is writing to.
pub const NOTE_FLAG: &str = "--note-from-stdin";
pub const SESSION_KEY_FLAG: &str = "--session-id-from-stdin";
/// Marks the one event per harness that may claim the pane's resume
/// identity. Without it a CLI started from inside a pane would overwrite
/// the conversation Argus reopens for that row.
pub const OWNS_SESSION_FLAG: &str = "--owns-session";

/// A stable command-string hook. Codex persists trust against the handler's
/// content hash, so the checkout-wide file must not contain ephemeral pane,
/// port, token, or executable-path values. Every spawned pane receives these
/// variables, and the helper's installed form still extracts hook stdin.
fn env_command_line(event: &Event, windows: bool) -> String {
    let (helper, url, token) = if windows {
        (
            "%ARGUS_HOOK%".to_string(),
            format!("%ARGUS_HOOK_URL%/status/{}", event.reports.as_str()),
            "%ARGUS_HOOK_TOKEN%".to_string(),
        )
    } else {
        (
            "$ARGUS_HOOK".to_string(),
            format!("$ARGUS_HOOK_URL/status/{}", event.reports.as_str()),
            "$ARGUS_HOOK_TOKEN".to_string(),
        )
    };
    let mut parts = vec![helper, url, token];
    if event.note_from_stdin {
        parts.push(NOTE_FLAG.to_string());
    }
    if let Some(key) = &event.session_id_key {
        parts.push(SESSION_KEY_FLAG.to_string());
        parts.push(key.clone());
    }
    if event.owns_session {
        parts.push(OWNS_SESSION_FLAG.to_string());
    }
    parts
        .into_iter()
        .map(|part| format!("\"{}\"", part.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

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

fn is_managed_item(item: &Value) -> bool {
    match item.get("hooks") {
        Some(Value::Array(inner)) => !inner.is_empty() && inner.iter().all(names_helper),
        _ => names_helper(item),
    }
}

fn remove_managed(value: &mut Value) -> bool {
    let Some(items) = value.as_array_mut() else {
        return false;
    };
    let before = items.len();
    items.retain(|item| !is_managed_item(item));
    items.len() != before
}

fn names_helper(entry: &Value) -> bool {
    entry
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| {
            is_hook_helper(command)
                || command.contains("argus-hook")
                || command.contains("orion-hook")
                || command.contains("ARGUS_HOOK")
        })
}

/// Matches our helper by file name, so a block written by a daemon that
/// lived somewhere else on disk — an older build, a different target dir —
/// is still recognized as ours and cleaned up.
fn is_hook_helper(command: &str) -> bool {
    let stem = crate::editor::program_stem(command);
    // `orion-hook` is the pre-rename name. A block naming it is still ours,
    // and still fires on every turn until something removes it.
    stem == "argus-hook" || stem == "orion-hook"
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
                    matcher: None,
                    note_from_stdin: false,
                    session_id_key: None,
                    owns_session: false,
                },
                Event {
                    name: "turn_end".into(),
                    reports: Report::Idle,
                    matcher: None,
                    note_from_stdin: false,
                    session_id_key: None,
                    owns_session: false,
                },
            ],
            context_event: None,
            plugin: None,
            resume: Vec::new(),
            resume_id: Vec::new(),
            command_string: false,
            rule_file: None,
        }
    }

    #[test]
    fn the_built_in_harnesses_know_how_to_be_continued() {
        // What restore appends to each template's command. Wrong flags here
        // mean an agent that comes back with nothing behind it, or one that
        // refuses to start at all.
        assert_eq!(Harness::claude().resume, ["--continue"]);
        assert_eq!(Harness::opencode().resume, ["--continue"]);
        assert_eq!(Harness::agy().resume, ["--continue"]);
        assert_eq!(
            Harness::codex().resume,
            ["resume", "--last"],
            "the last session, not the picker a bare `codex resume` opens"
        );
        assert!(
            Harness::generic().resume.is_empty(),
            "a CLI Argus knows nothing about is asked for nothing"
        );
        assert_eq!(Harness::claude().resume_id, ["--resume", "{session_id}"]);
        assert_eq!(Harness::codex().resume_id, ["resume", "{session_id}"]);
        assert_eq!(Harness::opencode().resume_id, ["--session", "{session_id}"]);
        assert_eq!(Harness::agy().resume_id, ["--conversation", "{session_id}"]);
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
                "sekrit".to_string(),
                // Every report says which conversation it came from, so a
                // CLI spawned inside the pane is not mistaken for its own.
                "--session-id-from-stdin".to_string(),
                "session_id".to_string()
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

        let starts = settings_of(dir.path(), &h)["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .clone();
        let entry = starts
            .iter()
            .find_map(|matcher| {
                matcher["hooks"]
                    .as_array()?
                    .iter()
                    .find(|hook| hook["args"][0] == "say")
            })
            .unwrap()
            .clone();
        let args: Vec<String> = serde_json::from_value(entry["args"].clone()).unwrap();
        assert_eq!(args[0], "say");
        assert!(
            args[1].contains("title"),
            "should teach renaming: {}",
            args[1]
        );
        assert!(
            args[1].contains("checkout"),
            "should teach checkout affiliation: {}",
            args[1]
        );
        assert!(
            args[1].contains("needs-review"),
            "should teach review state"
        );
        assert!(
            args[1].contains("status done"),
            "should teach completion state"
        );
        assert!(
            !args[1].contains("http://"),
            "no network in the instruction hook"
        );
    }

    #[test]
    fn a_new_claude_conversation_clears_stale_status_without_idling_on_compaction() {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::claude();
        h.install(dir.path(), PaneId(1), 5555, "tok").unwrap();

        let starts = settings_of(dir.path(), &h)["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .clone();
        let status = starts
            .iter()
            .find(|matcher| {
                matcher["hooks"][0]["args"][0]
                    .as_str()
                    .is_some_and(|arg| arg.ends_with("/status/idle"))
            })
            .expect("SessionStart should clear the previous conversation's status");
        assert_eq!(status["matcher"], "startup|resume|clear|fork");
        assert_eq!(
            status["hooks"][0]["args"],
            json!([
                "http://127.0.0.1:5555/pane/1/status/idle",
                "tok",
                SESSION_KEY_FLAG,
                "session_id",
                OWNS_SESSION_FLAG
            ])
        );
        let context = starts
            .iter()
            .find(|matcher| matcher["hooks"][0]["args"][0] == "say")
            .expect("the context hook must survive sharing SessionStart with status");
        assert!(
            context.get("matcher").is_none(),
            "the context hook must still run for every SessionStart source"
        );
        assert_eq!(
            starts.len(),
            2,
            "only the status and context hooks belong here"
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
        assert!(
            is_hook_helper(r"C:\old\target\debug\argus-hook.exe"),
            "a Windows path must be recognized whatever platform reads it"
        );
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
    fn codex_uses_its_project_hook_shape_and_cleans_up_only_its_handler() {
        let dir = tempfile::tempdir().unwrap();
        let codex = dir.path().join(".codex");
        std::fs::create_dir_all(&codex).unwrap();
        std::fs::write(
            codex.join("hooks.json"),
            r#"{"description":"mine","hooks":{"SessionStart":[{"matcher":"startup","hooks":[{"type":"command","command":"my-hook"}]}]}}"#,
        )
        .unwrap();

        let h = Harness::codex();
        h.install(dir.path(), PaneId(8), 4242, "tok").unwrap();
        let root = settings_of(dir.path(), &h);
        assert_eq!(root["description"], "mine");
        let groups = root["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(groups.len(), 2, "the user's SessionStart hook survives");
        let ours = groups
            .iter()
            .find(|group| group["matcher"] == "startup|resume|clear")
            .unwrap();
        let command = &ours["hooks"][0];
        assert!(command["command"]
            .as_str()
            .unwrap()
            .contains(SESSION_KEY_FLAG));
        assert!(command["commandWindows"].is_string());
        assert!(
            command.get("args").is_none(),
            "Codex requires one command string"
        );

        h.uninstall(dir.path()).unwrap();
        let root = settings_of(dir.path(), &h);
        assert_eq!(root["description"], "mine");
        assert_eq!(root["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert_eq!(
            root["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "my-hook"
        );
    }

    #[test]
    fn codex_hook_content_stays_stable_across_panes_and_daemon_boots() {
        // Codex trusts each project hook by a hash of its handler. Pane IDs,
        // ports, and per-boot tokens therefore cannot be baked into it.
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::codex();

        h.install(dir.path(), PaneId(1), 1111, "first-token")
            .unwrap();
        let first = settings_of(dir.path(), &h)["hooks"]["SessionStart"][0]["hooks"][0].clone();

        h.install(dir.path(), PaneId(9), 9999, "second-token")
            .unwrap();
        let second = settings_of(dir.path(), &h)["hooks"]["SessionStart"][0]["hooks"][0].clone();

        assert_eq!(
            first, second,
            "reinstalling must not invalidate Codex trust"
        );
        assert_eq!(
            second["command"],
            r#""$ARGUS_HOOK" "$ARGUS_HOOK_URL/status/idle" "$ARGUS_HOOK_TOKEN" "--session-id-from-stdin" "session_id" "--owns-session""#
        );
        assert_eq!(
            second["commandWindows"],
            r#""%ARGUS_HOOK%" "%ARGUS_HOOK_URL%/status/idle" "%ARGUS_HOOK_TOKEN%" "--session-id-from-stdin" "session_id" "--owns-session""#
        );
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
        let args: Vec<String> =
            serde_json::from_value(stop[0]["hooks"][0]["args"].clone()).unwrap();
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
        assert!(dir
            .path()
            .join(".claude")
            .join("settings.local.json")
            .is_file());
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
        assert!(
            root["hooks"]["PreToolUse"].is_array(),
            "the user's hook survives"
        );
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
        assert!(
            !dir.path().join(".claude").exists(),
            "must not create anything"
        );

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

        assert_eq!(
            settings_of(dir.path(), &h),
            original,
            "no residue left behind"
        );
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
    fn agents_are_told_to_isolate_branch_changes() {
        let text = instructions();

        assert!(text.contains("Never run `git switch` or `git checkout`"));
        assert!(text.contains("git worktree add"));
        assert!(text.contains("checkout"));
    }

    // --- the plugin mechanism ----------------------------------------------

    fn plugin_path(dir: &Path, h: &Harness) -> PathBuf {
        dir.join(&h.plugin.as_ref().unwrap().path)
    }

    #[test]
    fn opencode_reports_through_a_plugin_rather_than_a_hook_table() {
        // The bug this exists for: opencode has no JSON hooks, so before
        // this it resolved to the generic harness and sat at Idle for its
        // whole life however hard it was working.
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::opencode();
        assert!(h.settings.is_none(), "opencode has no hook file to write");

        h.install(dir.path(), PaneId(3), 4242, "tok").unwrap();
        let body = std::fs::read_to_string(plugin_path(dir.path(), &h)).unwrap();
        assert!(body.contains(PLUGIN_MARKER));
        // Per-pane facts stay in the environment the module reads at run
        // time, so one file is correct for every pane in the checkout.
        assert!(!body.contains("4242"), "a plugin must not bake in a port");
        assert!(!body.contains("tok"), "nor a token");
    }

    #[test]
    fn a_plugin_comes_back_out_with_the_directory_argus_made_for_it() {
        // Same contract as a hook block: it names a dead port the moment
        // this daemon exits, so it must not outlive the panes that need it.
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::opencode();
        h.install(dir.path(), PaneId(3), 4242, "tok").unwrap();
        h.uninstall(dir.path()).unwrap();
        assert!(!plugin_path(dir.path(), &h).exists());
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "an empty .opencode/ left behind is still litter"
        );
    }

    #[test]
    fn uninstalling_a_plugin_is_idempotent_and_safe_on_a_cold_checkout() {
        // It runs at startup across every configured checkout, most of
        // which have never hosted an agent.
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::opencode();
        h.uninstall(dir.path()).unwrap();
        h.install(dir.path(), PaneId(1), 1, "t").unwrap();
        h.uninstall(dir.path()).unwrap();
        h.uninstall(dir.path()).unwrap();
        assert!(!plugin_path(dir.path(), &h).exists());
    }

    #[test]
    fn a_plugin_the_user_wrote_themselves_is_left_alone() {
        // Their file, their directory. Only a module still carrying our
        // marker is ours to delete.
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::opencode();
        let path = plugin_path(dir.path(), &h);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "export const Mine = async () => ({})").unwrap();

        h.uninstall(dir.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "export const Mine = async () => ({})"
        );
    }

    #[test]
    fn the_opencode_plugin_maps_every_automatic_state() {
        // Completion states are explicit agent reports taught through the
        // injected instructions; lifecycle events supply only these states.
        let source = Harness::opencode().plugin.unwrap().source;
        for r in [
            Report::Working,
            Report::Idle,
            Report::Waiting,
            Report::Failed,
        ] {
            assert!(
                source.contains(&format!("\"{}\"", r.as_str())),
                "the opencode plugin never reports {}",
                r.as_str()
            );
        }
        // What a manual stop relies on: opencode drops the session to idle
        // on abort as well as on a finished turn.
        assert!(source.contains("session.status"));
        assert!(source.contains("chat.message"));
    }

    #[test]
    fn the_opencode_plugin_reports_root_and_child_sessions_without_transferring_ownership() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = dir.path().join("argus-status.mjs");
        let runner = dir.path().join("runner.mjs");
        std::fs::write(&plugin, Harness::opencode().plugin.unwrap().source).unwrap();
        std::fs::write(
            &runner,
            r#"
import { pathToFileURL } from "node:url";

const reports = [];
globalThis.fetch = async (url, init) => {
  reports.push({
    url,
    session: init.headers["X-Argus-Session"],
    authorization: init.headers.authorization,
    note: init.body,
  });
};

const { ArgusStatus } = await import(pathToFileURL(process.argv[2]));
const hooks = await ArgusStatus();
await hooks["chat.message"]({ sessionID: "old" });
await hooks.event({
  event: {
    type: "session.error",
    properties: { sessionID: "old", error: { name: "PermissionDenied" } },
  },
});
await hooks.event({
  event: {
    type: "session.created",
    properties: { info: { id: "child", parentID: "old" } },
  },
});
await hooks.event({
  event: {
    type: "permission.asked",
    properties: { sessionID: "child", title: "Approve child tool" },
  },
});
await hooks.event({
  event: {
    type: "session.deleted",
    properties: { info: { id: "child", parentID: "old" } },
  },
});
await hooks.event({
  event: {
    type: "session.created",
    properties: { sessionID: "new", info: { id: "new" } },
  },
});
await hooks["chat.message"]({ sessionID: "new" });
process.stdout.write(JSON.stringify(reports));
"#,
        )
        .unwrap();

        let output = match std::process::Command::new("node")
            .arg(&runner)
            .arg(&plugin)
            .env("ARGUS_HOOK_URL", "http://127.0.0.1/pane/1")
            .env("ARGUS_HOOK_TOKEN", "test-token")
            .output()
        {
            Ok(output) => output,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => panic!("could not run opencode plugin test: {e}"),
        };
        assert!(
            output.status.success(),
            "node failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let reports: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            reports,
            json!([
                { "url": "http://127.0.0.1/pane/1/session", "session": "old", "authorization": "Bearer test-token", "note": "old" },
                { "url": "http://127.0.0.1/pane/1/status/working", "session": "old", "authorization": "Bearer test-token", "note": "" },
                { "url": "http://127.0.0.1/pane/1/status/failed", "session": "old", "authorization": "Bearer test-token", "note": "PermissionDenied" },
                { "url": "http://127.0.0.1/pane/1/status/working", "session": "child", "authorization": "Bearer test-token", "note": "" },
                { "url": "http://127.0.0.1/pane/1/status/waiting", "session": "child", "authorization": "Bearer test-token", "note": "Approve child tool" },
                { "url": "http://127.0.0.1/pane/1/status/idle", "session": "child", "authorization": "Bearer test-token", "note": "" },
                { "url": "http://127.0.0.1/pane/1/session", "session": "new", "authorization": "Bearer test-token", "note": "new" },
                { "url": "http://127.0.0.1/pane/1/status/idle", "session": "new", "authorization": "Bearer test-token", "note": "" },
                { "url": "http://127.0.0.1/pane/1/status/working", "session": "new", "authorization": "Bearer test-token", "note": "" },
            ])
        );
    }

    #[test]
    fn the_opencode_plugin_calls_the_same_pane_api_the_helper_does() {
        // The module posts for itself rather than shelling out, so nothing
        // but this stops a change to the route or the environment's names
        // from leaving it talking to an endpoint that no longer exists.
        let source = Harness::opencode().plugin.unwrap().source;
        for var in [URL_VAR, TOKEN_VAR, INSTRUCTIONS_VAR] {
            assert!(source.contains(var), "the plugin never reads {var}");
        }
        assert!(source.contains("/status/${status}"), "wrong pane route");
        assert!(source.contains("Bearer ${TOKEN}"), "wrong authorization");
    }

    #[test]
    fn a_report_round_trips_through_its_wire_name() {
        for r in Report::ALL {
            assert_eq!(Report::parse(r.as_str()), Some(r));
        }
        assert_eq!(
            Report::parse("exited"),
            None,
            "only the daemon decides that"
        );
        assert_eq!(Report::parse(""), None);
    }

    #[test]
    fn every_report_maps_to_its_pane_status() {
        let expected = [
            PaneStatus::Working,
            PaneStatus::Idle,
            PaneStatus::Waiting,
            PaneStatus::NeedsReview,
            PaneStatus::Done,
            PaneStatus::Failed,
        ];
        assert_eq!(Report::ALL.map(Report::status), expected);
    }

    #[test]
    fn agy_installs_into_agents_hooks_json_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::agy();
        h.install(dir.path(), PaneId(5), 4242, "tok").unwrap();

        let hooks_file = dir.path().join(".agents").join("hooks.json");
        assert!(hooks_file.is_file(), "should write .agents/hooks.json");

        let rule_file = dir.path().join(".agents").join("rules").join("argus.md");
        assert!(rule_file.is_file(), "should write .agents/rules/argus.md");
        let rule_content = std::fs::read_to_string(&rule_file).unwrap();
        assert!(rule_content.contains("title"));

        let raw = std::fs::read_to_string(&hooks_file).unwrap();
        let root: Value = serde_json::from_str(&raw).unwrap();

        let argus = &root["argus"];
        assert!(
            argus.is_object(),
            "should be nested under 'argus' hook name"
        );

        let pre_inv = argus["PreInvocation"].as_array().unwrap();
        assert_eq!(pre_inv.len(), 1);
        let pre_entry = &pre_inv[0];
        assert_eq!(pre_entry["type"], "command");
        let pre_args: Vec<String> = serde_json::from_value(pre_entry["args"].clone()).unwrap();
        assert_eq!(pre_args[0], "http://127.0.0.1:4242/pane/5/status/working");
        assert_eq!(pre_args[1], "tok");
        assert_eq!(pre_args[2], SESSION_KEY_FLAG);
        assert_eq!(pre_args[3], "conversationId");

        let stop = argus["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        let stop_entry = &stop[0];
        assert_eq!(stop_entry["type"], "command");
        let stop_args: Vec<String> = serde_json::from_value(stop_entry["args"].clone()).unwrap();
        assert_eq!(stop_args[0], "http://127.0.0.1:4242/pane/5/status/idle");
        assert_eq!(stop_args[1], "tok");

        // Uninstall sweeps .agents/hooks.json, rules, and prunes the empty .agents directory
        h.uninstall(dir.path()).unwrap();
        assert!(!hooks_file.exists(), ".agents/hooks.json should be removed");
        assert!(
            !rule_file.exists(),
            ".agents/rules/argus.md should be removed"
        );
        assert!(
            !dir.path().join(".agents").exists(),
            "empty .agents dir should be pruned"
        );
    }
}
