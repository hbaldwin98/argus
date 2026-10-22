//! The comment box among the lines (CODE-42, 49, 50).

use argus_desktop_core::review::comment_box::{CommentPage, Recipient};
use gpui_kit::TestAppContext;

use super::*;
use crate::checkouts::stage::tests::{click_id, present};
use crate::keys::root::tests::{press, type_text};

fn commenting(recipients: &[(u64, &str)], to: Option<u64>) -> CodePage {
    let mut page = reading(30, 5);
    page.lines[6].picked = true;
    page.comment = Some(CommentPage {
        about: "lines 7 \u{2013} 7 \u{b7} src/lib.rs".into(),
        recipients: recipients.iter().map(|(pane, title)| Recipient { pane: *pane, title: (*title).into() }).collect(),
        to,
    });
    page
}

/// CODE-49: the box sits in the flow under the last picked line, and the code's gutter clears it.
#[gpui_kit::test]
fn the_box_stands_under_the_last_picked_line_clear_of_the_gutter(cx: &mut TestAppContext) {
    let opened = with(cx, commenting(&[(7, "claude")], Some(7)));
    settle(cx, &opened);
    assert!(present(cx, &opened, "review-comment"));
    let (card, picked, next) = (bounds_of(cx, &opened, "review-comment"), bounds_of(cx, &opened, ("diff-row", 6usize)), bounds_of(cx, &opened, ("diff-row", 8usize)));
    assert!(card.origin.y >= picked.origin.y + picked.size.height, "under the picked line");
    assert!(next.origin.y >= card.origin.y + card.size.height - gpui_kit::px(1.0), "and above the next");
    let pane = bounds_of(cx, &opened, "code-lines-pane");
    assert!((card.origin.x - pane.origin.x).as_f32() >= 68.0, "the card clears the number column and the marker");
}

/// CODE-50: the words go as `Comment { body }` and the box stays until the session says it is saved.
#[gpui_kit::test]
fn sending_sends_the_words_to_the_code_stage_and_the_box_stays(cx: &mut TestAppContext) {
    let opened = with(cx, commenting(&[(7, "claude")], Some(7)));
    settle(cx, &opened);
    type_text(cx, &opened, "hi");
    click_id(cx, &opened, "comment-send");
    assert_eq!(sent(&opened), ["Code { gesture: Comment { body: Some(\"hi\") } }"]);
    assert!(present(cx, &opened, "review-comment"), "closed by the session, not by the press");
    says(cx, &opened, reading(30, 5));
    assert!(!present(cx, &opened, "review-comment"));
}

/// CODE-50: Escape in the box and Cancel put it away; neither is the review's call.
#[gpui_kit::test]
fn escape_and_cancel_put_the_box_away_through_the_code_stage(cx: &mut TestAppContext) {
    let opened = with(cx, commenting(&[(7, "claude")], Some(7)));
    settle(cx, &opened);
    press(cx, &opened, "escape");
    assert_eq!(sent(&opened), ["Code { gesture: Comment { body: None } }"]);
    click_id(cx, &opened, "comment-cancel");
    assert_eq!(sent(&opened), ["Code { gesture: Comment { body: None } }"]);
}

/// CODE-50: with several agents each is a chip, and choosing one is the Code stage's click.
#[gpui_kit::test]
fn choosing_an_agent_is_the_code_stages_click(cx: &mut TestAppContext) {
    let opened = with(cx, commenting(&[(7, "claude"), (9, "codex")], None));
    settle(cx, &opened);
    click_id(cx, &opened, "comment-recipient-9");
    assert_eq!(sent(&opened), ["Code { gesture: Click { target: Recipient { pane: 9 } } }"]);
}

/// CODE-42: an open box has the keys, so a letter is typed and not a key of the stage; once it
/// closes the stage has them back.
#[gpui_kit::test]
fn the_box_takes_the_keys_while_open_and_the_stage_has_them_back_once_it_closes(cx: &mut TestAppContext) {
    let opened = with(cx, commenting(&[(7, "claude")], Some(7)));
    settle(cx, &opened);
    press(cx, &opened, "j");
    assert_eq!(sent(&opened), Vec::<String>::new(), "j is typed, not the next line");
    says(cx, &opened, reading(30, 5));
    settle(cx, &opened);
    press(cx, &opened, "j");
    assert_eq!(sent(&opened), ["Code { gesture: Action { action: Next } }"]);
}
