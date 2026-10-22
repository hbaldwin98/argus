use argus_protocol::TextChange;

use super::super::change::Change;
use super::*;

fn opened(version: u64, text: &str) -> EditEvent {
    EditEvent::Opened { checkout: 1, path: "a.rs".into(), version, text: text.into() }
}

fn type_at(live: &mut Live, len: usize, at: usize, text: &str) {
    let set = ChangeSet::of(&[Change { from: at, to: at, insert: text.into() }], len).unwrap();
    live.typed(&set);
}

fn is_open(ask: &EditGesture) -> bool {
    matches!(ask, EditGesture::Open { checkout: 1, path } if path == "a.rs")
}

fn opened_live(text: &str) -> Live {
    let (mut live, ask) = Live::open(1, "a.rs");
    assert!(is_open(&ask));
    let heard = live.heard(&opened(0, text));
    assert!(heard.mount.is_some());
    live
}

#[test]
fn a_file_opens_to_a_view_and_is_saved() {
    let (mut live, _) = Live::open(1, "a.rs");
    assert_eq!(live.status(), &Status::Opening);
    let heard = live.heard(&opened(0, "abc"));
    assert_eq!(heard.mount.unwrap().to_string(), "abc");
    assert_eq!(heard.read_only, Some(false));
    assert_eq!(live.status(), &Status::Saved);
}

#[test]
fn typing_is_written_and_saved_in_turn() {
    let mut live = opened_live("abc");
    type_at(&mut live, 3, 3, "!");
    assert_eq!(live.status(), &Status::Unsaved);
    let Some(EditGesture::Change { base, changes, .. }) = live.write() else { panic!("a write") };
    assert_eq!((base, changes), (0, vec![TextChange { from: 3, to: 3, insert: "!".into() }]));
    assert_eq!(live.status(), &Status::Saving);
    let heard = live.heard(&EditEvent::Saved { checkout: 1, path: "a.rs".into(), base: 0, version: 1 });
    assert!(heard.asks.is_empty() && heard.reread);
    assert_eq!(live.status(), &Status::Saved);
}

#[test]
fn a_change_on_a_version_not_held_opens_the_file_again_and_shows_nothing() {
    let mut live = opened_live("abc");
    let heard = live.heard(&EditEvent::Changed {
        checkout: 1,
        path: "a.rs".into(),
        from: 5,
        version: 6,
        changes: vec![],
    });
    assert!(heard.shown.is_none() && heard.asks.len() == 1 && is_open(&heard.asks[0]));
}

#[test]
fn the_version_the_daemon_answered_is_the_one_the_first_change_is_made_on() {
    // EDITOR-OPEN-14: a literal 0 would ask to open the file again.
    let (mut live, _) = Live::open(1, "a.rs");
    live.heard(&opened(7, "abc"));
    let heard = live.heard(&EditEvent::Changed {
        checkout: 1,
        path: "a.rs".into(),
        from: 7,
        version: 8,
        changes: vec![TextChange { from: 0, to: 1, insert: "A".into() }],
    });
    assert!(heard.asks.is_empty());
    let mut doc = Rope::from_str("abc");
    heard.shown.unwrap().apply_in_place(&mut doc);
    assert_eq!(doc.to_string(), "Abc");
}

#[test]
fn a_refusal_with_a_reason_fails_and_asks_to_try_again_but_one_without_does_not() {
    let mut live = opened_live("abc");
    type_at(&mut live, 3, 3, "!");
    let _ = live.write();
    let said = |reason: Option<&str>| EditEvent::Refused {
        checkout: 1,
        path: "a.rs".into(),
        base: 0,
        reason: reason.map(str::to_owned),
    };
    let heard = live.heard(&said(Some("could not write a.rs")));
    assert!(heard.retry);
    assert_eq!(live.status(), &Status::Failed("could not write a.rs".into()));
    assert!(live.write().is_some(), "the write is typed again");

    let mut other = opened_live("abc");
    type_at(&mut other, 3, 3, "!");
    let _ = other.write();
    assert!(!other.heard(&said(None)).retry);
}

#[test]
fn a_lost_connection_keeps_the_buffer_and_writes_nothing_until_the_file_is_open_again() {
    // EDITOR-RECONN-1, -2 and -11.
    let mut live = opened_live("abc");
    type_at(&mut live, 3, 3, "!");
    assert!(live.write().is_some());

    live.heard(&EditEvent::Lost);
    assert_eq!(live.status(), &Status::Offline);
    type_at(&mut live, 4, 0, "<");
    assert!(live.write().is_none(), "typing stays unsent while the connection is down");
    assert!(live.sync().unwrap().pending());

    let mut lives = Lives::default();
    lives.all.insert((1, "a.rs".into()), live);
    let asks = lives.connected();
    assert!(asks.len() == 1 && is_open(&asks[0]), "the connection coming back opens the file");
    let live = lives.get_mut(1, "a.rs").unwrap();
    assert_eq!(live.status(), &Status::Opening);
    assert!(live.write().is_none(), "still unsent until the daemon answers the open");

    // The write in flight never landed: the file is as it was, and the
    // typing, the write that was lost with it too, goes out on top.
    let heard = live.heard(&opened(0, "abc"));
    assert!(heard.shown.unwrap().is_empty());
    let Some(EditGesture::Change { changes, .. }) = heard.asks.first() else { panic!("typing is written on top") };
    assert_eq!(changes.len(), 2);
    assert_eq!(live.sync().unwrap().doc().to_string(), "<abc!");
    assert_eq!(live.status(), &Status::Saving);
}

#[test]
fn a_write_that_landed_before_the_connection_went_is_a_clash_held_not_a_guess() {
    let mut live = opened_live("abc");
    type_at(&mut live, 3, 3, "!");
    assert!(live.write().is_some());
    live.heard(&EditEvent::Lost);
    let mut lives = Lives::default();
    lives.all.insert((1, "a.rs".into()), live);
    lives.connected();
    let live = lives.get_mut(1, "a.rs").unwrap();
    // The file holds the write: both inserted at one place.
    let heard = live.heard(&opened(0, "abc!"));
    assert!(heard.asks.is_empty(), "nothing is written over a clash");
    assert_eq!(live.status(), &Status::Clash);
}

#[test]
fn only_a_file_the_connection_took_is_opened_again() {
    let mut lives = Lives::default();
    assert!(lives.open(1, "a.rs").is_some());
    assert!(lives.open(1, "a.rs").is_none(), "a second look at an open file asks nothing");
    lives.open(1, "b.rs");
    lives.heard(&opened(0, "abc"));
    assert!(lives.connected().is_empty(), "nothing was lost");
    lives.heard(&EditEvent::Lost);
    assert_eq!(lives.connected().len(), 2);
    assert!(lives.connected().is_empty(), "and only once");
}

#[test]
fn a_closed_file_stays_closed_across_a_lost_connection() {
    // EDITOR-RECONN-7: the page forgets it; the port does not.
    let mut live = opened_live("abc");
    live.heard(&EditEvent::Closed { checkout: 1, path: "a.rs".into(), reason: "gone".into() });
    assert_eq!(live.status(), &Status::Closed("gone".into()));
    live.heard(&EditEvent::Lost);
    assert_eq!(live.status(), &Status::Closed("gone".into()));
    let mut lives = Lives::default();
    lives.all.insert((1, "a.rs".into()), live);
    assert!(lives.connected().is_empty());
    type_at(lives.get_mut(1, "a.rs").unwrap(), 3, 3, "!");
    assert!(lives.get_mut(1, "a.rs").unwrap().write().is_none());
}

#[test]
fn a_clash_waits_on_the_person_and_writes_nothing_until_resolved() {
    let mut live = opened_live("let x = 1;");
    let set = ChangeSet::of(&[Change { from: 8, to: 9, insert: "2".into() }], 10).unwrap();
    live.typed(&set);
    let heard = live.heard(&EditEvent::Changed {
        checkout: 1,
        path: "a.rs".into(),
        from: 0,
        version: 1,
        changes: vec![TextChange { from: 8, to: 9, insert: "3".into() }],
    });
    assert!(heard.asks.is_empty());
    assert_eq!(live.status(), &Status::Clash);
    assert!(live.write().is_none());

    let heard = live.resolve(Keep::Mine);
    assert!(heard.shown.is_none());
    assert!(matches!(heard.asks.as_slice(), [EditGesture::Change { base: 1, .. }]));
}

#[test]
fn a_retired_file_is_done_once_everything_is_written() {
    let mut live = opened_live("abc");
    type_at(&mut live, 3, 3, "!");
    assert!(live.retire().is_some());
    assert!(!live.done(), "the write is in flight");
    live.heard(&EditEvent::Saved { checkout: 1, path: "a.rs".into(), base: 0, version: 1 });
    assert!(live.done());
    live.shown_again();
    assert!(!live.done());
}
