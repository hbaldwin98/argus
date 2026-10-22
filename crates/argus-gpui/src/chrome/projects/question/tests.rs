//! The removal question in a test window: up exactly while the rail holds it, and it answers
//! through the page's call, `Keep it` first.

use std::time::Duration;

use argus_desktop_core::connection::page::{Event, Rail, Removing};
use gpui_kit::TestAppContext;

use crate::keys::root::tests::{click_label, drain, has_label, hear, open, overlays, press, Opened};
use crate::kit::overlay::kind::Overlay;

fn asking(panes: usize) -> Event {
    Event::Rail(Box::new(Rail {
        loaded: true,
        removing: Some(Removing { project: 1, name: "orion".into(), repositories: 2, panes }),
        ..Rail::default()
    }))
}

fn nothing_asked() -> Event {
    Event::Rail(Box::new(Rail { loaded: true, ..Rail::default() }))
}

fn question(cx: &mut TestAppContext, panes: usize) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, vec![asking(panes)]);
    assert_eq!(overlays(cx), [Overlay::Question]);
    cx.executor().advance_clock(Duration::from_secs(1));
    hear(cx, &opened, Vec::new());
    drain(&opened);
    opened
}

/// SHELL-SWITCH-6: it says what is removed and what leaves the sidebar.
#[gpui_kit::test]
fn the_question_names_the_project_and_says_what_leaves_the_sidebar(cx: &mut TestAppContext) {
    let opened = question(cx, 0);
    assert!(has_label(cx, &opened, "Remove orion?"));
    assert!(has_label(cx, &opened, "Its 2 repositories leave the sidebar. The folders stay where they are, and adding it again brings them back."));
    assert!(!has_label(cx, &opened, "1 pane is running in it; argusd removes it only once they are closed."));
}

/// SHELL-SWITCH-6: running panes are warned of, in the singular or the plural.
#[gpui_kit::test]
fn running_panes_are_warned_of(cx: &mut TestAppContext) {
    let opened = question(cx, 2);
    assert!(has_label(cx, &opened, "2 panes are running in it; argusd removes it only once they are closed."));
}

/// SHELL-SWITCH-6: it is up while the rail holds it and goes when the rail does not, and one
/// rail heard twice is one question.
#[gpui_kit::test]
fn it_is_up_while_the_rail_holds_it_and_once(cx: &mut TestAppContext) {
    let opened = question(cx, 0);
    hear(cx, &opened, vec![asking(0)]);
    assert_eq!(overlays(cx), [Overlay::Question], "drawn once while held");
    hear(cx, &opened, vec![nothing_asked()]);
    assert!(overlays(cx).is_empty());
}

/// SHELL-SWITCH-6: `Keep it` answers no.
#[gpui_kit::test]
fn keep_it_answers_no(cx: &mut TestAppContext) {
    let opened = question(cx, 0);
    click_label(cx, &opened, "Keep it");
    assert_eq!(drain(&opened), ["AnswerRemoveProject { yes: false }"]);
}

/// SHELL-SWITCH-6: `Keep it` has the keys, so Enter is a press on it.
#[gpui_kit::test]
fn keep_it_has_the_keys_so_enter_answers_no(cx: &mut TestAppContext) {
    let opened = question(cx, 0);
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["AnswerRemoveProject { yes: false }"]);
}

/// SHELL-SWITCH-6: Escape answers no.
#[gpui_kit::test]
fn escape_answers_no(cx: &mut TestAppContext) {
    let opened = question(cx, 0);
    press(cx, &opened, "escape");
    assert_eq!(drain(&opened), ["AnswerRemoveProject { yes: false }"]);
    assert!(overlays(cx).is_empty());
}

/// SHELL-SWITCH-6: `Remove` answers yes, and the question stays until the session lets it go: the
/// project leaves when the daemon's tree says so.
#[gpui_kit::test]
fn remove_answers_yes_and_the_question_stays_until_the_rail_lets_it_go(cx: &mut TestAppContext) {
    let opened = question(cx, 0);
    click_label(cx, &opened, "Remove");
    assert_eq!(drain(&opened), ["AnswerRemoveProject { yes: true }"]);
    assert_eq!(overlays(cx), [Overlay::Question], "the client does not predict that it was removed");
}

/// SHELL-SWITCH-6: once no is said, a rail that still carries the question does not bring it back;
/// the next without it clears that, and a later question is asked.
#[gpui_kit::test]
fn a_no_is_not_undone_by_a_rail_that_still_carries_the_question(cx: &mut TestAppContext) {
    let opened = question(cx, 0);
    press(cx, &opened, "escape");
    drain(&opened);
    hear(cx, &opened, vec![asking(0)]);
    assert_eq!(overlays(cx), [], "the same rail again does not reopen it");
    hear(cx, &opened, vec![nothing_asked()]);
    hear(cx, &opened, vec![asking(0)]);
    assert_eq!(overlays(cx), [Overlay::Question], "a new question is asked");
}
