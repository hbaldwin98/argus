//! Which keys an extension may take, and the words a refused one is refused
//! in.
//!
//! An extension's key never shadows a key Argus binds where the extension's
//! would be heard, the leader, or the digits that open the stages, so the
//! shortcuts sheet stays the one truth about what a key does. The daemon
//! decides when the key is registered, and knows no client's keymap, so the
//! keys Argus keeps are listed here; each client's tests fail when it binds
//! one this list does not hold, which is what keeps the list from drifting.

use crate::extensions::ExtKey;

/// A key Argus binds where an extension's command key would be heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kept {
    pub key: &'static str,
    pub ctrl: bool,
    pub alt: bool,
    /// Asked only of a named key, as for [`ExtKey`].
    pub shift: bool,
}

const fn key(key: &'static str) -> Kept {
    Kept { key, ctrl: false, alt: false, shift: false }
}

const fn ctrl(key: &'static str) -> Kept {
    Kept { key, ctrl: true, alt: false, shift: false }
}

const fn alt(key: &'static str) -> Kept {
    Kept { key, ctrl: false, alt: true, shift: false }
}

const fn shift(key: &'static str) -> Kept {
    Kept { key, ctrl: false, alt: false, shift: true }
}

/// Every key the window binds outside a text field, after the leader
/// included: an extension's command is heard in the same places, and
/// through the leader from a pane.
pub const KEPT: &[Kept] = &[
    key("1"), key("2"), key("3"), key("4"), key("`"), key("?"), key("Escape"), key("Enter"), key("Tab"),
    key("/"), key("\\"), key(","), key("["), key("]"),
    key("ArrowUp"), key("ArrowDown"), key("ArrowLeft"), key("ArrowRight"), key("Home"), key("End"),
    key("PageUp"), key("PageDown"), shift("PageUp"), shift("PageDown"),
    key("a"), key("b"), key("c"), key("d"), key("e"), key("f"), key("g"), key("h"), key("j"), key("k"),
    key("l"), key("m"), key("n"), key("o"), key("p"), key("r"), key("s"), key("v"), key("x"), key("y"),
    key("z"),
    key("A"), key("B"), key("C"), key("D"), key("E"), key("F"), key("G"), key("H"), key("M"), key("N"),
    key("O"), key("P"), key("R"), key("S"), key("V"), key("X"), key("Y"),
    ctrl(" "), ctrl("n"), ctrl("k"), ctrl(","), ctrl("p"), ctrl("f"), ctrl("v"), ctrl("V"), alt("v"),
    ctrl("="), ctrl("+"), ctrl("-"), ctrl("0"), ctrl("Enter"), ctrl("."),
    key("+"), key("="), key("-"), key("J"), key("K"), key("L"), key("."), key("u"), key("U"),
];

/// The keys a view's own stage keeps for moving through its rows,
/// filtering them and leaving: an action's key cannot be one of them, nor a
/// key [`KEPT`] holds without a modifier.
pub const STAGE_KEYS: &[&str] = &["j", "k", "h", "l", "g", "G", "q", "/", "?", "`", " "];

impl Kept {
    fn is(&self, press: &ExtKey) -> bool {
        let character = press.is_character();
        self.key == press.key
            && self.ctrl == press.ctrl
            && self.alt == press.alt
            && (character || self.shift == press.shift)
    }
}

/// Whether Argus binds `press` where an extension's command would be
/// heard.
pub fn kept(press: &ExtKey) -> bool {
    KEPT.iter().any(|k| k.is(press))
}

/// Why `press` cannot be a command's key, or `None` when it can. Another
/// extension's key is not checked here: the daemon holds every command and
/// refuses the second of two.
pub fn command_key_refusal(press: &ExtKey) -> Option<String> {
    if press.key.is_empty() {
        return Some("names no key".into());
    }
    if press.ctrl && press.key == " " {
        return Some("Ctrl Space is the leader".into());
    }
    if !press.ctrl && !press.alt && press.key.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("{} opens a stage", press.key));
    }
    kept(press).then(|| format!("{} is Argus's own; the shortcuts sheet says what it does", press.spoken()))
}

/// Why `key` cannot be an action's key in its view, or `None` when it can.
pub fn action_key_refusal(key: &str) -> Option<String> {
    if key.chars().count() != 1 {
        return Some(format!("an action's key is one character, not {key:?}"));
    }
    if key.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("{key} opens a stage"));
    }
    if STAGE_KEYS.contains(&key) {
        return Some(format!("{key} moves through the view, or leaves it"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str) -> ExtKey {
        ExtKey::plain(key)
    }

    #[test]
    fn the_leader_the_digits_and_argus_keys_are_refused() {
        assert!(command_key_refusal(&ExtKey { ctrl: true, ..press(" ") }).unwrap().contains("leader"));
        assert!(command_key_refusal(&press("3")).unwrap().contains("stage"));
        assert!(command_key_refusal(&press("j")).unwrap().contains("Argus's own"));
        assert!(command_key_refusal(&ExtKey { ctrl: true, ..press("k") }).is_some());
        assert!(command_key_refusal(&press("")).is_some());
    }

    #[test]
    fn a_key_argus_leaves_free_is_taken() {
        for free in ["t", "w", "T", "I", "Q"] {
            assert_eq!(command_key_refusal(&press(free)), None, "{free}");
        }
        assert_eq!(command_key_refusal(&ExtKey { ctrl: true, ..press("d") }), None);
        assert_eq!(command_key_refusal(&ExtKey { alt: true, ..press("3") }), None, "a modified digit is free");
        assert_eq!(command_key_refusal(&press("F5")), None);
    }

    #[test]
    fn a_named_keys_shift_makes_another_key() {
        assert!(kept(&ExtKey { shift: true, ..press("PageUp") }));
        assert!(kept(&press("PageUp")));
        assert!(!kept(&ExtKey { shift: true, ..press("Home") }));
    }

    #[test]
    fn an_actions_key_is_one_character_the_view_does_not_keep() {
        assert_eq!(action_key_refusal("r"), None);
        assert_eq!(action_key_refusal("R"), None);
        assert!(action_key_refusal("j").is_some());
        assert!(action_key_refusal("2").is_some());
        assert!(action_key_refusal("ctrl+r").is_some());
        assert!(action_key_refusal("").is_some());
    }
}
