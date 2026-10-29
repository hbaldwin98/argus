//! The speech models dictation offers, as a person chooses between them: what each is called, what it
//! hears, and the languages a multilingual one can be told to listen for.
//!
//! The files behind them (repository, size, hash, stream shape) are `argus-dictation`'s catalog, which
//! this crate cannot depend on; each model's id is the catalog's id for it, and `argus-gpui` holds the
//! two lists to each other.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Model {
    /// English only, and the default: learn-harness's owner's everyday model.
    #[default]
    #[serde(rename = "parakeet-unified-en-0.6b")]
    Parakeet,
    #[serde(rename = "nemotron-3.5-asr-streaming-0.6b")]
    Nemotron,
}

pub const MODELS: [Model; 2] = [Model::Parakeet, Model::Nemotron];

impl Model {
    /// The id the catalog and the preferences file keep it by.
    pub fn id(self) -> &'static str {
        match self {
            Model::Parakeet => "parakeet-unified-en-0.6b",
            Model::Nemotron => "nemotron-3.5-asr-streaming-0.6b",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Model::Parakeet => "Parakeet Unified EN 0.6B",
            Model::Nemotron => "Nemotron 3.5 ASR Streaming 0.6B",
        }
    }

    /// What the chip and the panel call it.
    pub fn short(self) -> &'static str {
        match self {
            Model::Parakeet => "Parakeet",
            Model::Nemotron => "Nemotron",
        }
    }

    /// The line under its name, before its size.
    pub fn hears(self) -> &'static str {
        match self {
            Model::Parakeet => "English · streaming",
            Model::Nemotron => "32 languages · streaming",
        }
    }

    /// Whether it can be told which language to listen for.
    pub fn multilingual(self) -> bool {
        self == Model::Nemotron
    }
}

/// What the language setting holds when the model is to hear which language is spoken: Argus's own
/// word, since transcribe.cpp rejects the literal "auto" and is given no language instead.
pub const AUTO: &str = "auto";

/// Nemotron 3.5's 32 locales (its catalog entry), in the order learn-harness lists them.
pub const LOCALES: [(&str, &str); 32] = [
    ("en-US", "English (US)"),
    ("en-GB", "English (UK)"),
    ("es-US", "Spanish (US)"),
    ("es-ES", "Spanish (Spain)"),
    ("fr-FR", "French (France)"),
    ("fr-CA", "French (Canada)"),
    ("it-IT", "Italian"),
    ("pt-BR", "Portuguese (Brazil)"),
    ("pt-PT", "Portuguese (Portugal)"),
    ("nl-NL", "Dutch"),
    ("de-DE", "German"),
    ("tr-TR", "Turkish"),
    ("ru-RU", "Russian"),
    ("ar-AR", "Arabic"),
    ("hi-IN", "Hindi"),
    ("ja-JP", "Japanese"),
    ("ko-KR", "Korean"),
    ("vi-VN", "Vietnamese"),
    ("uk-UA", "Ukrainian"),
    ("pl-PL", "Polish"),
    ("sv-SE", "Swedish"),
    ("cs-CZ", "Czech"),
    ("nb-NO", "Norwegian (Bokmål)"),
    ("da-DK", "Danish"),
    ("bg-BG", "Bulgarian"),
    ("fi-FI", "Finnish"),
    ("hr-HR", "Croatian"),
    ("sk-SK", "Slovak"),
    ("zh-CN", "Chinese (Mandarin, Simplified)"),
    ("hu-HU", "Hungarian"),
    ("ro-RO", "Romanian"),
    ("et-EE", "Estonian"),
];

/// Whether `code` is a language the setting can hold.
pub fn is_language(code: &str) -> bool {
    code == AUTO || LOCALES.iter().any(|(locale, _)| *locale == code)
}

/// The language menu: Auto-detect first, then the locales by name.
pub fn languages() -> Vec<(&'static str, &'static str)> {
    let mut named: Vec<(&str, &str)> = LOCALES.to_vec();
    named.sort_by(|a, b| a.1.cmp(b.1));
    std::iter::once((AUTO, "Auto-detect")).chain(named).collect()
}

/// What a run listens for with `model` and the language chosen: none for Nemotron's auto, the chosen
/// locale for Nemotron, and English for Parakeet, which hears nothing else.
pub fn listens_for(model: Model, chosen: &str) -> Option<&'static str> {
    if !model.multilingual() {
        return Some("en");
    }
    LOCALES.iter().find(|(locale, _)| *locale == chosen).map(|(locale, _)| *locale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_language_menu_is_auto_then_all_thirty_two_by_name() {
        let menu = languages();
        assert_eq!(menu.len(), 33);
        assert_eq!(menu[0], (AUTO, "Auto-detect"));
        let names: Vec<&str> = menu[1..].iter().map(|(_, name)| *name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }

    #[test]
    fn parakeet_listens_for_english_and_nemotron_for_what_was_chosen_or_whatever_it_hears() {
        assert_eq!(listens_for(Model::Parakeet, "de-DE"), Some("en"));
        assert_eq!(listens_for(Model::Nemotron, "de-DE"), Some("de-DE"));
        assert_eq!(listens_for(Model::Nemotron, AUTO), None, "auto is no language at all");
        assert!(is_language(AUTO) && is_language("pt-BR") && !is_language("auto-detect") && !is_language("pt"));
    }

    #[test]
    fn a_model_is_kept_by_the_catalogs_id() {
        for model in MODELS {
            assert_eq!(serde_json::to_value(model).unwrap(), model.id());
        }
    }
}
