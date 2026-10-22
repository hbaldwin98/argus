//! The grammars installed, kept in a folder of the instance's config
//! directory so a dev instance has its own and a grammar is downloaded once
//! per instance: `<name>.wasm`, `<name>.scm` and a `<name>.json` that says
//! what it is and where it came from. Storage that will not write leaves a
//! grammar to last only as long as the window, as the page's store did.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::fetch::language_of;
use super::known::known_for;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub label: String,
    /// Where it came from, `package@version`, for the person to read.
    pub source: String,
    /// What the parser exports, `tree_sitter_<symbol>`.
    pub symbol: String,
    pub wasm: Vec<u8>,
    pub query: String,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    label: String,
    source: String,
    /// Absent from what an earlier build kept, where the name was the symbol.
    #[serde(default)]
    symbol: Option<String>,
}

/// What a grammar is kept under, for the file that wants one: the pinned
/// grammar's name, or the language npm names it by. Store and lookup both go
/// through this, so what a search installed for `Dockerfile` is found again
/// for the next one (EDITOR-GRAM-12, which the page gets wrong).
pub fn key_for(path: &str) -> Option<String> {
    known_for(path).map(|k| k.name.to_owned()).or_else(|| language_of(path))
}

/// A name is a file name here, so it is only ever lower-case letters, digits,
/// `_` and `-`, and short.
fn safe(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

pub struct Store {
    dir: PathBuf,
    /// Kept too, so a grammar whose write failed still serves the window.
    memory: Mutex<HashMap<String, Installed>>,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Store { dir: dir.into(), memory: Mutex::new(HashMap::new()) }
    }

    /// Under this instance's config directory.
    pub fn in_config() -> Self {
        Store::new(argus_protocol::config_dir().join("grammars"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, name: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{name}.{ext}"))
    }

    pub fn installed(&self, name: &str) -> Option<Installed> {
        if !safe(name) {
            return None;
        }
        if let Some(kept) = self.memory.lock().ok()?.get(name) {
            return Some(kept.clone());
        }
        // All three, or none: the metadata is written last, so a grammar
        // that was stopped half way is not read.
        let meta: Meta = serde_json::from_slice(&std::fs::read(self.file(name, "json")).ok()?).ok()?;
        let wasm = std::fs::read(self.file(name, "wasm")).ok()?;
        let query = std::fs::read_to_string(self.file(name, "scm")).ok()?;
        let symbol = meta.symbol.unwrap_or_else(|| name.to_owned());
        Some(Installed { name: name.to_owned(), label: meta.label, source: meta.source, symbol, wasm, query })
    }

    /// Keeps it for the window, and writes it for the next one. The files are
    /// written beside their final names and renamed into them, the metadata last.
    pub fn keep(&self, grammar: &Installed) -> io::Result<()> {
        if !safe(&grammar.name) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{:?} is not a name a grammar is kept under", grammar.name)));
        }
        if let Ok(mut memory) = self.memory.lock() {
            memory.insert(grammar.name.clone(), grammar.clone());
        }
        std::fs::create_dir_all(&self.dir)?;
        let meta = serde_json::to_vec(&Meta {
            label: grammar.label.clone(),
            source: grammar.source.clone(),
            symbol: Some(grammar.symbol.clone()),
        })?;
        self.write(&grammar.name, "wasm", &grammar.wasm)?;
        self.write(&grammar.name, "scm", grammar.query.as_bytes())?;
        self.write(&grammar.name, "json", &meta)
    }

    fn write(&self, name: &str, ext: &str, bytes: &[u8]) -> io::Result<()> {
        let part = self.file(name, &format!("{ext}.part"));
        std::fs::write(&part, bytes)?;
        std::fs::rename(&part, self.file(name, ext))
    }
}

#[cfg(test)]
mod tests;
