use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    #[serde(default, rename = "project")]
    pub projects: Vec<ProjectConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub repos: Vec<String>,
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

pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}
