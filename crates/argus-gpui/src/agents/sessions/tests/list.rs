//! The list and the preview (SESSIONS-2..4, -7, -13, -15, -16, -22, -24): what is drawn for the
//! page, and the press on a row, the fold or an offer.

use argus_desktop_core::agents::sessions_dialog::SessionsPage;
use gpui_kit::{point, px, TestAppContext};

use super::{hear_page, line, preview, reading, row, said, sessions};
use crate::agents::sessions::Held;
use crate::keys::root::tests::{click_label, drain, has_label, open};

fn many() -> SessionsPage {
    let mut page = sessions();
    page.rows = (0..40).map(|n| row(n, &format!("Session {n}"), n == 0, n == 0)).collect();
    page
}

#[gpui_kit::test]
fn a_click_on_a_row_moves_the_cursor_there_and_the_row_is_drawn_for_what_core_sent(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = sessions();
    page.rows = vec![row(0, "Live one", true, true), row(1, "Another", true, false)];
    hear_page(cx, &opened, page);
    assert!(has_label(cx, &opened, "Live one") && has_label(cx, &opened, "Live"), "the Live heading names the group");
    drain(&opened);
    click_label(cx, &opened, "Another");
    assert_eq!(said(&opened), ["SessionsAction { action: Next, row: Some(1) }"]);
}

#[gpui_kit::test]
fn the_ended_group_is_one_row_that_asks_to_toggle_and_a_row_with_no_title_says_untitled(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = sessions();
    page.rows.push(row(1, "", false, false));
    hear_page(cx, &opened, page);
    assert!(has_label(cx, &opened, "4 \u{b7} newest 20m ago") || has_label(cx, &opened, "2 \u{b7} newest 20m ago"));
    assert!(has_label(cx, &opened, "Untitled session"));
    drain(&opened);
    click_label(cx, &opened, "Ended");
    assert_eq!(said(&opened), ["SessionsAction { action: ToggleEnded, row: None }"]);
}

#[gpui_kit::test]
fn a_search_shows_the_ended_rows_under_a_plain_heading_that_does_not_toggle(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = sessions();
    page.search = "ctx".into();
    page.ended_folded = false;
    page.rows.push(row(1, "Ended hit", false, false));
    hear_page(cx, &opened, page);
    assert!(has_label(cx, &opened, "Ended hit"));
    drain(&opened);
    click_label(cx, &opened, "Ended");
    assert!(said(&opened).is_empty(), "a heading is not a button: {:?}", said(&opened));
}

#[gpui_kit::test]
fn a_removed_worktrees_row_says_it_was_since_removed(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = sessions();
    let mut gone = row(1, "Old one", false, false);
    gone.removed = true;
    gone.checkout = "kitty-keys".into();
    page.rows.push(gone);
    page.ended_folded = false;
    hear_page(cx, &opened, page);
    assert!(has_label(cx, &opened, "claude on kitty-keys, since removed \u{b7} 3 turns"));
}

#[gpui_kit::test]
fn the_offer_of_each_kind_sends_what_its_button_says(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, sessions());
    drain(&opened);
    click_label(cx, &opened, "Go to its pane");
    click_label(cx, &opened, "Open full transcript");
    assert_eq!(said(&opened), ["SessionsAction { action: Choose, row: None }", "SessionsAction { action: Transcript, row: None }"]);
    let mut page = sessions();
    page.preview = Some(preview("resume"));
    hear_page(cx, &opened, page);
    assert!(has_label(cx, &opened, "claude --resume 2b7f1c9e"), "the exact command is named");
    drain(&opened);
    click_label(cx, &opened, "Resume in a new pane");
    assert_eq!(said(&opened), ["SessionsAction { action: Resume, row: None }"]);
}

#[gpui_kit::test]
fn a_session_that_cannot_be_resumed_says_why_and_offers_only_what_is_readable(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = sessions();
    let mut said_why = preview("transcript");
    said_why.why = Some("its worktree is gone".into());
    said_why.readable = false;
    page.preview = Some(said_why);
    hear_page(cx, &opened, page);
    assert!(has_label(cx, &opened, "Not resumable: its worktree is gone"));
    assert!(!has_label(cx, &opened, "Open full transcript") && !has_label(cx, &opened, "Resume in a new pane"));
}

#[gpui_kit::test]
fn the_list_says_loading_why_it_is_empty_and_a_preview_with_no_session_says_to_pick_one(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = sessions();
    page.loading = true;
    page.rows.clear();
    page.preview = None;
    hear_page(cx, &opened, page.clone());
    assert!(has_label(cx, &opened, "Loading sessions"));
    page.loading = false;
    page.empty = Some("No agent has worked in argus yet.".into());
    hear_page(cx, &opened, page);
    assert!(has_label(cx, &opened, "No agent has worked in argus yet.") && has_label(cx, &opened, "Pick a session."));
}

#[gpui_kit::test]
fn the_preview_says_a_notice_in_place_of_a_transcript_and_notes_the_earlier_entries(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = sessions();
    page.preview.as_mut().unwrap().notice = Some("Nothing was said in it yet.".into());
    hear_page(cx, &opened, page);
    assert!(has_label(cx, &opened, "Nothing was said in it yet."));
    let mut page = sessions();
    let preview = page.preview.as_mut().unwrap();
    preview.entries = vec![line("user", "hello there")];
    preview.earlier = 1;
    hear_page(cx, &opened, page);
    assert!(has_label(cx, &opened, "1 earlier entry not shown") && has_label(cx, &opened, "hello there"));
}

#[gpui_kit::test]
fn the_list_keeps_its_offset_while_the_reader_is_up_and_after_it_closes(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, many());
    let view = cx.update(|cx| cx.global::<Held>().0.clone());
    let list = view.read_with(cx, |view, _| view.list.clone());
    list.set_offset(point(px(0.0), px(-300.0)));
    let mut open_reader = many();
    open_reader.reader = reading().reader;
    open_reader.preview = None;
    hear_page(cx, &opened, open_reader);
    hear_page(cx, &opened, many());
    assert_eq!(list.offset().y, px(-300.0), "the page loses it; this window keeps where the person was");
}

#[gpui_kit::test]
fn a_transcript_newly_read_in_the_preview_opens_at_its_end_and_a_redraw_stays_where_it_was(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = sessions();
    page.preview.as_mut().unwrap().entries = (0..60).map(|n| line("agent", &format!("Reply number {n} to the person"))).collect();
    hear_page(cx, &opened, page.clone());
    let view = cx.update(|cx| cx.global::<Held>().0.clone());
    let body = view.read_with(cx, |view, _| view.body.clone());
    assert!(body.offset().y < px(0.0), "it opened scrolled to the end: {:?}", body.offset());
    body.set_offset(point(px(0.0), px(-120.0)));
    hear_page(cx, &opened, page);
    assert_eq!(body.offset().y, px(-120.0), "the same session redrawn stays");
}
