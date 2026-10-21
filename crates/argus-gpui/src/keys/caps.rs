//! How a chord of the session's keymap is written for a menu or a tooltip:
//! the leader's press first when the chord follows it, modifiers before the
//! key, as the page's `keys/chords.ts` writes it.

use argus_desktop_core::keys::keymap::{Action, Chord, Group, Place};

/// One press, its modifiers first: `Ctrl K`, `Shift PgUp`, `j`.
fn press(chord: &Chord) -> String {
    let name = match chord.key {
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

fn chords(keymap: &[Group], action: Action, place: Place) -> &[Chord] {
    keymap
        .iter()
        .find(|group| group.place == place)
        .and_then(|group| group.bindings.iter().find(|binding| binding.action == action))
        .map_or(&[], |binding| binding.chords)
}

/// The leader, as the pane's binding that sends it through holds it.
fn leader(keymap: &[Group]) -> String {
    chords(keymap, Action::Leader, Place::Pane)
        .first()
        .map_or_else(|| "Ctrl Space".to_owned(), |chord| press(&Chord { leader: false, ..*chord }))
}

/// The presses each chord bound to `action` in `place` takes, one list per
/// chord (alternatives): none while the keymap has not been heard.
pub fn presses(keymap: &[Group], action: Action, place: Place) -> Vec<Vec<String>> {
    chords(keymap, action, place)
        .iter()
        .map(|chord| if chord.leader { vec![leader(keymap), press(chord)] } else { vec![press(chord)] })
        .collect()
}

/// `words` with its keys after it, `Hide the console (\`)`, as a title.
pub fn titled(words: &str, keymap: &[Group], action: Action, place: Place) -> String {
    let keys: Vec<String> = presses(keymap, action, place).iter().map(|p| p.join(" ")).collect();
    if keys.is_empty() {
        words.to_owned()
    } else {
        format!("{words} ({})", keys.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::keys::keymap::groups;

    #[test]
    fn a_chord_after_the_leader_is_written_leader_first() {
        let keymap = groups();
        assert_eq!(presses(&keymap, Action::SplitConsole, Place::Pane), [["Ctrl Space", "\\"]]);
        assert_eq!(presses(&keymap, Action::ConsoleFullscreen, Place::Pane), [["Ctrl Space", "z"]]);
    }

    #[test]
    fn a_title_carries_its_keys_and_none_before_the_keymap_is_heard() {
        let keymap = groups();
        assert_eq!(titled("Hide the console", &keymap, Action::ToggleConsole, Place::Anywhere), "Hide the console (`)");
        assert_eq!(titled("Hide the console", &[], Action::ToggleConsole, Place::Anywhere), "Hide the console");
    }
}
