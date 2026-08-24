use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    #[serde(default, rename = "project")]
    pub projects: Vec<ProjectConfig>,
    #[serde(default, rename = "agent")]
    pub agents: Vec<AgentConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub repos: Vec<String>,
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

pub fn config_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "orion")
        .map(|d| d.config_dir().join("projects.toml"))
        .unwrap_or_else(|| PathBuf::from("projects.toml"))
}

const DEFAULT_CONFIG: &str = r#"# Orion projects. Each project groups one or more repositories.
#
# [[project]]
# name = "orion"
# repos = ["~/src/orion"]

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
pub fn append_project(name: &str, repo_path: &Path) -> Result<()> {
    let cfg_path = config_path();
    let repo = repo_path.to_string_lossy().replace('\\', "/");
    // `{:?}` on a &str produces a properly quote/backslash-escaped literal,
    // which is also valid TOML basic-string syntax.
    let block = format!("\n[[project]]\nname = {name:?}\nrepos = [{repo:?}]\n");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg_path)
        .with_context(|| format!("opening {}", cfg_path.display()))?;
    f.write_all(block.as_bytes())
        .with_context(|| format!("appending project to {}", cfg_path.display()))?;
    Ok(())
}

pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}
