//! Telling the daemon what this client shows, so it can say where the
//! person is working.

use super::*;
use argus_protocol::{Hello, View};

fn views(sent: Vec<ClientMsg>) -> Vec<View> {
    sent.into_iter()
        .filter_map(|msg| match msg {
            ClientMsg::View(view) => Some(view),
            _ => None,
        })
        .collect()
}

/// A harness whose daemon greeted with this build's capabilities, focus
/// among them, and has been told the first view.
fn greeted() -> Harness {
    let mut h = Harness::new();
    h.app.greeted(Some(&Hello::this_build()));
    h.app.report_view();
    h
}

#[test]
fn the_daemon_is_told_the_selection_once_and_again_when_it_moves() {
    let mut h = greeted();
    let first = views(h.sent());
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].project, Some(ProjectId(1)));
    assert!(first[0].active);

    h.app.report_view();
    assert!(views(h.sent()).is_empty(), "nothing moved");

    h.keys("lllj");
    h.app.report_view();
    let moved = views(h.sent());
    assert_eq!(moved, vec![h.app.view()]);
    assert_ne!(moved[0], first[0]);
}

#[test]
fn a_daemon_that_cannot_take_a_view_is_never_sent_one() {
    let mut h = Harness::new();
    let older = Hello {
        capabilities: Vec::new(),
        ..Hello::this_build()
    };
    h.app.greeted(Some(&older));
    h.app.report_view();
    assert!(views(h.sent()).is_empty());
}

#[test]
fn a_terminal_that_loses_focus_says_so_and_takes_it_back_on_return() {
    let mut h = greeted();
    h.sent();

    h.app.set_terminal_focus(false);
    let away = views(h.sent());
    assert_eq!(away.len(), 1);
    assert!(!away[0].active);

    // Returning to the same selection is still said, so this client takes
    // the focus back from whichever the person was at meanwhile.
    h.app.set_terminal_focus(true);
    let back = views(h.sent());
    assert_eq!(back.len(), 1);
    assert!(back[0].active);
}

#[test]
fn a_host_taken_off_screen_says_it_shows_nothing() {
    let mut h = greeted();
    h.sent();

    h.app.set_on_screen(false);
    assert_eq!(views(h.sent()), vec![View::default()]);
}

#[test]
fn a_daemon_reached_again_is_told_afresh() {
    let mut h = greeted();
    h.sent();

    h.app.greeted(Some(&Hello::this_build()));
    h.app.report_view();
    assert_eq!(views(h.sent()).len(), 1, "a new daemon has been told nothing");
}
