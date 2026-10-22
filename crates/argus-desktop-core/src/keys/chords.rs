//! How a key is written: the shortcuts sheet, the palette and a menu's key hint say each chord of
//! the keymap, and of an extension, the same way (the page's `keys/chords.ts`).
//!
//! A keymap chord has a `'static` key and an extension's an owned one, so a [`KeyChord`] is the
//! one shape both are written from.

use super::keymap::{Action, Chord, Group, Place};
use crate::extensions::keys::ExtChord;

/// One key as it is written, whichever table it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    /// Pressed after the leader.
    pub leader: bool,
}

impl From<&Chord> for KeyChord {
    fn from(chord: &Chord) -> Self {
        Self { key: chord.key.to_owned(), ctrl: chord.ctrl, alt: chord.alt, shift: chord.shift, meta: chord.meta, leader: chord.leader }
    }
}

impl From<&ExtChord> for KeyChord {
    fn from(chord: &ExtChord) -> Self {
        Self { key: chord.key.clone(), ctrl: chord.ctrl, alt: chord.alt, shift: chord.shift, meta: chord.meta, leader: chord.leader }
    }
}

/// One press, its modifiers first: `Ctrl K`, `Shift PgUp`, `j`.
pub fn press(chord: &KeyChord) -> String {
    let name = match chord.key.as_str() {
        " " => "Space",
        "ArrowDown" => "\u{2193}",
        "ArrowUp" => "\u{2191}",
        "ArrowLeft" => "\u{2190}",
        "ArrowRight" => "\u{2192}",
        "Escape" => "Esc",
        "PageUp" => "PgUp",
        "PageDown" => "PgDn",
        other => other,
    };
    let mac = cfg!(target_os = "macos");
    let modifiers: Vec<&str> = [
        chord.ctrl.then_some("Ctrl"),
        chord.alt.then_some(if mac { "\u{2325}" } else { "Alt" }),
        chord.shift.then_some("Shift"),
        chord.meta.then_some(if mac { "\u{2318}" } else { "Meta" }),
    ]
    .into_iter()
    .flatten()
    .collect();
    // A letter held with a modifier is written as on the key; alone it is the character typed.
    let key = if !modifiers.is_empty() && name.chars().count() == 1 { name.to_uppercase() } else { name.to_owned() };
    modifiers.iter().map(|m| (*m).to_owned()).chain([key]).collect::<Vec<_>>().join(" ")
}

/// The leader, as the pane's binding that sends it through holds it.
pub fn leader(keymap: &[Group]) -> String {
    keymap
        .iter()
        .find(|group| group.place == Place::Pane)
        .and_then(|group| group.bindings.iter().find(|binding| binding.action == Action::Leader))
        .and_then(|binding| binding.chords.first())
        .map_or_else(|| "Ctrl Space".to_owned(), |chord| press(&KeyChord { leader: false, ..KeyChord::from(chord) }))
}

/// The presses of one chord: the leader's first when it follows the leader.
pub fn presses_of(chord: &KeyChord, leader: &str) -> Vec<String> {
    if chord.leader {
        vec![leader.to_owned(), press(chord)]
    } else {
        vec![press(chord)]
    }
}

/// A label in the interface's one case: the keymap writes them lowercase.
pub fn sentence(label: &str) -> String {
    let mut chars = label.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::keymap::groups;

    fn chord(key: &str) -> KeyChord {
        KeyChord { key: key.into(), ctrl: false, alt: false, shift: false, meta: false, leader: false }
    }

    #[test]
    fn a_letter_with_a_modifier_is_written_as_on_the_key_and_alone_as_typed() {
        assert_eq!(press(&KeyChord { ctrl: true, ..chord("k") }), "Ctrl K");
        assert_eq!(press(&chord("k")), "k");
        assert_eq!(press(&KeyChord { shift: true, ..chord("PageUp") }), "Shift PgUp");
    }

    #[test]
    fn named_keys_are_written_by_their_names() {
        for (key, written) in [(" ", "Space"), ("Escape", "Esc"), ("ArrowLeft", "\u{2190}"), ("PageDown", "PgDn")] {
            assert_eq!(press(&chord(key)), written);
        }
    }

    #[test]
    fn the_leader_is_the_panes_binding_that_sends_it_through() {
        assert_eq!(leader(&groups()), "Ctrl Space");
        assert_eq!(leader(&[]), "Ctrl Space", "the keymap not heard yet");
    }

    #[test]
    fn a_chord_after_the_leader_is_two_presses() {
        let after = KeyChord { leader: true, ..chord("z") };
        assert_eq!(presses_of(&after, "Ctrl Space"), ["Ctrl Space", "z"]);
        assert_eq!(presses_of(&chord("z"), "Ctrl Space"), ["z"]);
    }

    #[test]
    fn an_extensions_chord_is_written_as_the_keymaps_is() {
        let ext = ExtChord { key: "p".into(), ctrl: true, alt: false, shift: false, meta: false, leader: false };
        assert_eq!(press(&KeyChord::from(&ext)), "Ctrl P");
    }

    #[test]
    fn a_label_begins_with_a_capital_and_an_empty_one_stays_empty() {
        assert_eq!(sentence("go to the workspace"), "Go to the workspace");
        assert_eq!(sentence(""), "");
        assert_eq!(sentence("\u{e9}tat"), "\u{c9}tat");
    }
}
