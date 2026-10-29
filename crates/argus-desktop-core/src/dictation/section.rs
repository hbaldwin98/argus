//! What the Settings sheet's Dictation section shows: the choices, each model's file and what can
//! be done with it, the engine's state in one line, and the menus' entries.
//!
//! The rows are learn-harness's `dictation-settings.tsx`, with the three a native window needs
//! added: the microphone, the two keys and Enter after the paste.

use super::machine::{megabytes, Engine, FileState, Machine};
use super::model::{languages, MODELS};
use super::panel::Tone;
use super::preferences::DictationPreferences;

/// What a model's row offers to do with its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Download,
    Resume,
    Cancel,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRow {
    pub model: super::model::Model,
    pub name: String,
    /// What it hears, how big it is, and how far its download has got.
    pub line: String,
    pub chosen: bool,
    /// How much of it is on disk, in thousandths, while it downloads or after one stopped.
    pub progress: Option<u16>,
    pub act: Option<Act>,
    /// Why its last download failed.
    pub failed: Option<String>,
}

/// One entry of a menu: the value it chooses, and what it is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<T> {
    pub value: T,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SectionPage {
    pub chosen: DictationPreferences,
    pub models: Vec<ModelRow>,
    pub status: (Tone, String),
    pub languages: Vec<Entry<String>>,
    /// The language is the chosen model's to hear: only Nemotron's is.
    pub language_applies: bool,
    /// The system's default first, as `None`.
    pub microphones: Vec<Entry<Option<String>>>,
    /// The two keys as written: `Ctrl Shift Space`, `Ctrl Space · V`.
    pub hold: String,
    pub toggle: String,
}

/// How far `done` of `total` is, in thousandths.
fn thousandths(done: u64, total: u64) -> u16 {
    if total == 0 {
        return 0;
    }
    (done.min(total) * 1000 / total) as u16
}

/// The line under the section's model cards: whether the engine is up, on what, and what happens next.
fn status(chosen: &DictationPreferences, machine: &Machine) -> (Tone, String) {
    if !chosen.enabled {
        return (Tone::Plain, "Off: the keys go to the pane's program as before".into());
    }
    match &machine.engine {
        Engine::Ready { model, backend, device, cpu_fallback } if *model == chosen.model => {
            if *cpu_fallback {
                (Tone::Warn, format!("Ready on the CPU, {device}: the GPU would not load \u{b7} unloads after 10 idle minutes"))
            } else {
                (Tone::Ok, format!("Ready on {device} ({backend}) \u{b7} unloads after 10 idle minutes"))
            }
        }
        Engine::Loading { model } => (Tone::Plain, format!("Loading {}\u{2026}", model.short())),
        Engine::Failed { reason } => (Tone::Bad, reason.clone()),
        _ if !machine.ready(chosen.model) => (Tone::Warn, format!("Download {} to dictate", chosen.model.short())),
        _ => (Tone::Plain, "Loads when you first dictate, in a second or three \u{b7} unloads after 10 idle minutes".into()),
    }
}

pub fn section(chosen: &DictationPreferences, machine: &Machine, hold: String, toggle: String) -> SectionPage {
    let models = MODELS
        .iter()
        .map(|&model| {
            let file = machine.file(model);
            let size = file.map_or_else(String::new, |file| format!(" \u{b7} {}", megabytes(file.size)));
            let state = file.map_or(FileState::Missing, |file| file.state.clone());
            let (progress, act, failed, how_far) = match &state {
                FileState::Missing => (None, Act::Download, None, String::new()),
                FileState::Downloading { done, total } => {
                    let at = thousandths(*done, *total);
                    (Some(at), Act::Cancel, None, format!(" \u{b7} {}%", at / 10))
                }
                FileState::Partial { done, total } => {
                    let at = thousandths(*done, *total);
                    (Some(at), Act::Resume, None, format!(" \u{b7} stopped at {}%", at / 10))
                }
                FileState::Ready => (None, Act::Delete, None, String::new()),
                FileState::Failed { reason } => (None, Act::Download, Some(reason.clone()), String::new()),
            };
            ModelRow {
                model,
                name: model.label().into(),
                line: format!("{}{size}{how_far}", model.hears()),
                chosen: model == chosen.model,
                progress,
                act: Some(act),
                failed,
            }
        })
        .collect();
    let default = machine.default_microphone.as_deref().map_or_else(|| "System default".to_owned(), |name| format!("System default ({name})"));
    let mut microphones = vec![Entry { value: None, label: default }];
    microphones.extend(machine.microphones.iter().map(|mic| Entry { value: Some(mic.id.clone()), label: mic.name.clone() }));
    // A microphone chosen and since unplugged is still named, so the menu says what is chosen.
    if let Some(id) = &chosen.microphone {
        if !machine.microphones.iter().any(|mic| &mic.id == id) {
            microphones.push(Entry { value: Some(id.clone()), label: "A microphone not plugged in".into() });
        }
    }
    SectionPage {
        chosen: chosen.clone(),
        models,
        status: status(chosen, machine),
        languages: languages().into_iter().map(|(code, name)| Entry { value: code.to_owned(), label: name.to_owned() }).collect(),
        language_applies: chosen.model.multilingual(),
        microphones,
        hold,
        toggle,
    }
}
