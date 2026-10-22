//! The reader in a window: its keys, its jump list and find, what its scroll follows, and the lines it
//! draws or skips (TRANSCRIPT-1..16).

use argus_desktop_core::connection::page::{ReaderPage, TranscriptLine};
use argus_desktop_core::system::clock::Clock;
use gpui_kit::TestAppContext;

use super::entry::{draws, entry, Mark};
use super::*;
use crate::agents::sessions::tests::{hear_page, line, reading, said};
use crate::keys::root::tests::{asked, click_label, drain, has_label, open, press, type_text};

fn page() -> ReaderPage {
    reading().reader.unwrap()
}

#[test]
fn a_transcript_newly_read_opens_at_its_end_a_move_scrolls_its_entry_and_otherwise_it_stays() {
    let mut reader = page();
    let mut placed = Placed::default();
    assert_eq!(scroll_for(&reader, &placed), Scroll::End, "newly read");
    placed = Placed { session: Some(reader.session.session_id.clone()), moves: 0, turn: None };
    assert_eq!(scroll_for(&reader, &placed), Scroll::Stay, "the same session redrawn");
    reader.moves = 1;
    reader.target = Some(3);
    assert_eq!(scroll_for(&reader, &placed), Scroll::Entry(3), "a move goes to its entry");
    placed.moves = 1;
    assert_eq!(scroll_for(&reader, &placed), Scroll::Stay, "the same move is done");
    reader.moves = 2;
    assert_eq!(scroll_for(&reader, &placed), Scroll::Entry(3), "a move to where the page already is still scrolls back");
    reader.moves = 3;
    reader.target = None;
    assert_eq!(scroll_for(&reader, &placed), Scroll::Stay, "a move with nowhere to go does not");
    reader.session.loading = true;
    placed.session = None;
    assert_eq!(scroll_for(&reader, &placed), Scroll::Stay, "nothing is read while it loads");
}

#[test]
fn a_line_of_a_kind_this_window_does_not_know_is_skipped_and_the_rest_keep_their_places() {
    let unknown = TranscriptLine { kind: "thinking", ..line("agent", "hmm") };
    assert!(!draws(&unknown) && draws(&line("user", "x")) && draws(&line("tool", "y")));
    let tokens = crate::theme::themes::ledger::tokens();
    assert!(entry("e", &unknown, Mark::None, &Clock::local(), &tokens).is_none());
    let mut reader = page();
    reader.session.entries = vec![line("user", "a"), unknown, line("agent", "b"), line("user", "c")];
    reader.session.earlier = 2;
    // Entry 3 is the third drawn child after the note of the earlier ones.
    assert_eq!(child_index(&reader, 3), 3);
    assert_eq!(child_index(&reader, 0), 1);
}


#[gpui_kit::test]
fn the_readers_keys_are_the_transcript_places_and_the_lists_keys_are_not_heard(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, reading());
    drain(&opened);
    for (key, action) in [
        ("j", "Next"),
        ("down", "Next"),
        ("k", "Previous"),
        ("home", "First"),
        ("end", "Last"),
        ("n", "NextMatch"),
        ("shift-n", "PreviousMatch"),
        ("y", "CopyId"),
        ("r", "Resume"),
        ("enter", "Choose"),
        ("escape", "CloseLayer"),
    ] {
        press(cx, &opened, key);
        assert_eq!(said(&opened).pop().unwrap_or_default(), format!("SessionsAction {{ action: {action}, row: None }}"), "{key}");
        drain(&opened);
    }
    for key in ["h", "c", "o"] {
        press(cx, &opened, key);
    }
    assert!(asked(&opened).is_empty(), "the list's own keys are not the reader's: {:?}", asked(&opened));
}

#[gpui_kit::test]
fn slash_gives_find_the_keys_each_edit_is_sent_and_enter_gives_the_reader_the_keys_back(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, reading());
    drain(&opened);
    press(cx, &opened, "/");
    type_text(cx, &opened, "br");
    assert_eq!(said(&opened), ["FindInTranscript { find: \"b\" }", "FindInTranscript { find: \"br\" }"]);
    drain(&opened);
    press(cx, &opened, "enter");
    assert!(asked(&opened).is_empty(), "Enter only blurs the field");
    press(cx, &opened, "j");
    assert_eq!(said(&opened), ["SessionsAction { action: Next, row: None }"]);
}

#[gpui_kit::test]
fn a_turn_row_sends_its_place_in_the_list_and_the_back_button_asks_to_close(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear_page(cx, &opened, reading());
    drain(&opened);
    click_label(cx, &opened, "Turn: Second ask");
    assert_eq!(said(&opened), ["SessionsAction { action: Next, row: Some(1) }"]);
    drain(&opened);
    click_label(cx, &opened, "Back to the recent sessions");
    assert_eq!(said(&opened), ["SessionsAction { action: CloseLayer, row: None }"]);
}

#[gpui_kit::test]
fn the_reader_names_the_session_counts_the_matches_and_says_when_there_are_no_turns(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut reader = reading();
    {
        let page = reader.reader.as_mut().unwrap();
        page.find = "ask".into();
        page.matches = vec![0, 3];
        page.found = Some(3);
    }
    hear_page(cx, &opened, reader);
    assert!(has_label(cx, &opened, "Trim the context") && has_label(cx, &opened, "2 of 2"));
    assert!(has_label(cx, &opened, "Copy session id") && has_label(cx, &opened, "Go to its pane"));
    let mut none = reading();
    {
        let page = none.reader.as_mut().unwrap();
        page.jumps.clear();
        page.find = "zzz".into();
    }
    hear_page(cx, &opened, none);
    assert!(has_label(cx, &opened, "no match"));
}

#[gpui_kit::test]
fn the_reader_says_a_notice_in_place_of_a_transcript_and_notes_the_entries_not_kept(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut notice = reading();
    notice.reader.as_mut().unwrap().session.notice = Some("Its transcript is missing: moved, cleaned up, or never written.".into());
    hear_page(cx, &opened, notice);
    assert!(has_label(cx, &opened, "Its transcript is missing: moved, cleaned up, or never written."));
    let mut earlier = reading();
    earlier.reader.as_mut().unwrap().session.earlier = 3;
    hear_page(cx, &opened, earlier);
    assert!(has_label(cx, &opened, "3 earlier entries not kept: the newest are read"));
}

#[gpui_kit::test]
fn a_turn_jumped_to_scrolls_the_body_to_it_and_a_session_newly_read_opens_at_the_end(cx: &mut TestAppContext) {
    use gpui_kit::px;
    let opened = open(cx);
    let mut long = reading();
    long.reader.as_mut().unwrap().session.entries = (0..80).map(|n| line(if n % 20 == 0 { "user" } else { "agent" }, &format!("Entry {n} of a long talk"))).collect();
    hear_page(cx, &opened, long.clone());
    let view = cx.update(|cx| cx.global::<crate::agents::sessions::Held>().0.clone());
    let body = view.read_with(cx, |view, cx| view.reader.read(cx).body.clone());
    assert!(body.offset().y < px(0.0), "opened at the end: {:?}", body.offset());
    let before = body.offset().y;
    let page = long.reader.as_mut().unwrap();
    page.moves = 1;
    page.target = Some(40);
    hear_page(cx, &opened, long);
    assert!(body.offset().y > before, "entry 40 is nearer the top than the end was: {:?} from {before:?}", body.offset());
}
