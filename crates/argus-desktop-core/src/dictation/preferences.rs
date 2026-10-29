//! What the Settings sheet's Dictation section sets, kept in `desktop.json` under `dictation`, and
//! each change to it held to what the window can do.
//!
//! Like the rest of the window's preferences a change takes effect as it is made and touches no
//! daemon. A hand-edited file is read field by field, so one bad value costs only itself, never the
//! whole file (learn-harness's settings sanitizer drops bad fields one by one the same way).

use serde::{Deserialize, Serialize};

use super::hold::Hold;
use super::model::{is_language, Model, AUTO};

/// Where the model runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Device {
    /// The GPU when there is one, else the CPU.
    #[default]
    Auto,
    Gpu,
    Cpu,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationPreferences {
    pub enabled: bool,
    pub model: Model,
    /// A Nemotron locale, or `auto`; Parakeet hears English whatever this says.
    pub language: String,
    pub device: Device,
    /// The microphone's id; none follows the system's default as it changes.
    pub microphone: Option<String>,
    /// The Silero gate: silence never reaches the model.
    pub vad: bool,
    pub remove_fillers: bool,
    pub trailing_space: bool,
    /// Sends Enter after the paste, so the prompt goes as soon as the key is let go.
    pub enter_after: bool,
    pub hold: Hold,
}

/// The design page's defaults: on, Parakeet, fillers removed, no trailing space, no Enter.
impl Default for DictationPreferences {
    fn default() -> Self {
        DictationPreferences {
            enabled: true,
            model: Model::default(),
            language: AUTO.into(),
            device: Device::default(),
            microphone: None,
            vad: true,
            remove_fillers: true,
            trailing_space: false,
            enter_after: false,
            hold: Hold::default(),
        }
    }
}

/// One change the section makes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", tag = "set")]
pub enum DictationChange {
    Enabled { on: bool },
    Model { model: Model },
    Language { language: String },
    Device { device: Device },
    /// None is the system's default.
    Microphone { id: Option<String> },
    Vad { on: bool },
    RemoveFillers { on: bool },
    TrailingSpace { on: bool },
    EnterAfter { on: bool },
    Hold { hold: Hold },
}

impl DictationPreferences {
    /// Makes `change`, or says why it cannot be made and leaves them as they were.
    pub fn change(&mut self, change: DictationChange) -> Result<(), String> {
        match change {
            DictationChange::Enabled { on } => self.enabled = on,
            DictationChange::Model { model } => self.model = model,
            DictationChange::Language { language } if is_language(&language) => self.language = language,
            DictationChange::Language { language } => return Err(format!("{language} is not a language dictation hears")),
            DictationChange::Device { device } => self.device = device,
            DictationChange::Microphone { id } => self.microphone = id.filter(|id| !id.trim().is_empty()),
            DictationChange::Vad { on } => self.vad = on,
            DictationChange::RemoveFillers { on } => self.remove_fillers = on,
            DictationChange::TrailingSpace { on } => self.trailing_space = on,
            DictationChange::EnterAfter { on } => self.enter_after = on,
            DictationChange::Hold { hold } => match hold.refusal() {
                Some(why) => return Err(why),
                None => self.hold = hold,
            },
        }
        Ok(())
    }

    /// The preferences a file's `dictation` value says, each field read on its own: one missing or
    /// not understood keeps its default, and a language or chord the window cannot use is let go.
    pub fn read(value: &serde_json::Value) -> Self {
        let mut read = DictationPreferences::default();
        let Some(fields) = value.as_object() else { return read };
        fn field<T: serde::de::DeserializeOwned>(fields: &serde_json::Map<String, serde_json::Value>, name: &str, into: &mut T) {
            if let Some(value) = fields.get(name).and_then(|value| serde_json::from_value(value.clone()).ok()) {
                *into = value;
            }
        }
        field(fields, "enabled", &mut read.enabled);
        field(fields, "model", &mut read.model);
        field(fields, "language", &mut read.language);
        field(fields, "device", &mut read.device);
        field(fields, "microphone", &mut read.microphone);
        field(fields, "vad", &mut read.vad);
        field(fields, "removeFillers", &mut read.remove_fillers);
        field(fields, "trailingSpace", &mut read.trailing_space);
        field(fields, "enterAfter", &mut read.enter_after);
        field(fields, "hold", &mut read.hold);
        if !is_language(&read.language) {
            read.language = AUTO.into();
        }
        if read.hold.refusal().is_some() {
            read.hold = Hold::default();
        }
        read.microphone = read.microphone.filter(|id| !id.trim().is_empty());
        read
    }
}

/// `Settings.dictation`'s reader: see [`DictationPreferences::read`].
pub fn lenient<'de, D: serde::Deserializer<'de>>(from: D) -> Result<DictationPreferences, D::Error> {
    let value = serde_json::Value::deserialize(from)?;
    Ok(DictationPreferences::read(&value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_defaults_are_parakeet_with_fillers_removed_and_nothing_sent_after() {
        let p = DictationPreferences::default();
        assert!(p.enabled && p.vad && p.remove_fillers);
        assert!(!p.trailing_space && !p.enter_after);
        assert_eq!((p.model, p.language.as_str(), p.device, p.microphone), (Model::Parakeet, "auto", Device::Auto, None));
        assert_eq!(p.hold.written(), "Ctrl Shift Space");
    }

    #[test]
    fn preferences_written_are_the_preferences_read() {
        let p = DictationPreferences {
            enabled: false,
            model: Model::Nemotron,
            language: "de-DE".into(),
            device: Device::Gpu,
            microphone: Some("wasapi:{0.0.1.00000000}.{abc}".into()),
            vad: false,
            remove_fillers: false,
            trailing_space: true,
            enter_after: true,
            hold: Hold { key: "F9".into(), ctrl: false, alt: true, shift: false, meta: false },
        };
        let value = serde_json::to_value(&p).unwrap();
        assert_eq!(value["model"], "nemotron-3.5-asr-streaming-0.6b");
        assert_eq!(value["removeFillers"], false);
        assert_eq!(DictationPreferences::read(&value), p);
    }

    #[test]
    fn a_hand_edited_file_keeps_its_good_fields_and_drops_the_bad_ones_one_by_one() {
        let read = DictationPreferences::read(&json!({
            "enabled": "yes", "model": "whisper", "language": "de-DE", "device": "tpu",
            "vad": false, "trailingSpace": true, "hold": { "key": " ", "ctrl": true }, "microphone": "  ",
        }));
        assert!(read.enabled, "a word is not a switch: the default");
        assert_eq!(read.model, Model::Parakeet, "an unknown model: the default");
        assert_eq!(read.language, "de-DE");
        assert_eq!(read.device, Device::Auto);
        assert!(!read.vad && read.trailing_space);
        assert_eq!(read.hold, Hold::default(), "the leader cannot be the chord");
        assert_eq!(read.microphone, None, "blank is the system's default");
        assert_eq!(DictationPreferences::read(&json!({ "language": "klingon" })).language, "auto");
        assert_eq!(DictationPreferences::read(&json!("on")), DictationPreferences::default());
    }

    #[test]
    fn a_change_is_held_to_what_the_window_can_do() {
        let mut p = DictationPreferences::default();
        assert!(p.change(DictationChange::Language { language: "auto-detect".into() }).is_err());
        assert_eq!(p.language, "auto", "a refusal changes nothing");
        p.change(DictationChange::Language { language: "fr-CA".into() }).unwrap();
        assert_eq!(p.language, "fr-CA");
        let leader = Hold { shift: false, ..Hold::default() };
        assert!(p.change(DictationChange::Hold { hold: leader }).is_err());
        assert_eq!(p.hold, Hold::default());
        p.change(DictationChange::Microphone { id: Some(String::new()) }).unwrap();
        assert_eq!(p.microphone, None);
        p.change(DictationChange::EnterAfter { on: true }).unwrap();
        assert!(p.enter_after);
    }

    #[test]
    fn a_change_is_read_as_the_section_sends_it() {
        let change: DictationChange = serde_json::from_value(json!({ "set": "device", "device": "cpu" })).unwrap();
        assert_eq!(change, DictationChange::Device { device: Device::Cpu });
        let change: DictationChange = serde_json::from_value(json!({ "set": "removeFillers", "on": false })).unwrap();
        assert_eq!(change, DictationChange::RemoveFillers { on: false });
    }
}
