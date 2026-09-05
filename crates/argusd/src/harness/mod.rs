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

use argus_protocol::{PaneId, INSTRUCTIONS_COMMAND};
use serde::Deserialize;
use serde_json::{json, Value};

mod hooks;
mod install;
mod skill;

pub use hooks::{env, helper_path};
use hooks::*;
use install::*;

/// The statuses a harness can report, and the URL vocabulary they travel in.
///
/// Deliberately not [`PaneStatus`]: that has an `Exited` variant, which is
/// the daemon's to decide from the process, never a hook's to claim. It
/// lives in `argus-protocol` because `argus-hook` names the same statuses
/// from the other side of the wire.
pub use argus_protocol::Report;

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
    /// Whether this event's stdin carries the user's prompt, which the
    /// daemon should use as the pane's title. Prompt-submit events do;
    /// tool-start events must not, or every row is named after a tool.
    #[serde(default)]
    pub title_from_stdin: bool,
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
    /// Skip the status POST and talk only to `/session`. Cursor's
    /// `sessionStart` is fire-and-forget and can run after tools have
    /// already marked the pane working; posting `idle` then would snap
    /// the row back.
    #[serde(default)]
    pub claim_only: bool,
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
    /// context. Where Argus tells an agent how to load its skill.
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
    /// `command` plus `args` shape.
    pub command_string: bool,
    /// When [`command_string`] is set, bake the helper path, pane URL, and
    /// token into the command. Cursor's hook runner does not inherit the
    /// pane environment (so env-based routing silently no-ops), while Codex
    /// must keep the env form so its trust hash stays stable across boots.
    pub bake_command: bool,
    /// Optional workspace rule markdown file to install into checkout.
    pub rule_file: Option<PathBuf>,
    /// Where this harness discovers the managed Argus skill, relative to the checkout.
    pub skill_dir: Option<PathBuf>,
    /// Top-level `version` some settings files require (Cursor's hooks.json).
    pub settings_version: Option<u64>,
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
            bake_command: false,
            rule_file: None,
            skill_dir: None,
            settings_version: None,
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
                    title_from_stdin: true,
                    session_id_key: Some("session_id".into()),
                    owns_session: false,
                    claim_only: false,
                },
                Event {
                    name: "Stop".into(),
                    reports: Report::Idle,
                    matcher: None,
                    note_from_stdin: false,
                    title_from_stdin: false,
                    session_id_key: Some("session_id".into()),
                    owns_session: false,
                    claim_only: false,
                },
                Event {
                    name: "Notification".into(),
                    reports: Report::Waiting,
                    matcher: None,
                    // Carries the text of what it is asking for.
                    note_from_stdin: true,
                    title_from_stdin: false,
                    session_id_key: Some("session_id".into()),
                    owns_session: false,
                    claim_only: false,
                },
                Event {
                    name: "SessionStart".into(),
                    reports: Report::Idle,
                    // Compaction starts a fresh context while the same turn
                    // is still running, so it must not make the pane idle.
                    matcher: Some("startup|resume|clear|fork".into()),
                    note_from_stdin: false,
                    title_from_stdin: false,
                    session_id_key: Some("session_id".into()),
                    owns_session: true,
                    claim_only: false,
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
            bake_command: false,
            rule_file: None,
            skill_dir: Some(PathBuf::from(".claude/skills/argus")),
            settings_version: None,
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
                title_from_stdin: false,
                session_id_key: Some("session_id".into()),
                owns_session: true,
                claim_only: false,
            }],
            context_event: Some("SessionStart".to_string()),
            plugin: None,
            resume: vec!["resume".to_string(), "--last".to_string()],
            resume_id: vec!["resume".to_string(), "{session_id}".to_string()],
            command_string: true,
            bake_command: false,
            rule_file: None,
            skill_dir: Some(PathBuf::from(".agents/skills/argus")),
            settings_version: None,
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
            bake_command: false,
            rule_file: None,
            skill_dir: Some(PathBuf::from(".agents/skills/argus")),
            settings_version: None,
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
                    title_from_stdin: true,
                    session_id_key: Some("conversationId".into()),
                    owns_session: true,
                    claim_only: false,
                },
                Event {
                    name: "Stop".into(),
                    reports: Report::Idle,
                    matcher: None,
                    note_from_stdin: false,
                    title_from_stdin: false,
                    session_id_key: Some("conversationId".into()),
                    owns_session: false,
                    claim_only: false,
                },
            ],
            context_event: None,
            plugin: None,
            resume: vec!["--continue".to_string()],
            resume_id: vec!["--conversation".to_string(), "{session_id}".to_string()],
            command_string: false,
            bake_command: false,
            rule_file: Some(PathBuf::from(".agents").join("rules").join("argus.md")),
            skill_dir: Some(PathBuf::from(".agents/skills/argus")),
            settings_version: None,
        }
    }

    /// Cursor Agent (`agent` CLI) discovers project hooks in `.cursor/hooks.json`
    /// and always-on rules in `.cursor/rules/`. Hooks use a shell command string
    /// and require top-level `version: 1`. `sessionStart` claims identity
    /// without posting idle — the event is fire-and-forget and can arrive
    /// after tools have already marked working. `beforeSubmitPrompt` plus
    /// tool-start events (`preToolUse`, `beforeShellExecution`) mark working
    /// — the CLI often skips lifecycle hooks, so tool-start is what actually
    /// turns the pane; `stop` marks idle.
    pub fn agent() -> Harness {
        Harness {
            name: "agent".to_string(),
            settings: Some(PathBuf::from(".cursor").join("hooks.json")),
            hooks_key: "hooks".to_string(),
            shape: Shape::Flat,
            events: vec![
                Event {
                    name: "sessionStart".into(),
                    reports: Report::Idle,
                    matcher: None,
                    note_from_stdin: false,
                    title_from_stdin: false,
                    session_id_key: Some("conversation_id".into()),
                    owns_session: true,
                    claim_only: true,
                },
                Event {
                    name: "beforeSubmitPrompt".into(),
                    reports: Report::Working,
                    matcher: None,
                    note_from_stdin: false,
                    title_from_stdin: true,
                    session_id_key: Some("conversation_id".into()),
                    owns_session: false,
                    claim_only: false,
                },
                Event {
                    name: "preToolUse".into(),
                    reports: Report::Working,
                    matcher: None,
                    note_from_stdin: false,
                    title_from_stdin: false,
                    session_id_key: Some("conversation_id".into()),
                    owns_session: false,
                    claim_only: false,
                },
                Event {
                    name: "beforeShellExecution".into(),
                    reports: Report::Working,
                    matcher: None,
                    note_from_stdin: false,
                    title_from_stdin: false,
                    session_id_key: Some("conversation_id".into()),
                    owns_session: false,
                    claim_only: false,
                },
                Event {
                    name: "stop".into(),
                    reports: Report::Idle,
                    matcher: None,
                    note_from_stdin: false,
                    title_from_stdin: false,
                    session_id_key: Some("conversation_id".into()),
                    owns_session: false,
                    claim_only: false,
                },
            ],
            context_event: None,
            plugin: None,
            resume: vec!["--continue".to_string()],
            resume_id: vec!["--resume".to_string(), "{session_id}".to_string()],
            command_string: true,
            // Cursor spawns hooks without the pane environment, so env-based
            // routing never reaches the daemon; title updates from the shell
            // still work because they inherit ARGUS_HOOK_*.
            bake_command: true,
            rule_file: Some(PathBuf::from(".cursor").join("rules").join("argus.mdc")),
            skill_dir: Some(PathBuf::from(".agents/skills/argus")),
            settings_version: Some(1),
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
            Harness::agent(),
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
}

#[cfg(test)]
mod tests;
