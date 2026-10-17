//! Which font families this machine has installed, by every name the
//! webview might know each by, and which are monospaced, for the Settings
//! dialog to offer.

use std::collections::BTreeMap;

use ttf_parser::name_id::{FAMILY, TYPOGRAPHIC_FAMILY};
use ttf_parser::Language;

use crate::connection::page::FontFamily;

/// Read from every font file on the machine, so slow enough to be done
/// once, off the window's thread.
pub fn installed() -> Vec<FontFamily> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let mut faces = Vec::new();
    for face in db.faces() {
        db.with_face_data(face.id, |data, index| {
            if let Ok(parsed) = ttf_parser::Face::parse(data, index) {
                faces.push((names(&parsed), parsed.is_monospaced()));
            }
        });
    }
    families(faces)
}

/// A face's family as both its legacy and its typographic name: Windows
/// knows a Nerd Font as `JetBrainsMono NFM`, fontconfig as `JetBrainsMono
/// Nerd Font Mono`, and the page keeps whichever its webview draws.
fn names(face: &ttf_parser::Face) -> Vec<String> {
    // Only a Unicode record decodes; a Mac Roman copy of the same name is
    // skipped rather than chosen and lost.
    let english = |id: u16| {
        let named = || {
            face.names()
                .into_iter()
                .filter(move |n| n.name_id == id)
                .filter_map(|n| Some((n.language(), n.to_string()?)))
        };
        named()
            .find(|(language, _)| *language == Language::English_UnitedStates)
            .or_else(|| named().next())
            .map(|(_, name)| name)
    };
    [FAMILY, TYPOGRAPHIC_FAMILY].into_iter().filter_map(english).collect()
}

/// The families, once each, in the order a person reads them, each
/// monospaced if any of its faces is. A name starting with a dot is
/// macOS's own, hidden from every picker; one with a quote could not be
/// chosen, since the setting strips it.
fn families(faces: Vec<(Vec<String>, bool)>) -> Vec<FontFamily> {
    let mut unique: BTreeMap<String, FontFamily> = BTreeMap::new();
    for (names, mono) in faces {
        for name in names {
            let name = name.trim();
            if name.is_empty() || name.starts_with('.') || name.contains(['"', '\'', '\\', ';', '{', '}']) {
                continue;
            }
            let family = unique
                .entry(name.to_lowercase())
                .or_insert_with(|| FontFamily { name: name.to_string(), mono: false });
            family.mono |= mono;
        }
    }
    unique.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(names: &[&str], mono: bool) -> (Vec<String>, bool) {
        (names.iter().map(|n| n.to_string()).collect(), mono)
    }

    fn listed(found: Vec<FontFamily>) -> Vec<(String, bool)> {
        found.into_iter().map(|f| (f.name, f.mono)).collect()
    }

    #[test]
    fn offers_each_family_once_by_every_name_marking_the_monospaced() {
        let found = families(vec![
            face(&["JetBrainsMono NFM", "JetBrainsMono Nerd Font Mono"], true),
            face(&["JetBrainsMono NFM", "JetBrainsMono Nerd Font Mono"], true),
            face(&["Cascadia Mono"], true),
            face(&["cascadia mono"], true),
            face(&["Segoe UI"], false),
        ]);
        assert_eq!(
            listed(found),
            [
                ("Cascadia Mono".to_string(), true),
                ("JetBrainsMono Nerd Font Mono".to_string(), true),
                ("JetBrainsMono NFM".to_string(), true),
                ("Segoe UI".to_string(), false),
            ]
        );
    }

    #[test]
    fn a_family_is_monospaced_when_any_of_its_faces_is() {
        let found = families(vec![face(&["Mixed"], false), face(&["Mixed"], true)]);
        assert_eq!(listed(found), [("Mixed".to_string(), true)]);
    }

    #[test]
    fn leaves_out_hidden_and_unchoosable_names() {
        let found = families(vec![face(&[".SF NS Mono", "Odd\"Name", "  ", "Menlo"], true)]);
        assert_eq!(listed(found), [("Menlo".to_string(), true)]);
    }
}
