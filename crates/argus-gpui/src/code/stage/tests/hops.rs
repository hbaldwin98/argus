//! The hops into Code from a review or the history and back (CODE-8, REVIEW-AGENTS-REVIEW-32 and 44,
//! HISTORY-7 and 21).
//!
//! The session parks the dialog whole, so for the window the dialog is no longer open, the stage
//! is Code with a way back, and the Git stage is down; Back un-parks it and the dialog is open
//! again over the stage it was opened over. The window holds none of this and only draws what it
//! hears.

use argus_desktop_core::connection::page::{Event, HistoryPage, ReviewPage, Stage};
use gpui_kit::TestAppContext;

use super::*;
use crate::code::file::tests::reading;
use crate::keys::root::tests::{asked, press};

fn review_open() -> Event {
    Event::Review(Box::new(ReviewPage { open: true, checkout: 100, name: "main".into(), branch: "main".into(), ..ReviewPage::default() }))
}

fn review_away() -> Event {
    Event::Review(Box::default())
}

fn history_open() -> Event {
    Event::History(Box::new(HistoryPage { open: true, name: "main".into(), branch: "main".into(), ..HistoryPage::default() }))
}

fn rail_on(stage: Stage) -> Event {
    Event::Rail(Box::new(argus_desktop_core::connection::page::Rail { stage, ..rail() }))
}

fn from_review() -> CodePage {
    CodePage { back: Some("review \u{b7} lib.rs:4".into()), ..reading(30, 4) }
}

/// REVIEW-44: `o` parks the review: the Git stage is down, Code is up with its way back and the
/// keys, and the review is not drawn though the session still holds it.
#[gpui_kit::test]
fn a_parked_review_is_no_stage_and_code_stands_in_its_place(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), rail_on(Stage::Checkouts), review_open()]);
    assert!(present(cx, &opened, "git"), "the review is the Git stage while it is open");
    hear(cx, &opened, vec![review_away(), rail_on(Stage::Code), Event::Code(Box::new(from_review()))]);
    assert!(!present(cx, &opened, "git") && present(cx, &opened, "code-file-area"));
    assert_eq!(label(cx, &opened, "code-back").as_deref(), Some("Back to review \u{b7} lib.rs:4"));
}

/// CODE-8: `B`, the button and Escape each send what the page sends; the window changes only when the
/// review is open again, on the Changes tab, over the stage it was opened over.
#[gpui_kit::test]
fn back_asks_the_session_and_the_review_returns_as_it_was_when_it_says_so(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), rail_on(Stage::Code), Event::Code(Box::new(from_review()))]);
    drain(&opened);
    press(cx, &opened, "b");
    click_id(cx, &opened, "code-back");
    press(cx, &opened, "escape");
    assert_eq!(
        asked(&opened).into_iter().filter(|call| call.starts_with("Code")).collect::<Vec<_>>(),
        [
            "Code { gesture: Action { action: GoBack } }",
            "Code { gesture: Click { target: Back } }",
            "Code { gesture: Action { action: CloseLayer } }",
        ]
    );
    assert!(present(cx, &opened, "code-file-area") && !present(cx, &opened, "git"), "a request changes nothing");
    hear(cx, &opened, vec![review_open(), rail_on(Stage::Checkouts), Event::Code(Box::new(CodePage { back: None, ..from_review() }))]);
    assert!(present(cx, &opened, "git") && present(cx, &opened, "review") && !present(cx, &opened, "code-file-area"));
}

/// HISTORY-21: a parked history comes back on the History tab.
#[gpui_kit::test]
fn a_parked_history_comes_back_on_the_history_tab(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), rail_on(Stage::Checkouts), history_open()]);
    assert!(present(cx, &opened, "history"));
    hear(cx, &opened, vec![Event::History(Box::default()), rail_on(Stage::Code), Event::Code(Box::new(CodePage { back: Some("history".into()), ..reading(30, 1) }))]);
    assert!(!present(cx, &opened, "git") && label(cx, &opened, "code-back").as_deref() == Some("Back to history"));
    hear(cx, &opened, vec![history_open(), rail_on(Stage::Checkouts)]);
    assert!(present(cx, &opened, "history") && !present(cx, &opened, "review"));
}
