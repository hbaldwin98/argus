//! Writing a feature's or task's brief in the brief window.

use super::*;

#[test]
fn m_opens_nothing_now_that_rows_hold_no_notes() {
    let mut h = Harness::new();
    h.keys("ll");

    h.key(KeyCode::Char('m'));

    assert!(h.app.overlay.is_none());
    assert!(h.sent().is_empty());
}

#[test]
fn typing_a_brief_saves_it_on_leaving_insert_mode() {
    let mut h = harness_with_a_brief("");
    h.key(KeyCode::Char('i'));
    h.keys("hello");
    assert!(h.sent().is_empty(), "nothing goes out mid-word");

    h.key(KeyCode::Esc);

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::SetFeatureBody { slug, body, .. }] if slug == "pty" && body == "hello"
    ));
    assert_eq!(h.app.brief.as_ref().unwrap().mode, BriefMode::View);
    assert!(matches!(h.app.overlay, Some(Overlay::Brief)), "still open");
}

#[test]
fn an_unchanged_brief_is_not_written_back() {
    let mut h = harness_with_a_brief("one");
    h.key(KeyCode::Char('i'));
    h.key(KeyCode::Esc);
    assert!(h.sent().is_empty());
}

#[test]
fn q_saves_and_closes_the_window() {
    let mut h = harness_with_a_brief("start");
    h.key(KeyCode::Char('$'));
    h.key(KeyCode::Char('i'));
    h.keys("!");
    h.key(KeyCode::Esc);
    h.sent();
    h.key(KeyCode::Char('$'));
    h.key(KeyCode::Char('i'));
    h.keys("?");

    h.key(KeyCode::Esc);
    h.key(KeyCode::Char('q'));

    assert!(h.app.overlay.is_none());
    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::SetFeatureBody { body, .. }] if body == "start!?"
    ));
}

#[test]
fn in_insert_mode_navigation_keys_are_just_letters() {
    let mut h = harness_with_a_brief("");
    h.key(KeyCode::Char('i'));
    h.keys("jkqfF ");
    assert_eq!(h.app.brief.as_ref().unwrap().body(), "jkqfF ");
    assert!(matches!(h.app.overlay, Some(Overlay::Brief)));
}
