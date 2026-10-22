use super::*;

#[test]
fn nothing_is_playing_until_a_register_is() {
    let m = Macros::default();
    assert!(!m.playing());
    assert!(m.recording.is_none());
    assert_eq!(m.latest, None);
}

#[test]
fn something_is_playing_for_as_long_as_one_is_inside_another() {
    let m = Macros { depth: 2, ..Macros::default() };
    assert!(m.playing());
}

#[test]
fn the_limits_are_a_hundred_levels_and_ten_thousand_keys() {
    assert_eq!(MOST_DEPTH, 100);
    assert_eq!(MOST_KEYS, 10_000);
}
