use super::*;

#[test]
fn a_key_waits_for_the_pause_unless_typing_never_paused() {
    let mut typing = Typing::default();
    assert_eq!(typing.typed(0), Then::After(QUIET_MS));
    assert_eq!(typing.typed(150), Then::After(QUIET_MS));
    assert_eq!(typing.typed(999), Then::After(QUIET_MS));
    assert_eq!(typing.typed(1000), Then::Write, "a burst of a second is written at its next key");
}

#[test]
fn a_write_ends_the_burst_so_the_next_key_begins_another() {
    let mut typing = Typing::default();
    typing.typed(0);
    typing.written();
    assert_eq!(typing.typed(5000), Then::After(QUIET_MS));
    assert_eq!(typing.typed(5500), Then::After(QUIET_MS));
}
