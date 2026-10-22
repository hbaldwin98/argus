use super::*;

fn page(key: &str) -> PageKey {
    PageKey { key: key.into(), ..PageKey::default() }
}

#[test]
fn a_character_is_itself_with_shift_already_in_it() {
    assert_eq!(vim_key(&page("j")), Some(Key::ch('j')));
    assert_eq!(vim_key(&PageKey { shift: true, ..page("J") }), Some(Key::ch('J')));
    assert_eq!(vim_key(&PageKey { shift: true, ..page(":") }), Some(Key::ch(':')));
    assert_eq!(vim_key(&page("é")), Some(Key::ch('é')));
}

#[test]
fn a_ctrl_chord_names_its_character_unshifted_and_ctrl_left_square_bracket_is_escape() {
    assert_eq!(vim_key(&PageKey { ctrl: true, ..page("r") }), Some(Key::ctrl('r')));
    assert_eq!(vim_key(&PageKey { ctrl: true, ..page("[") }), Some(Key::of(Code::Esc)));
    assert_eq!(vim_key(&PageKey { ctrl: true, ..page("p") }), Some(Key::ctrl('p')));
}

#[test]
fn the_named_keys_are_vims() {
    for (name, code) in [
        ("Escape", Code::Esc),
        ("Enter", Code::Enter),
        ("Tab", Code::Tab),
        ("Backspace", Code::Backspace),
        ("Delete", Code::Delete),
        ("Insert", Code::Insert),
        ("ArrowLeft", Code::Left),
        ("ArrowRight", Code::Right),
        ("ArrowUp", Code::Up),
        ("ArrowDown", Code::Down),
        ("Home", Code::Home),
        ("End", Code::End),
        ("PageUp", Code::PageUp),
        ("PageDown", Code::PageDown),
    ] {
        assert_eq!(vim_key(&page(name)), Some(Key::of(code)), "{name}");
    }
}

#[test]
fn only_space_and_backspace_tell_shift() {
    assert_eq!(vim_key(&PageKey { shift: true, ..page(" ") }), Some(Key::shifted(Code::Char(' '))));
    assert_eq!(vim_key(&PageKey { shift: true, ..page("Backspace") }), Some(Key::shifted(Code::Backspace)));
    assert_eq!(vim_key(&PageKey { shift: true, ..page("ArrowDown") }), Some(Key::of(Code::Down)));
    assert_eq!(vim_key(&PageKey { ctrl: true, ..page(" ") }), Some(Key::ctrl(' ')));
    assert_eq!(vim_key(&PageKey { ctrl: true, ..page("Backspace") }), Some(Key { ctrl: true, ..Key::of(Code::Backspace) }));
}

#[test]
fn a_key_held_with_alt_or_the_command_key_is_not_vims() {
    assert_eq!(vim_key(&PageKey { alt: true, ..page("c") }), None);
    assert_eq!(vim_key(&PageKey { meta: true, ..page("s") }), None);
}

#[test]
fn a_bare_modifier_a_function_key_and_a_dead_key_are_not_vims() {
    for name in ["Shift", "Control", "Alt", "Meta", "F5", "Dead", "CapsLock", ""] {
        assert_eq!(vim_key(&page(name)), None, "{name:?}");
    }
}

#[test]
fn ctrl_shift_and_a_letter_is_not_vims_so_the_editors_own_chord_has_it() {
    // EDITOR-KEYS-4: `Mod-Shift-f` reaches the editor's key with Vim on; Ctrl-f alone is the page down.
    assert_eq!(vim_key(&PageKey { ctrl: true, shift: true, ..page("F") }), None);
    assert_eq!(vim_key(&PageKey { ctrl: true, ..page("f") }), Some(Key::ctrl('f')));
}
