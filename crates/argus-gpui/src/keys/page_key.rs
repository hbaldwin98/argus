//! Which `PageKey` a GPUI keystroke is: the page's `KeyboardEvent.key`,
//! `.code` and modifiers, rebuilt from what GPUI reports, so the session
//! hears the same key from both windows and `argus-desktop-core`'s
//! `keys::page_key::to_key` decides what the pane's child is sent.
//!
//! GPUI names a key by what is printed on it unshifted (`a`, `1`, `up`) and
//! gives the character typed in `key_char`; the DOM names it by what it
//! types (`A`, `!`, `ArrowUp`). The adapter is that translation and nothing
//! else; the keymap, the leader and the paste chord are the session's.

use argus_desktop_core::keys::page_key::PageKey;
use gpui_kit::Keystroke;

/// The page's key for `keystroke`, or `None` for one the window's own
/// handling should go on with: a bare modifier, a key with no name the
/// page has, and text that is more than one character, which arrives as
/// text from the input handler instead.
pub fn from_keystroke(keystroke: &Keystroke) -> Option<PageKey> {
    let mods = &keystroke.modifiers;
    let mut page = PageKey {
        key: String::new(),
        code: String::new(),
        ctrl: mods.control,
        alt: mods.alt,
        shift: mods.shift,
        meta: mods.platform,
    };
    if let Some(name) = named(&keystroke.key) {
        page.key = name.clone();
        page.code = if name == " " { "Space".to_owned() } else { name };
        return Some(page);
    }
    let mut chars = keystroke.key.chars();
    let printed = chars.next().filter(|_| chars.next().is_none())?;
    page.code = physical(printed);
    let typed = keystroke.key_char.as_deref().filter(|text| !text.is_empty());
    page.key = match typed {
        // AltGr is Ctrl and Alt on Windows: the program is sent the
        // character the layout made, not a chord.
        Some(text) if mods.control && mods.alt => {
            page.ctrl = false;
            page.alt = false;
            single(text)?
        }
        // With Ctrl or Alt the character is the key's own, as the DOM has
        // it: Ctrl-Shift-C is `C`, and Alt-F is `f` where a Mac's option
        // layer typed `ƒ` (the session reads the physical key for Alt).
        // A symbol or digit under Shift is the shifted one (`_`, `?`), which
        // is what Ctrl-_ and Ctrl-? are named by.
        _ if (mods.control || mods.alt) && mods.shift && !printed.is_alphabetic() => {
            typed.and_then(single).unwrap_or_else(|| shifted(printed).to_string())
        }
        _ if mods.control || mods.alt => cased(printed, mods.shift),
        Some(text) => single(text)?,
        None => cased(printed, mods.shift),
    };
    Some(page)
}

/// A keystroke's text as one key, or nothing when it is a sequence.
fn single(text: &str) -> Option<String> {
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then(|| first.to_string())
}

/// What Shift types on a US layout, for when the platform reported no text
/// under Ctrl; a key with no shifted form is itself.
fn shifted(printed: char) -> char {
    match printed {
        '1' => '!',
        '2' => '@',
        '3' => '#',
        '4' => '$',
        '5' => '%',
        '6' => '^',
        '7' => '&',
        '8' => '*',
        '9' => '(',
        '0' => ')',
        '-' => '_',
        '=' => '+',
        '[' => '{',
        ']' => '}',
        '\\' => '|',
        ';' => ':',
        '\'' => '"',
        ',' => '<',
        '.' => '>',
        '/' => '?',
        '`' => '~',
        other => other,
    }
}

fn cased(printed: char, shift: bool) -> String {
    if shift {
        printed.to_uppercase().collect()
    } else {
        printed.to_lowercase().collect()
    }
}

/// `KeyboardEvent.code` for a key that is a letter or a digit, else empty.
fn physical(printed: char) -> String {
    match printed {
        'a'..='z' | 'A'..='Z' => format!("Key{}", printed.to_ascii_uppercase()),
        '0'..='9' => format!("Digit{printed}"),
        _ => String::new(),
    }
}

/// The DOM's name for a key GPUI names, or `None` for one that is a
/// character, a bare modifier, or a key the page never sent.
fn named(key: &str) -> Option<String> {
    let name = match key {
        "enter" => "Enter",
        "backspace" => "Backspace",
        "tab" => "Tab",
        "escape" => "Escape",
        "space" => " ",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "insert" => "Insert",
        "delete" => "Delete",
        _ => {
            let n: u8 = key.strip_prefix('f')?.parse().ok()?;
            return (1..=24).contains(&n).then(|| format!("F{n}"));
        }
    };
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::keys::page_key::to_key;
    use argus_protocol::panes::keys::encode;
    use gpui_kit::Modifiers;

    fn stroke(modifiers: Modifiers, key: &str, key_char: Option<&str>) -> Keystroke {
        Keystroke { modifiers, key: key.into(), key_char: key_char.map(Into::into) }
    }

    fn plain(key: &str) -> Keystroke {
        stroke(Modifiers::none(), key, Some(key))
    }

    fn named_key(key: &str) -> Keystroke {
        stroke(Modifiers::none(), key, None)
    }

    /// What the pane's child is sent for `keystroke`, end to end.
    fn bytes(keystroke: &Keystroke) -> Vec<u8> {
        from_keystroke(keystroke)
            .and_then(|page| to_key(&page))
            .map(|key| encode(&key))
            .unwrap_or_default()
    }

    #[test]
    fn a_letter_types_itself() {
        let page = from_keystroke(&plain("a")).unwrap();
        assert_eq!((page.key.as_str(), page.code.as_str()), ("a", "KeyA"));
        assert!(!page.ctrl && !page.alt && !page.shift && !page.meta);
        assert_eq!(bytes(&plain("a")), b"a");
    }

    #[test]
    fn a_shifted_letter_is_the_capital() {
        let shifted = stroke(Modifiers::shift(), "a", Some("A"));
        let page = from_keystroke(&shifted).unwrap();
        assert_eq!((page.key.as_str(), page.shift), ("A", true));
        assert_eq!(bytes(&shifted), b"A");
    }

    #[test]
    fn a_shifted_symbol_is_the_symbol_typed_not_the_key_printed() {
        let bang = stroke(Modifiers::shift(), "1", Some("!"));
        let page = from_keystroke(&bang).unwrap();
        assert_eq!((page.key.as_str(), page.code.as_str()), ("!", "Digit1"));
        assert_eq!(bytes(&bang), b"!");
        assert_eq!(bytes(&stroke(Modifiers::shift(), ";", Some(":"))), b":");
    }

    #[test]
    fn a_character_the_layout_made_is_typed_whole() {
        assert_eq!(bytes(&stroke(Modifiers::none(), "e", Some("é"))), "é".as_bytes());
    }

    #[test]
    fn space_is_a_space_and_with_ctrl_the_leader() {
        assert_eq!(from_keystroke(&stroke(Modifiers::none(), "space", Some(" "))).unwrap().key, " ");
        let leader = from_keystroke(&stroke(Modifiers::control(), "space", None)).unwrap();
        assert_eq!((leader.key.as_str(), leader.ctrl), (" ", true));
        assert_eq!(bytes(&stroke(Modifiers::none(), "space", Some(" "))), b" ");
    }

    #[test]
    fn enter_backspace_tab_and_escape_are_the_pages_names() {
        for (gpui, dom, sent) in [
            ("enter", "Enter", &b"\r"[..]),
            ("backspace", "Backspace", b"\x7f"),
            ("tab", "Tab", b"\t"),
            ("escape", "Escape", b"\x1b"),
        ] {
            let page = from_keystroke(&named_key(gpui)).unwrap();
            assert_eq!(page.key, dom);
            assert_eq!(bytes(&named_key(gpui)), sent, "{gpui}");
        }
    }

    #[test]
    fn shift_tab_is_a_back_tab() {
        assert_eq!(bytes(&stroke(Modifiers::shift(), "tab", None)), b"\x1b[Z");
    }

    #[test]
    fn the_arrows_and_the_navigation_keys_are_the_pages_names() {
        for (gpui, dom) in [
            ("up", "ArrowUp"),
            ("down", "ArrowDown"),
            ("left", "ArrowLeft"),
            ("right", "ArrowRight"),
            ("home", "Home"),
            ("end", "End"),
            ("pageup", "PageUp"),
            ("pagedown", "PageDown"),
            ("insert", "Insert"),
            ("delete", "Delete"),
        ] {
            assert_eq!(from_keystroke(&named_key(gpui)).unwrap().key, dom);
        }
        assert_eq!(bytes(&named_key("up")), b"\x1b[A");
        assert_eq!(bytes(&named_key("left")), b"\x1b[D");
    }

    #[test]
    fn ctrl_c_is_the_interrupt() {
        let ctrl_c = stroke(Modifiers::control(), "c", None);
        let page = from_keystroke(&ctrl_c).unwrap();
        assert_eq!((page.key.as_str(), page.code.as_str(), page.ctrl), ("c", "KeyC", true));
        assert_eq!(bytes(&ctrl_c), [0x03]);
    }

    #[test]
    fn ctrl_shift_v_is_the_capital_the_paste_chord_names() {
        let page = from_keystroke(&stroke(Modifiers::control_shift(), "v", None)).unwrap();
        assert_eq!((page.key.as_str(), page.ctrl, page.shift), ("V", true, true));
    }

    #[test]
    fn ctrl_shift_of_a_symbol_is_the_shifted_symbol_the_dom_names() {
        for typed in [None, Some("_")] {
            let undo = stroke(Modifiers::control_shift(), "-", typed);
            assert_eq!(from_keystroke(&undo).unwrap().key, "_");
            assert_eq!(bytes(&undo), [0x1f]);
        }
        assert_eq!(bytes(&stroke(Modifiers::control_shift(), "/", None)), [0x7f]);
    }

    #[test]
    fn windows_sends_a_shifted_symbol_as_the_symbol_with_shift_cleared() {
        // gpui-pre-windows turns Ctrl-Shift-- into key `_` with no Shift, and
        // Alt-Shift-1 into `!`: the key already is the shifted one.
        let undo = stroke(Modifiers { shift: false, ..Modifiers::control_shift() }, "_", None);
        assert_eq!(from_keystroke(&undo).unwrap().key, "_");
        assert_eq!(bytes(&undo), [0x1f]);
        let bang = stroke(Modifiers { shift: false, ..Modifiers::alt() }, "!", None);
        assert_eq!(from_keystroke(&bang).unwrap().key, "!");
    }

    #[test]
    fn alt_with_a_letter_is_escape_then_the_letter() {
        // Windows types the letter under Alt; a Mac's option layer a symbol.
        for typed in [Some("f"), Some("ƒ"), None] {
            let alt_f = stroke(Modifiers::alt(), "f", typed);
            let page = from_keystroke(&alt_f).unwrap();
            assert_eq!((page.key.as_str(), page.code.as_str(), page.alt), ("f", "KeyF", true));
            assert_eq!(bytes(&alt_f), b"\x1bf", "{typed:?}");
        }
    }

    #[test]
    fn altgr_types_the_character_not_a_chord() {
        let both = Modifiers { control: true, alt: true, ..Modifiers::none() };
        let at = stroke(both, "q", Some("@"));
        let page = from_keystroke(&at).unwrap();
        assert_eq!((page.key.as_str(), page.ctrl, page.alt), ("@", false, false));
        assert_eq!(bytes(&at), b"@");
    }

    #[test]
    fn function_keys_are_f1_to_f24() {
        assert_eq!(from_keystroke(&named_key("f1")).unwrap().key, "F1");
        assert_eq!(from_keystroke(&named_key("f12")).unwrap().key, "F12");
        assert_eq!(from_keystroke(&named_key("f24")).unwrap().key, "F24");
        assert_eq!(bytes(&named_key("f5")), b"\x1b[15~");
        assert!(from_keystroke(&named_key("f25")).is_none());
        assert!(from_keystroke(&named_key("f0")).is_none());
    }

    #[test]
    fn modifiers_ride_along_on_a_named_key() {
        let page = from_keystroke(&stroke(Modifiers::control_shift(), "left", None)).unwrap();
        assert_eq!((page.key.as_str(), page.ctrl, page.shift), ("ArrowLeft", true, true));
    }

    #[test]
    fn the_command_key_is_meta_and_the_session_leaves_it_to_the_window() {
        let win_c = stroke(Modifiers::command(), "c", None);
        assert!(from_keystroke(&win_c).unwrap().meta);
        assert_eq!(bytes(&win_c), Vec::<u8>::new());
    }

    #[test]
    fn a_bare_modifier_or_an_unnamed_key_is_not_a_key() {
        for name in ["shift", "control", "alt", "platform", "function", "menu", "back", "forward"] {
            assert!(from_keystroke(&named_key(name)).is_none(), "{name}");
        }
    }

    #[test]
    fn text_of_more_than_one_character_is_left_to_the_input_handler() {
        assert!(from_keystroke(&stroke(Modifiers::none(), "a", Some("ab"))).is_none());
    }
}
