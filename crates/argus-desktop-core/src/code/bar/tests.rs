use super::*;

/// EDITOR-STATE: the table of words, one for each status.
#[test]
fn each_status_has_its_words() {
    assert_eq!(said(&Status::Opening), "Opening\u{2026}");
    assert_eq!(said(&Status::Saved), "Saved");
    assert_eq!(said(&Status::Unsaved), "Unsaved");
    assert_eq!(said(&Status::Saving), "Saving\u{2026}");
    assert_eq!(said(&Status::Clash), "An agent changed what you were editing");
    assert_eq!(said(&Status::Failed("disk full".into())), "Not saved: disk full");
    assert_eq!(said(&Status::Closed("it is binary".into())), "Closed: it is binary");
    assert_eq!(said(&Status::Offline), "Disconnected; kept here until the daemon is back");
}

#[test]
fn faults_are_danger_the_lost_connection_is_attention_and_the_quiet_states_are_quiet() {
    assert_eq!(tone(&Status::Saved), Tone::Faint);
    assert_eq!(tone(&Status::Opening), Tone::Faint);
    assert_eq!(tone(&Status::Unsaved), Tone::Muted);
    assert_eq!(tone(&Status::Saving), Tone::Muted);
    assert_eq!(tone(&Status::Clash), Tone::Danger);
    assert_eq!(tone(&Status::Failed(String::new())), Tone::Danger);
    assert_eq!(tone(&Status::Closed(String::new())), Tone::Danger);
    assert_eq!(tone(&Status::Offline), Tone::Attention);
}
