//! What this machine offers dictation, as the window that runs the engine reports it: each model's
//! file, the engine's state and the microphones.
//!
//! Only a window built with the engine reports one, and until it does the session sends no
//! dictation at all, so a window without it (the Tauri page) never hears of it.

use super::model::Model;

/// Where a model's file stands on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FileState {
    #[default]
    Missing,
    Downloading { done: u64, total: u64 },
    /// A download stopped part way; it resumes from `done`.
    Partial { done: u64, total: u64 },
    Ready,
    /// The last download failed: a hash that did not match, the network, the disk.
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    pub model: Model,
    /// Its size in bytes, the catalog's.
    pub size: u64,
    pub state: FileState,
}

/// The engine, which loads one model at a time and unloads it after ten idle minutes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Engine {
    #[default]
    Unloaded,
    Loading { model: Model },
    Ready { model: Model, backend: String, device: String, cpu_fallback: bool },
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Microphone {
    /// What the preferences keep it by.
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Machine {
    pub files: Vec<ModelFile>,
    pub engine: Engine,
    pub microphones: Vec<Microphone>,
    /// The system's default input's name, for the menu's first line.
    pub default_microphone: Option<String>,
}

impl Machine {
    pub fn file(&self, model: Model) -> Option<&ModelFile> {
        self.files.iter().find(|file| file.model == model)
    }

    pub fn ready(&self, model: Model) -> bool {
        self.file(model).is_some_and(|file| file.state == FileState::Ready)
    }
}

/// A size as the Settings sheet and the panel say it: `731 MB`.
pub fn megabytes(bytes: u64) -> String {
    format!("{} MB", (bytes + 500_000) / 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_is_said_in_whole_megabytes_as_the_catalog_rounds_them() {
        assert_eq!(megabytes(731_357_568), "731 MB");
        assert_eq!(megabytes(751_094_240), "751 MB");
        assert_eq!(megabytes(0), "0 MB");
    }
}
