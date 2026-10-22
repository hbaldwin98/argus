//! Which chords are the editor's own (EDITOR-KEYS-4). The key walk that holds every other key for
//! a focused editor is `keys::field`'s and is tested there.

use super::*;

fn key(name: &str, ctrl: bool, alt: bool, shift: bool) -> PageKey {
    PageKey { key: name.into(), code: String::new(), ctrl, alt, shift, meta: false }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn mod_chords_write_go_to_a_file_and_search_with_ctrl() {
    assert_eq!(chord(&key("s", true, false, false)), Some(Chord::Write));
    assert_eq!(chord(&key("p", true, false, false)), Some(Chord::GoToFile));
    assert_eq!(chord(&key("F", true, false, true)), Some(Chord::Search));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn plain_ctrl_f_stays_the_editors_find_and_a_bare_letter_is_typed() {
    assert_eq!(chord(&key("f", true, false, false)), None, "Ctrl F is the editor's find");
    assert_eq!(chord(&key("s", false, false, false)), None);
    assert_eq!(chord(&key("p", false, false, false)), None);
}

#[test]
fn alt_chords_comment_and_step_between_changes() {
    assert_eq!(chord(&key("c", false, true, false)), Some(Chord::Comment));
    assert_eq!(chord(&key("]", false, true, false)), Some(Chord::Step { forward: true }));
    assert_eq!(chord(&key("[", false, true, false)), Some(Chord::Step { forward: false }));
    assert_eq!(chord(&key("]", false, false, false)), None, "a bare bracket is a character");
    assert_eq!(chord(&key("c", true, true, false)), None, "Ctrl Alt c is not Alt c");
}
