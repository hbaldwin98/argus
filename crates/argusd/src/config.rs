use std::io::Write;
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
    /// `ask = { reports = "waiting", note = true }`.
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
                session_id_key: None,
                owns_session: false,
            },
            EventConfig::Detailed {
                reports,
                note,
                matcher,
                session_id,
                owns_session,
            } => crate::harness::Event {
                name,
                reports,
                matcher,
                note_from_stdin: note,
                session_id_key: session_id,
                owns_session,
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

/// Appends a `[[project]]` block to `projects.toml` on disk, so a project
/// added at runtime (any directory, not just ones already configured)
/// survives a daemon restart. Appends raw text rather than round-tripping
/// through serde so a user's existing comments in the file aren't clobbered.
///
/// What it writes is the root, not the repositories found under it: the
/// point of adding a directory is that Argus keeps looking there, so a
/// repository cloned into it next month arrives without the user editing
/// anything.
pub fn append_project(name: &str, root_path: &Path, workspace: &str) -> Result<()> {
    let cfg_path = config_path();
    let root = root_path.to_string_lossy().replace('\\', "/");
    // `{:?}` on a &str produces a properly quote/backslash-escaped literal,
    // which is also valid TOML basic-string syntax.
    let block =
        format!("\n[[project]]\nname = {name:?}\nroot = {root:?}\nworkspace = {workspace:?}\n");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg_path)
        .with_context(|| format!("opening {}", cfg_path.display()))?;
    f.write_all(block.as_bytes())
        .with_context(|| format!("appending project to {}", cfg_path.display()))?;
    Ok(())
}

/// Deletes the `[[project]]` block at `index` (counting project blocks in
/// file order, which is the order [`ConfigFile::projects`] is built in) —
/// how a project leaves the panel for good rather than until the next
/// restart. Text-level like [`append_project`], and for the same reason:
/// the rest of the file is the user's, comments included, and a serde
/// round-trip would rewrite all of it.
///
/// `name` is what the daemon believes sits at that index. If the file has
/// been edited by hand since it was loaded the index may have moved, so a
/// mismatch falls back to a block that uniquely carries that name, and
/// removes nothing at all when even that is ambiguous.
pub fn remove_project(index: usize, name: &str) -> Result<()> {
    let cfg_path = config_path();
    let raw = std::fs::read_to_string(&cfg_path)
        .with_context(|| format!("reading {}", cfg_path.display()))?;
    let lines: Vec<&str> = raw.lines().collect();
    let Some((start, end)) = locate_project(&lines, index, name) else {
        anyhow::bail!("no project named {name:?} in {}", cfg_path.display());
    };

    // Comments sitting directly on top of the block introduce it, and
    // reading them under the *next* project is worse than losing them.
    // Anything further up is separated by a blank line and stays.
    let mut start = start;
    while start > 0 && lines[start - 1].trim_start().starts_with('#') {
        start -= 1;
    }

    // The blank line `append_project` writes ahead of each block goes with
    // it, so adding and removing the same project leaves the file as it
    // was rather than growing a gap every round trip.
    let start = if start > 0 && lines[start - 1].trim().is_empty() {
        start - 1
    } else {
        start
    };

    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    kept.extend_from_slice(&lines[..start]);
    kept.extend_from_slice(&lines[end..]);
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    std::fs::write(&cfg_path, out).with_context(|| format!("rewriting {}", cfg_path.display()))?;
    Ok(())
}

/// The line range of the `[[project]]` block the daemon believes is
/// `name` at `index`. If the file has been edited by hand since it was
/// loaded the index may have moved, so a mismatch falls back to a block
/// that uniquely carries that name, and gives up when even that is
/// ambiguous — better to touch nothing than the wrong project.
fn locate_project(lines: &[&str], index: usize, name: &str) -> Option<(usize, usize)> {
    let blocks = project_blocks(lines);
    match blocks.get(index) {
        Some(&(start, end)) if block_name(&lines[start..end]).as_deref() == Some(name) => {
            Some((start, end))
        }
        _ => {
            let mut matching = blocks
                .iter()
                .filter(|&&(start, end)| block_name(&lines[start..end]).as_deref() == Some(name));
            match (matching.next(), matching.next()) {
                (Some(&only), None) => Some(only),
                _ => None,
            }
        }
    }
}

/// Adds one repository path to the `repos` list of the `[[project]]` block
/// at `index` — the counterpart to [`append_project`] for a repository the
/// project's root would never find, and the reason `repos` and `root` can
/// both be set on one project.
///
/// Text-level like the rest of this module, so the user's other blocks and
/// comments survive. Only the `repos` key is rewritten, and it is rewritten
/// as one line: the paths are the value, their formatting isn't.
pub fn append_repo(index: usize, name: &str, path: &Path) -> Result<()> {
    let cfg_path = config_path();
    let raw = std::fs::read_to_string(&cfg_path)
        .with_context(|| format!("reading {}", cfg_path.display()))?;
    let lines: Vec<&str> = raw.lines().collect();
    let Some((start, end)) = locate_project(&lines, index, name) else {
        anyhow::bail!("no project named {name:?} in {}", cfg_path.display());
    };

    let mut repos = block_repos(&lines[start..end]);
    repos.push(path.to_string_lossy().replace('\\', "/"));
    let rendered = format!(
        "repos = [{}]",
        repos
            .iter()
            .map(|r| format!("{r:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut out: Vec<String> = lines[..start].iter().map(|l| l.to_string()).collect();
    match repos_span(&lines[start..end]) {
        Some((key_start, key_end)) => {
            out.extend(
                lines[start..start + key_start]
                    .iter()
                    .map(|l| l.to_string()),
            );
            out.push(rendered);
            out.extend(lines[start + key_end..end].iter().map(|l| l.to_string()));
        }
        None => {
            // After the block's own keys but before the blank line that
            // separates it from whatever follows, so the file keeps its
            // shape.
            let mut at = end;
            while at > start + 1 && lines[at - 1].trim().is_empty() {
                at -= 1;
            }
            out.extend(lines[start..at].iter().map(|l| l.to_string()));
            out.push(rendered);
            out.extend(lines[at..end].iter().map(|l| l.to_string()));
        }
    }
    out.extend(lines[end..].iter().map(|l| l.to_string()));

    let mut text = out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    std::fs::write(&cfg_path, text).with_context(|| format!("rewriting {}", cfg_path.display()))?;
    Ok(())
}

/// The `repos` a block already lists, read as TOML for the same reason
/// [`block_name`] is.
fn block_repos(block: &[&str]) -> Vec<String> {
    let Some(body) = block.get(1..).map(|b| b.join("\n")) else {
        return Vec::new();
    };
    let Ok(table) = toml::from_str::<toml::Table>(&body) else {
        return Vec::new();
    };
    table
        .get("repos")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// The block-relative line range the `repos` assignment occupies, arrays
/// written across several lines included.
fn repos_span(block: &[&str]) -> Option<(usize, usize)> {
    let start = block.iter().position(|l| {
        let t = l.trim_start();
        t.strip_prefix("repos")
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })?;
    let mut depth = 0i32;
    for (offset, line) in block[start..].iter().enumerate() {
        depth += line.chars().filter(|&c| c == '[').count() as i32;
        depth -= line.chars().filter(|&c| c == ']').count() as i32;
        if depth <= 0 {
            return Some((start, start + offset + 1));
        }
    }
    Some((start, block.len()))
}

/// Half-open line ranges of each `[[project]]` block, header included. A
/// block runs until the next table header of any kind, so the trailing
/// blank line before that header goes with the block it follows.
fn project_blocks(lines: &[&str]) -> Vec<(usize, usize)> {
    let header = |line: &str| line.trim_start().starts_with('[');
    let mut blocks = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("[[project]]") {
            let end = lines[i + 1..]
                .iter()
                .position(|l| header(l))
                .map(|offset| i + 1 + offset)
                .unwrap_or(lines.len());
            blocks.push((i, end));
        }
    }
    blocks
}

/// The `name` a block declares, read by parsing the block's body as the
/// table it is — so quoting and escapes mean what TOML says they mean
/// rather than what a regex would guess.
fn block_name(block: &[&str]) -> Option<String> {
    let body = block.get(1..)?.join("\n");
    let table: toml::Table = toml::from_str(&body).ok()?;
    Some(table.get("name")?.as_str()?.to_string())
}

/// Repository paths the user has taken out of the panel. Kept beside
/// `projects.toml` rather than in it, for the same reason `open-workspace`
/// is: which of a scan's results you want to look at is Argus's
/// bookkeeping about a directory listing, not a description of the
/// project. Removing the project drops its repositories with it, so this
/// file only ever holds paths under projects that still exist.
fn excluded_path() -> PathBuf {
    config_path().with_file_name("excluded-repos")
}

/// One path per line, absolute and as written when it was excluded.
pub fn load_excluded_repos() -> Vec<PathBuf> {
    let Ok(raw) = std::fs::read_to_string(excluded_path()) else {
        return Vec::new();
    };
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Replaces the exclusion file with exactly these paths — how exclusions
/// under a removed project are forgotten.
pub fn rewrite_excluded_repos(paths: &[PathBuf]) -> Result<()> {
    let file = excluded_path();
    if paths.is_empty() {
        // Nothing excluded and no file is the same state, and the tidier
        // one to leave behind.
        match std::fs::remove_file(&file) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e).with_context(|| format!("removing {}", file.display())),
        }
    }
    let body: String = paths.iter().map(|p| format!("{}\n", p.display())).collect();
    std::fs::write(&file, body).with_context(|| format!("writing {}", file.display()))
}

pub fn append_excluded_repo(path: &Path) -> Result<()> {
    let file = excluded_path();
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .with_context(|| format!("opening {}", file.display()))?;
    writeln!(f, "{}", path.display())
        .with_context(|| format!("appending to {}", file.display()))?;
    Ok(())
}

/// Appends a `[[workspace]]` block, so a workspace created from the TUI
/// outlives the daemon. Declaring it is what makes an empty workspace
/// exist at all: a workspace with no projects has nothing else in the file
/// to imply it.
pub fn append_workspace(name: &str) -> Result<()> {
    let cfg_path = config_path();
    let block = format!("\n[[workspace]]\nname = {name:?}\n");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg_path)
        .with_context(|| format!("opening {}", cfg_path.display()))?;
    f.write_all(block.as_bytes())
        .with_context(|| format!("appending workspace to {}", cfg_path.display()))?;
    Ok(())
}

/// Where the name of the open workspace is remembered. A file of its own
/// rather than a key in `projects.toml`: that file is the user's, edited by
/// hand and full of their comments, and `append_project` deliberately only
/// ever appends to it. Which workspace happens to be open is Argus's
/// bookkeeping, not configuration, so it lives beside it instead.
fn open_workspace_path() -> PathBuf {
    config_path().with_file_name("open-workspace")
}

/// The workspace that was open when the daemon last exited, if it was ever
/// recorded. The caller resolves it against the workspaces that actually
/// exist — the name may since have been removed from the config.
pub fn load_open_workspace() -> Option<String> {
    let raw = std::fs::read_to_string(open_workspace_path()).ok()?;
    let name = raw.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// Best-effort: failing to remember the open workspace is not worth
/// failing the switch the user just asked for.
pub fn save_open_workspace(name: &str) {
    let path = open_workspace_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&path, name) {
        tracing::warn!("could not remember the open workspace: {e}");
    }
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
