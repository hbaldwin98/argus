//! Which font families to offer in the Settings dialog, from the faces a
//! machine has: each family once, by every name the webview might know it
//! by, marked monospaced if any of its faces is. Reading the faces is the
//! window's (`installed_fonts` in the binary that draws).

use std::collections::BTreeMap;

use crate::connection::page::FontFamily;

/// The families, once each, in the order a person reads them, each
/// monospaced if any of its faces is. A name starting with a dot is
/// macOS's own, hidden from every picker; one with a quote could not be
/// chosen, since the setting strips it.
pub fn families(faces: Vec<(Vec<String>, bool)>) -> Vec<FontFamily> {
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
