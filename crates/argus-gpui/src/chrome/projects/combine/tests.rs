//! The combine-projects dialog in a test window: it is up while the session holds it, says only what
//! the page says, and sends its gestures without predicting the daemon's answer.

use std::time::Duration;

use argus_desktop_core::chrome::add_page::ProjectRef;
use argus_desktop_core::chrome::combine_page::{CombinePage, CombineRow, IntoChoice};
use argus_desktop_core::connection::page::Event;
use gpui_kit::prelude::*;
use gpui_kit::test::TestQueryExt as _;
use gpui_kit::TestAppContext;

use crate::keys::root::tests::{click_label, drain, has_label, hear, open, overlays, press, Opened};
use crate::kit::overlay::kind::Overlay;

fn row(id: u64, name: &str, ticked: bool) -> CombineRow {
    CombineRow { id, name: name.into(), repositories: 2, needs_you: 0, working: 0, scans_folder: false, ticked }
}

fn page() -> CombinePage {
    CombinePage {
        open: true,
        into: Some(ProjectRef { id: 1, name: "argus".into() }),
        choices: vec![IntoChoice { id: 1, name: "argus".into(), repositories: 1 }, IntoChoice { id: 2, name: "work".into(), repositories: 2 }, IntoChoice { id: 3, name: "dots".into(), repositories: 2 }],
        rows: vec![row(2, "work", false), CombineRow { needs_you: 1, scans_folder: true, ..row(3, "dots", false) }],
        holds: vec!["argus".into()],
        repositories: 1,
        ..CombinePage::default()
    }
}

fn say(page: CombinePage) -> Event {
    Event::Combine(Box::new(page))
}

/// The dialog up for `page`, settled, and the calls so far gone.
fn dialog(cx: &mut TestAppContext, page: CombinePage) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, vec![say(page)]);
    assert_eq!(overlays(cx), [Overlay::Combine]);
    cx.executor().advance_clock(Duration::from_secs(1));
    hear(cx, &opened, Vec::new());
    drain(&opened);
    opened
}

fn ticked(cx: &mut TestAppContext, opened: &Opened, label: &str) -> Option<bool> {
    cx.update_window(opened.handle, |_, window, _| window.find_by_label(label).checked()).unwrap()
}

#[gpui_kit::test]
fn the_dialog_is_up_while_the_session_holds_it_and_goes_when_it_does_not(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    assert!(has_label(cx, &opened, "Combine projects"));
    hear(cx, &opened, vec![say(CombinePage::default())]);
    assert!(overlays(cx).is_empty());
}

/// The dialog says what the page says: the project folded into and its tally, the others with their
/// notes, and nothing of a summary until something is ticked.
#[gpui_kit::test]
fn it_lists_the_other_projects_with_their_notes_and_says_nothing_until_one_is_ticked(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    assert!(has_label(cx, &opened, "keeps its name and settings"));
    assert!(has_label(cx, &opened, "Move these projects\u{2019} repositories into argus:"));
    assert!(has_label(cx, &opened, "work") && has_label(cx, &opened, "dots"));
    assert!(!has_label(cx, &opened, "Their feature boards come too; features keep their numbers."));
    assert!(has_label(cx, &opened, "Combine into argus"), "off with nothing ticked");
    hear(cx, &opened, vec![say(CombinePage { rows: vec![row(2, "work", true), page().rows[1].clone()], holds: vec!["argus".into(), "api".into()], repositories: 2, agents: 1, ..page() })]);
    assert!(has_label(cx, &opened, "argus will hold 2 repositories: argus, api."));
    assert!(has_label(cx, &opened, "Running panes move with their repositories and keep running: 1 agent."));
    assert!(has_label(cx, &opened, "Their feature boards come too; features keep their numbers."));
    assert!(has_label(cx, &opened, "Combine 1 into argus"));
}

/// A press on a row asks for the tick and draws none until the page says (the client never predicts).
#[gpui_kit::test]
fn a_row_asks_for_its_tick_and_is_drawn_ticked_when_the_page_says(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    click_label(cx, &opened, "work");
    assert_eq!(drain(&opened), ["CombineProjects { gesture: Tick { project: 2 } }"]);
    assert_eq!(ticked(cx, &opened, "work"), Some(false));
    hear(cx, &opened, vec![say(CombinePage { rows: vec![row(2, "work", true), page().rows[1].clone()], ..page() })]);
    assert_eq!(ticked(cx, &opened, "work"), Some(true));
}

#[gpui_kit::test]
fn combine_asks_only_with_something_ticked_and_shows_busy_while_the_daemon_answers(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    click_label(cx, &opened, "Combine into argus");
    assert!(drain(&opened).is_empty(), "nothing ticked, nothing asked");
    let with_tick = CombinePage { rows: vec![row(2, "work", true), page().rows[1].clone()], ..page() };
    hear(cx, &opened, vec![say(with_tick.clone())]);
    click_label(cx, &opened, "Combine 1 into argus");
    assert_eq!(drain(&opened), ["CombineProjects { gesture: Combine }"]);
    hear(cx, &opened, vec![say(CombinePage { combining: true, ..with_tick.clone() })]);
    assert!(has_label(cx, &opened, "Combining\u{2026}"));
    click_label(cx, &opened, "Combining\u{2026}");
    assert!(drain(&opened).is_empty(), "one ask at a time");
    assert_eq!(overlays(cx), [Overlay::Combine], "the dialog waits for the daemon");
}

#[gpui_kit::test]
fn a_refusal_is_said_in_the_dialog_which_stays_open(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    hear(cx, &opened, vec![say(CombinePage { refusal: Some("work is being removed".into()), ..page() })]);
    assert!(has_label(cx, &opened, "work is being removed"));
    assert_eq!(overlays(cx), [Overlay::Combine]);
}

#[gpui_kit::test]
fn cancel_and_escape_send_close_and_put_the_dialog_away(cx: &mut TestAppContext) {
    let opened = dialog(cx, page());
    click_label(cx, &opened, "Cancel");
    assert_eq!(drain(&opened), ["CombineProjects { gesture: Close }"]);
    assert!(overlays(cx).is_empty());
    hear(cx, &opened, vec![say(CombinePage { combining: false, ..page() })]);
    assert!(overlays(cx).is_empty(), "a page still open before the answer does not bring it back");
    hear(cx, &opened, vec![say(CombinePage::default())]);
    hear(cx, &opened, vec![say(page())]);
    assert_eq!(overlays(cx), [Overlay::Combine]);
    press(cx, &opened, "escape");
    assert_eq!(drain(&opened), ["CombineProjects { gesture: Close }"]);
}
