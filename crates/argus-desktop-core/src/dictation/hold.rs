//! The push-to-talk chord: the one key the window keeps from a pane's program while it is held, as
//! it keeps the leader, so it can hear the key let go.
//!
//! Set in the Settings sheet, so it is checked there: it needs a modifier a person would not type
//! text with, and it may not be a key the window already answers, the leader among them, or the
//! window would hear one press two ways.

use serde::{Deserialize, Serialize};

use crate::keys::chords::{press, KeyChord};
use crate::keys::keymap::{Place, BINDINGS, LEADER};
use crate::keys::page_key::PageKey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hold {
    /// `KeyboardEvent.key`, as a page key names it: `" "` for the space bar.
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub meta: bool,
}

/// Handy's default, Ctrl Space, is Argus's leader, so the design page took the next one along.
impl Default for Hold {
    fn default() -> Self {
        Hold { key: " ".into(), ctrl: true, alt: false, shift: true, meta: false }
    }
}

/// The keys that are only modifiers: none of them is a chord on its own.
const MODIFIERS: [&str; 6] = ["Control", "Shift", "Alt", "Meta", "AltGraph", "CapsLock"];

impl Hold {
    /// Whether `page` is this chord pressed: every modifier as the chord has it, and the key, a letter
    /// in either case since shift is part of the chord here, not of the character.
    pub fn matches(&self, page: &PageKey) -> bool {
        page.key.eq_ignore_ascii_case(&self.key) && page.ctrl == self.ctrl && page.alt == self.alt && page.shift == self.shift && page.meta == self.meta
    }

    /// How the Settings sheet and the panel write it: `Ctrl Shift Space`.
    pub fn written(&self) -> String {
        press(&KeyChord { key: self.key.clone(), ctrl: self.ctrl, alt: self.alt, shift: self.shift, meta: self.meta, leader: false })
    }

    /// Why it cannot be the chord, if it cannot.
    pub fn refusal(&self) -> Option<String> {
        if self.key.is_empty() || MODIFIERS.contains(&self.key.as_str()) {
            return Some("Hold a key with the modifiers, not the modifiers alone".into());
        }
        if !(self.ctrl || self.alt || self.meta) {
            return Some("Push-to-talk needs Ctrl, Alt or the command key, or it would type".into());
        }
        let page = PageKey { key: self.key.clone(), code: String::new(), ctrl: self.ctrl, alt: self.alt, shift: self.shift, meta: self.meta };
        if LEADER.matches(&page) && !self.shift {
            return Some(format!("{} is the leader", self.written()));
        }
        let taken = BINDINGS.iter().find(|binding| {
            matches!(binding.place, Place::Window | Place::Anywhere | Place::Pane) && binding.chords.iter().any(|chord| !chord.leader && chord.matches(&page) && chord.shift == self.shift)
        });
        taken.map(|binding| format!("{} is already the window's key to {}", self.written(), binding.label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str, ctrl: bool, shift: bool) -> PageKey {
        PageKey { key: key.into(), ctrl, shift, ..PageKey::default() }
    }

    #[test]
    fn the_default_is_ctrl_shift_space_and_the_leader_is_not_it() {
        let hold = Hold::default();
        assert_eq!(hold.written(), "Ctrl Shift Space");
        assert!(hold.matches(&key(" ", true, true)));
        assert!(!hold.matches(&key(" ", true, false)), "Ctrl Space is the leader");
        assert!(!hold.matches(&key(" ", false, true)));
        assert_eq!(hold.refusal(), None);
    }

    #[test]
    fn a_letter_is_matched_whichever_case_shift_gives_it() {
        let hold = Hold { key: "d".into(), ctrl: true, alt: true, shift: true, meta: false };
        assert!(hold.matches(&PageKey { key: "D".into(), ctrl: true, alt: true, shift: true, ..PageKey::default() }));
    }

    #[test]
    fn a_chord_that_would_type_or_that_the_window_already_answers_is_refused() {
        let leader = Hold { shift: false, ..Hold::default() };
        assert_eq!(leader.refusal().as_deref(), Some("Ctrl Space is the leader"));
        assert!(Hold { key: "k".into(), ctrl: true, shift: false, ..Hold::default() }.refusal().is_some(), "the palette");
        // Paste and zoom are on the command key on a Mac and on Ctrl elsewhere (`keymap::PASTE`).
        let primary = |key: &str| Hold { key: key.into(), ctrl: !cfg!(target_os = "macos"), shift: false, meta: cfg!(target_os = "macos"), ..Hold::default() };
        assert!(primary("v").refusal().is_some(), "paste");
        assert!(primary("=").refusal().is_some(), "zoom");
        assert!(Hold { key: " ".into(), ctrl: false, shift: true, ..Hold::default() }.refusal().is_some(), "shift alone types");
        assert!(Hold { key: "Shift".into(), ..Hold::default() }.refusal().is_some(), "a modifier alone");
        assert_eq!(Hold { key: "F9".into(), ctrl: false, alt: true, shift: false, meta: false }.refusal(), None);
    }
}
