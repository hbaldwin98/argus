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

#[derive(Debug, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub repos: Vec<String>,
    /// Which workspace this project belongs to. Absent means
    /// [`DEFAULT_WORKSPACE`].
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub cmd: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

/// Built-in agent templates used when the config has no `[[agent]]` entries,
/// so spawning a known agent CLI works zero-config.
pub fn default_agents() -> Vec<AgentConfig> {
    ["claude", "codex", "opencode"]
        .into_iter()
        .map(|name| AgentConfig {
            name: name.to_string(),
            cmd: vec![name.to_string()],
            env: Default::default(),
        })
        .collect()
}

/// Where Argus keeps its configuration. `ARGUS_CONFIG_DIR` overrides the
/// platform location — needed by tests, which must not read or scribble on
/// the real user's config, and handy for running a throwaway instance
/// alongside a real one.
pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ARGUS_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    directories::ProjectDirs::from("", "", "argus")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_path() -> PathBuf {
    config_dir().join("projects.toml")
}

const DEFAULT_CONFIG: &str = r#"# Argus projects. Each project groups one or more repositories.
#
# [[project]]
# name = "argus"
# repos = ["~/src/argus"]

# Agent templates available from the "a" picker. claude/codex/opencode are
# already built in with no config needed; add [[agent]] entries here to
# override them or add your own.
#
# [[agent]]
# name = "claude"
# cmd = ["claude"]
# env = { CLAUDE_PROJECT_DIR = "." }
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
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let file: ConfigFile =
        toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    Ok(file)
}

/// Appends a `[[project]]` block for a single-repo project to
/// `projects.toml` on disk, so a project added at runtime (any directory,
/// not just ones already configured) survives a daemon restart. Appends
/// raw text rather than round-tripping through serde so a user's existing
/// comments in the file aren't clobbered.
pub fn append_project(name: &str, repo_path: &Path, workspace: &str) -> Result<()> {
    let cfg_path = config_path();
    let repo = repo_path.to_string_lossy().replace('\\', "/");
    // `{:?}` on a &str produces a properly quote/backslash-escaped literal,
    // which is also valid TOML basic-string syntax.
    let block =
        format!("\n[[project]]\nname = {name:?}\nrepos = [{repo:?}]\nworkspace = {workspace:?}\n");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg_path)
        .with_context(|| format!("opening {}", cfg_path.display()))?;
    f.write_all(block.as_bytes())
        .with_context(|| format!("appending project to {}", cfg_path.display()))?;
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
