//! `projects.toml`: what the user declares.
//!
//! Read-only — nothing here is ever written back. What the user does to
//! the panel while it is running is an overlay in the store, folded over
//! this at startup by [`with_overlays`], so hand-editing the file and
//! adding a project from the UI never fight over the same lines.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct ConfigFile {
    #[serde(default, rename = "workspace")]
    pub workspaces: Vec<WorkspaceConfig>,
    #[serde(default, rename = "project")]
    pub projects: Vec<ProjectConfig>,
    #[serde(default, rename = "agent")]
    pub agents: Vec<AgentConfig>,
    /// Descriptions of agent CLIs Argus doesn't ship knowledge of. A block
    /// whose name matches a built-in replaces it.
    #[serde(default, rename = "harness")]
    pub harnesses: Vec<HarnessConfig>,
}

/// A named group of projects (DESIGN.md §11). Declaring one is optional —
/// a project's `workspace` key creates it implicitly, and projects with no
/// key at all land in [`DEFAULT_WORKSPACE`].
#[derive(Debug, Deserialize)]
pub struct WorkspaceConfig {
    pub name: String,
}

/// Every install has this workspace, whether or not the config names it.
/// It is where unassigned projects go, so a config that has never heard of
/// workspaces keeps working unchanged.
pub const DEFAULT_WORKSPACE: &str = "default";

#[derive(Debug, Default, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    /// A directory to find repositories under. Every Git repository at or
    /// beneath it becomes a repository of this project, and keeps doing so
    /// as repositories are cloned into it or removed from it. See
    /// `git::discover_repositories`.
    #[serde(default)]
    pub root: Option<String>,
    /// Repositories named one at a time, joined to whatever `root` finds.
    /// A path here is taken at its word: one that is not a Git repository
    /// at all still becomes a row, which is how a plain directory gets
    /// panes.
    #[serde(default)]
    pub repos: Vec<String>,
    /// Which workspace this project belongs to. Absent means
    /// [`DEFAULT_WORKSPACE`].
    #[serde(default)]
    pub workspace: Option<String>,
    /// Where worktrees Argus creates for this project's repositories go.
    /// Absent keeps them under `<primary>/.argus/worktrees`; a directory
    /// here holds one subdirectory per repository, so two repositories can
    /// have a branch of the same name without landing on each other.
    #[serde(default)]
    pub worktree_root: Option<String>,
    /// Directories under `root` the scan must not walk into, beyond the
    /// ones it always skips. A bare name matches anywhere under the root;
    /// a `/`-separated path matches that one directory.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Directories to walk into anyway — this beats both `exclude` and the
    /// built-in skips, so a repository kept somewhere the defaults would
    /// never look is still reachable by naming it.
    #[serde(default)]
    pub include: Vec<String>,
    /// Whether a checkout in this project may hold only one agent at a
    /// time. Sharing a checkout is allowed by default and merely shown;
    /// this turns it into a refusal, for repositories where two agents
    /// editing the same files is never what was meant.
    #[serde(default)]
    pub exclusive: bool,
    /// Commands to run in a worktree Argus has just created — installing
    /// dependencies, seeding an untracked config file, whatever a fresh
    /// checkout needs before it is worth opening. Each is parsed into
    /// arguments the way the editor command is, and run without a shell.
    #[serde(default)]
    pub setup: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub cmd: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// Which harness this CLI speaks. Absent means the template's own name
    /// if that names a harness, and `generic` otherwise — so `name =
    /// "claude"` keeps working with no extra key.
    #[serde(default)]
    pub harness: Option<String>,
    /// What to do when the CLI exits on its own.
    #[serde(default)]
    pub restart: Restart,
}

/// Whether a pane starts its agent again when the process ends. Closing a
/// pane is never a restart: that takes the row out first, and what is gone
/// has nothing to restart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Restart {
    /// Leave the exited row where it is, for the operator to read and
    /// close. What every agent did before there was a choice.
    #[default]
    Never,
    /// Start again only when the CLI exited non-zero — a crash, a killed
    /// process, an API that gave up — and leave a clean exit alone.
    OnFailure,
    /// Start again however it ended, for a CLI whose normal end is exiting.
    Always,
}

/// How a particular agent CLI can be asked to report its status, in the
/// user's config rather than in Argus's source. See `harness.rs`.
#[derive(Debug, Clone, Deserialize)]
pub struct HarnessConfig {
    pub name: String,
    /// Path to the harness's hook config, relative to the checkout. Absent
    /// means the harness takes only the environment.
    #[serde(default)]
    pub settings: Option<String>,
    #[serde(default = "default_hooks_key")]
    pub hooks_key: String,
    #[serde(default = "default_shape")]
    pub shape: crate::harness::Shape,
    /// Event name -> what it reports, e.g. `turn_end = "idle"` or
    /// `ask = { reports = "waiting", note = true }` or
    /// `prompt = { reports = "working", title = true }`.
    #[serde(default)]
    pub events: std::collections::BTreeMap<String, EventConfig>,
    /// An event whose command's stdout the harness feeds to the model.
    #[serde(default)]
    pub context_event: Option<String>,
    /// Arguments that make this CLI continue its last conversation, added
    /// to the agent's `cmd` when Argus restores a recorded pane.
    #[serde(default)]
    pub resume: Vec<String>,
    /// Arguments for reopening one exact conversation. Every occurrence of
    /// `{session_id}` is replaced with the captured ID and passed as argv.
    #[serde(default)]
    pub resume_id: Vec<String>,
    /// Optional workspace rule markdown file to install into checkout.
    #[serde(default)]
    pub rule_file: Option<String>,
}

/// A bare status is the common case; the table form is for an event that
/// hands its hook a message worth showing as the pane's note.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EventConfig {
    Reports(crate::harness::Report),
    Detailed {
        reports: crate::harness::Report,
        #[serde(default)]
        note: bool,
        #[serde(default)]
        title: bool,
        #[serde(default)]
        matcher: Option<String>,
        /// Top-level stdin JSON key containing the harness session ID.
        #[serde(default)]
        session_id: Option<String>,
        /// Whether this event is the harness announcing its own session
        /// start, and so may claim the identity the pane resumes from.
        #[serde(default)]
        owns_session: bool,
    },
}

impl EventConfig {
    fn into_event(self, name: String) -> crate::harness::Event {
        match self {
            EventConfig::Reports(reports) => crate::harness::Event {
                name,
                reports,
                matcher: None,
                note_from_stdin: false,
                title_from_stdin: false,
                session_id_key: None,
                owns_session: false,
                claim_only: false,
            },
            EventConfig::Detailed {
                reports,
                note,
                title,
                matcher,
                session_id,
                owns_session,
            } => crate::harness::Event {
                name,
                reports,
                matcher,
                note_from_stdin: note,
                title_from_stdin: title,
                session_id_key: session_id,
                owns_session,
                claim_only: false,
            },
        }
    }
}

fn default_hooks_key() -> String {
    "hooks".to_string()
}

fn default_shape() -> crate::harness::Shape {
    crate::harness::Shape::Flat
}

impl From<HarnessConfig> for crate::harness::Harness {
    fn from(c: HarnessConfig) -> Self {
        crate::harness::Harness {
            name: c.name,
            settings: c.settings.map(PathBuf::from),
            hooks_key: c.hooks_key,
            shape: c.shape,
            events: c
                .events
                .into_iter()
                .map(|(name, e)| e.into_event(name))
                .collect(),
            context_event: c.context_event,
            // A plugin is a program, not a dialect, so it can only come
            // from a built-in. See `harness::Plugin`.
            plugin: None,
            resume: c.resume,
            resume_id: c.resume_id,
            command_string: false,
            bake_command: false,
            rule_file: c.rule_file.map(PathBuf::from),
            settings_version: None,
        }
    }
}

/// The harnesses Argus will use this run: built-ins, with any same-named
/// block from the config replacing one.
pub fn harnesses(configured: Vec<HarnessConfig>) -> Vec<crate::harness::Harness> {
    let mut out = crate::harness::Harness::builtins();
    for c in configured {
        let h: crate::harness::Harness = c.into();
        match out.iter().position(|b| b.name == h.name) {
            Some(i) => out[i] = h,
            None => out.push(h),
        }
    }
    out
}

/// Built-in agent templates used when the config has no `[[agent]]` entries,
/// so spawning a known agent CLI works zero-config.
pub fn default_agents() -> Vec<AgentConfig> {
    ["claude", "codex", "opencode", "agy", "agent"]
        .into_iter()
        .map(|name| AgentConfig {
            name: name.to_string(),
            cmd: vec![name.to_string()],
            env: Default::default(),
            harness: None,
            restart: Restart::Never,
        })
        .collect()
}

pub use argus_protocol::config_dir;

pub fn config_path() -> PathBuf {
    config_dir().join("projects.toml")
}

const DEFAULT_CONFIG: &str = r#"# Argus projects. Each project groups one or more repositories.
#
# `root` is a directory to find them under: every Git repository at or
# beneath it becomes a repository of the project, including ones cloned
# there later. `repos` names them one at a time instead, and a path there
# need not be a Git repository at all. A project can use either or both.
#
# [[project]]
# name = "src"
# root = "~/src"
#
# [[project]]
# name = "argus"
# repos = ["~/src/argus"]

# Agent templates available from the "a" picker. claude/codex/opencode/agy/agent
# are already built in with no config needed; add [[agent]] entries here to
# override them or add your own.
#
# [[agent]]
# name = "claude"
# cmd = ["claude"]
# env = { CLAUDE_PROJECT_DIR = "." }

# Every agent pane is handed ARGUS_HOOK_URL, ARGUS_HOOK_TOKEN and ARGUS_HOOK,
# so any CLI that can run a command can report its status and rename its own
# pane. Describe a CLI's hook file here to have Argus wire it up itself:
#
# [[harness]]
# name = "herdr"
# settings = ".herdr/hooks.json"
# hooks_key = "hooks"
# shape = "flat"            # or "matcher" for Claude Code's nesting
#
# resume = ["--continue"]   # legacy: reopen the last conversation
# resume_id = ["--resume", "{session_id}"] # reopen one exact conversation
#
# [harness.events]
# turn_start = "working"
# turn_end = "idle"
# ask = { reports = "waiting", note = true }   # note: stdin explains why
# prompt = { reports = "working", title = true } # title: stdin is the user prompt
# start = { reports = "idle", session_id = "session_id" }
#
# [[agent]]
# name = "herdr"
# cmd = ["herdr"]
# harness = "herdr"
"#;

pub fn load() -> Result<ConfigFile> {
    let path = config_path();
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(&path, DEFAULT_CONFIG)
            .with_context(|| format!("writing default config to {}", path.display()))?;
    }
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let file: ConfigFile =
        toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    Ok(file)
}

/// The config as the panel should show it: what the user declared, plus
/// what they did to the panel while Argus was running.
///
/// Merged here rather than written back, because `projects.toml` is the
/// user's — hand-edited, full of their comments, and read-only as far as
/// Argus is concerned. So a project added from the TUI lives in the store,
/// and a declared one the user removed is hidden rather than deleted:
/// taking a row out of the panel is not permission to edit their file.
pub fn with_overlays(mut config: ConfigFile, overlays: &crate::store::Overlays) -> ConfigFile {
    config
        .projects
        .retain(|p| !overlays.hidden.iter().any(|h| h == &p.name));

    for (project, path) in &overlays.repos {
        if let Some(p) = config.projects.iter_mut().find(|p| &p.name == project) {
            p.repos.push(path_str(path));
        }
    }

    for (added, repos) in &overlays.projects {
        // The config wins for a directory that appears in both: the file is
        // the source of truth, and the overlay is only what it never said.
        // Matched on the root because that is what the user added — two
        // projects that share a basename are still two directories.
        let declared = added.root.as_os_str();
        if config
            .projects
            .iter()
            .filter_map(|p| p.root.as_deref())
            .any(|root| expand_home(root).as_os_str() == declared)
        {
            continue;
        }
        config.projects.push(ProjectConfig {
            name: added.name.clone(),
            // The root, not the repositories found under it: the point of
            // adding a directory is that Argus keeps looking there, so a
            // repository cloned into it next month arrives on its own.
            root: Some(path_str(&added.root)),
            repos: repos.iter().map(|r| path_str(r)).collect(),
            workspace: Some(added.workspace.clone()),
            ..Default::default()
        });
    }

    for name in &overlays.workspaces {
        if !config.workspaces.iter().any(|w| &w.name == name) {
            config
                .workspaces
                .push(WorkspaceConfig { name: name.clone() });
        }
    }

    config
}

fn path_str(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> ConfigFile {
        toml::from_str(raw).expect("the test config should parse")
    }

    #[test]
    fn a_project_predating_roots_still_names_its_repositories_outright() {
        // `repos` is the schema every existing config is written in. It has
        // to keep meaning exactly what it meant, root or no root.
        let cfg = parse(
            r#"
[[project]]
name = "argus"
repos = ["~/src/argus", "~/src/notes"]
"#,
        );
        let project = &cfg.projects[0];
        assert_eq!(project.repos, ["~/src/argus", "~/src/notes"]);
        assert!(project.root.is_none());
    }

    #[test]
    fn a_project_can_name_a_root_to_find_its_repositories_under() {
        let cfg = parse(
            r#"
[[project]]
name = "src"
root = "~/src"
repos = ["~/scratch"]
"#,
        );
        let project = &cfg.projects[0];
        assert_eq!(project.root.as_deref(), Some("~/src"));
        assert_eq!(project.repos, ["~/scratch"], "the two combine");
    }

    #[test]
    fn a_harness_block_can_say_how_its_cli_resumes() {
        // The point of the key: a CLI Argus ships no knowledge of can still
        // come back holding the conversation it had.
        let cfg = parse(
            r#"
[[harness]]
name = "herdr"
resume = ["--continue"]
"#,
        );
        let herdr = harnesses(cfg.harnesses)
            .into_iter()
            .find(|h| h.name == "herdr")
            .expect("a configured harness joins the built-ins");
        assert_eq!(herdr.resume, ["--continue"]);
    }

    #[test]
    fn a_custom_harness_can_capture_and_resume_an_exact_session() {
        let cfg = parse(
            r#"
[[harness]]
name = "herdr"
resume_id = ["--session", "{session_id}"]

[harness.events]
start = { reports = "idle", session_id = "conversation_id" }
"#,
        );
        let harness: crate::harness::Harness = cfg.harnesses.into_iter().next().unwrap().into();
        assert_eq!(harness.resume_id, ["--session", "{session_id}"]);
        assert_eq!(
            harness.events[0].session_id_key.as_deref(),
            Some("conversation_id")
        );
    }

    #[test]
    fn a_matcher_shaped_harness_preserves_an_event_matcher() {
        let cfg = parse(
            r#"
[[harness]]
name = "custom-claude"
shape = "matcher"

[harness.events]
SessionStart = { reports = "idle", matcher = "startup|resume|clear|fork" }
"#,
        );
        let harness: crate::harness::Harness = cfg
            .harnesses
            .into_iter()
            .next()
            .expect("the configured harness should exist")
            .into();

        assert_eq!(harness.events.len(), 1);
        assert_eq!(
            harness.events[0].matcher.as_deref(),
            Some("startup|resume|clear|fork")
        );
    }

    #[test]
    fn a_custom_harness_can_title_the_pane_from_a_prompt_event() {
        let cfg = parse(
            r#"
[[harness]]
name = "herdr"

[harness.events]
prompt = { reports = "working", title = true }
ask = { reports = "waiting", note = true }
"#,
        );
        let harness: crate::harness::Harness = cfg.harnesses.into_iter().next().unwrap().into();
        let prompt = harness
            .events
            .iter()
            .find(|e| e.name == "prompt")
            .expect("prompt event");
        assert!(prompt.title_from_stdin);
        assert!(!prompt.note_from_stdin);
        let ask = harness
            .events
            .iter()
            .find(|e| e.name == "ask")
            .expect("ask event");
        assert!(ask.note_from_stdin);
        assert!(!ask.title_from_stdin);
    }

    #[test]
    fn a_custom_harness_can_report_completion_states() {
        let cfg = parse(
            r#"
[[harness]]
name = "reviewer"

[harness.events]
ready = "needs-review"
complete = "done"
"#,
        );
        let harness: crate::harness::Harness = cfg.harnesses.into_iter().next().unwrap().into();
        assert!(harness
            .events
            .iter()
            .any(|e| e.reports == crate::harness::Report::NeedsReview));
        assert!(harness
            .events
            .iter()
            .any(|e| e.reports == crate::harness::Report::Done));
    }

    #[test]
    fn a_block_that_replaces_a_built_in_replaces_what_it_knew_about_resuming() {
        // Same rule as its plugin: a same-named block is the whole harness,
        // so a user overriding `claude` has to restate `resume` to keep it.
        let cfg = parse(
            r#"
[[harness]]
name = "claude"
"#,
        );
        let all = harnesses(cfg.harnesses);
        assert_eq!(all.iter().filter(|h| h.name == "claude").count(), 1);
        assert!(all
            .iter()
            .find(|h| h.name == "claude")
            .expect("still there, just theirs now")
            .resume
            .is_empty());
    }
}
