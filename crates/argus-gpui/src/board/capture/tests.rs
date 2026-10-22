//! The note a new feature is written as, in the whole window: what it draws of the session's page, what
//! typing and the keys send, and what a push does to the field. Rules cited are `gpui/parity/board.md`'s.

use argus_desktop_core::board::capture_note::CapturePage;
use argus_desktop_core::connection::page::Event;
use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, TestAppContext};

use crate::board::snapshots;
use crate::checkouts::stage::tests::click_id;
use crate::keys::root::tests::{blur, click_label, drain, has_label, hear, press, type_text, Opened};

fn on(cx: &mut TestAppContext) -> Opened {
    snapshots::on(cx, "capture")
}

fn page(opened: &Opened, cx: &mut TestAppContext) -> CapturePage {
    cx.update(|cx| opened.heard.read(cx).capture.as_deref().cloned().expect("a note is open"))
}

fn heard(page: CapturePage) -> Vec<Event> {
    vec![Event::Capture(Box::new(page))]
}

/// What the field says now.
fn field(opened: &Opened, cx: &mut TestAppContext) -> String {
    cx.update_window(opened.handle, |_, window, _| window.find_by_label("Feature note").value().map(str::to_owned)).unwrap().unwrap_or_default()
}

fn note_call(text: &str) -> String {
    format!("Capture {{ gesture: Note {{ note: {text:?} }} }}")
}

fn create_call(text: &str) -> String {
    format!("Capture {{ gesture: Create {{ note: {text:?} }} }}")
}

const DISCARD: &str = "Capture { gesture: Discard }";

// BOARD-CAPTURE-2, -3, -5, -11: the head, the field, the legend and the reading.
#[gpui_kit::test]
fn the_note_draws_its_head_its_field_its_legend_and_what_it_reads_as(cx: &mut TestAppContext) {
    let opened = on(cx);
    for label in ["Discard", "Create", "Feature note", "The note", "What the note reads as", "ready to create"] {
        assert!(has_label(cx, &opened, label), "{label}");
    }
    assert!(has_label(cx, &opened, "What the first characters of a line mean"), "the legend");
    let note = page(&opened, cx);
    assert_eq!(field(&opened, cx), note.note, "the field says what the session last heard");
    cx.update_window(opened.handle, |_, window, _| {
        assert!(window.find_by_label("Feature note").focused().unwrap_or(false), "the field takes the keys when drawn (BOARD-CAPTURE-3)");
    })
    .unwrap();
}

// BOARD-CAPTURE-4: every edit sends the whole note as it reads.
#[gpui_kit::test]
fn typing_sends_the_whole_note_as_it_now_reads(cx: &mut TestAppContext) {
    let opened = on(cx);
    let before = page(&opened, cx).note;
    type_text(cx, &opened, "!");
    assert_eq!(drain(&opened), [note_call(&format!("{before}!"))]);
}

// BOARD-CAPTURE-7: Ctrl Enter creates with the text as it is, Escape discards, a plain Enter is a newline.
#[gpui_kit::test]
fn ctrl_enter_creates_with_the_field_as_it_is_and_a_plain_enter_is_a_newline(cx: &mut TestAppContext) {
    let opened = on(cx);
    let before = page(&opened, cx).note;
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), [note_call(&format!("{before}\n"))], "a newline, and nothing created");
    type_text(cx, &opened, "x");
    drain(&opened);
    press(cx, &opened, "ctrl-enter");
    assert_eq!(drain(&opened), [create_call(&format!("{before}\nx"))], "the text read from the field when pressed");
}

#[gpui_kit::test]
fn escape_discards_the_note_from_the_field_and_from_anywhere_else(cx: &mut TestAppContext) {
    let opened = on(cx);
    press(cx, &opened, "escape");
    assert_eq!(drain(&opened), [DISCARD], "from the field (Writing)");
    blur(cx, &opened);
    press(cx, &opened, "escape");
    assert_eq!(drain(&opened), [DISCARD], "from the stage, where the Capture layer puts it away");
}

// BOARD-CAPTURE-12: the button and the crumb discard too.
#[gpui_kit::test]
fn discard_the_button_and_the_crumb_all_put_the_note_away(cx: &mut TestAppContext) {
    let opened = on(cx);
    click_label(cx, &opened, "Discard");
    assert_eq!(drain(&opened), [DISCARD]);
    click_id(cx, &opened, "capture-crumb");
    assert_eq!(drain(&opened), [DISCARD], "the crumb that says where it goes back to");
}

// BOARD-CAPTURE-8, -9: the button creates with the field's text, and waits while blocked or creating.
#[gpui_kit::test]
fn create_sends_the_note_once_it_can_be_made_and_waits_while_it_cannot_or_is_being(cx: &mut TestAppContext) {
    let opened = on(cx);
    let note = page(&opened, cx);
    click_label(cx, &opened, "Create");
    assert_eq!(drain(&opened), [create_call(&note.note)]);
    hear(cx, &opened, heard(CapturePage { blocked: Some("a task needs words".into()), ..note.clone() }));
    assert!(has_label(cx, &opened, "a task needs words"), "the verdict says why it cannot");
    click_label(cx, &opened, "Create");
    assert!(drain(&opened).is_empty(), "a note that is blocked sends nothing from its button");
    hear(cx, &opened, heard(CapturePage { creating: true, blocked: None, ..note }));
    assert!(has_label(cx, &opened, "Creating\u{2026}") && has_label(cx, &opened, "creating\u{2026}"), "the button and the verdict say it is being made");
    click_label(cx, &opened, "Creating\u{2026}");
    assert!(drain(&opened).is_empty());
}

// BOARD-CAPTURE-11: a line the grammar could not place is named.
#[gpui_kit::test]
fn a_line_the_grammar_could_not_place_is_named_with_how_a_line_starts(cx: &mut TestAppContext) {
    let opened = on(cx);
    let note = page(&opened, cx);
    hear(cx, &opened, heard(CapturePage { unread: vec![3, 5], ..note.clone() }));
    assert!(has_label(cx, &opened, "Lines 3, 5 read as nothing. Start a task with -, a question with ?, a decision made with !."));
    hear(cx, &opened, heard(CapturePage { unread: vec![4], ..note }));
    assert!(has_label(cx, &opened, "Line 4 reads as nothing. Start a task with -, a question with ?, a decision made with !."));
}

// BOARD-CAPTURE-11: an empty note says what it is waiting for.
#[gpui_kit::test]
fn an_empty_note_says_it_is_waiting_for_a_title(cx: &mut TestAppContext) {
    let opened = on(cx);
    let empty = CapturePage { open: true, repository: "argus".into(), note: String::new(), blocked: Some("write a title on the first line".into()), ..CapturePage::default() };
    hear(cx, &opened, heard(empty));
    assert!(has_label(cx, &opened, "write a title on the first line"));
    assert!(!has_label(cx, &opened, "ready to create"));
}

// BOARD-CAPTURE-13: the field is never replaced while it is typed in; it takes the session's word when it is not.
#[gpui_kit::test]
fn a_reading_does_not_replace_what_is_being_typed_but_a_note_from_elsewhere_is_shown(cx: &mut TestAppContext) {
    let opened = on(cx);
    let note = page(&opened, cx);
    type_text(cx, &opened, "z");
    let typed = field(&opened, cx);
    hear(cx, &opened, heard(CapturePage { note: "an older echo".into(), ..note.clone() }));
    assert_eq!(field(&opened, cx), typed, "the text being typed wins over Rust's echo of it");
    blur(cx, &opened);
    hear(cx, &opened, heard(CapturePage { note: "a note sent again".into(), ..note }));
    assert_eq!(field(&opened, cx), "a note sent again", "a field nobody is typing in follows the session");
}

// BOARD-CAPTURE-6: a number for each line, as tall as the rows the line takes once wrapped.
#[gpui_kit::test]
fn each_line_has_a_number_as_tall_as_the_rows_it_wraps_to(cx: &mut TestAppContext) {
    let opened = on(cx);
    let long = "word ".repeat(120);
    let text = format!("Title\n{long}\nend");
    let note = page(&opened, cx);
    blur(cx, &opened);
    hear(cx, &opened, heard(CapturePage { note: text, ..note }));
    // The field is laid out at its width on a frame of its own, which the numbers follow.
    for _ in 0..2 {
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        cx.run_until_parked();
    }
    let heights: Vec<f32> = cx
        .update_window(opened.handle, |_, window, _| (1..=3usize).map(|line| window.find(("capture-line", line)).bounds().size.height.as_f32()).collect())
        .unwrap();
    assert!(heights[1] > heights[0] * 1.9, "a line that wraps takes several rows: {heights:?}");
    assert!((heights[0] - heights[2]).abs() < 0.5, "a line that does not is one row: {heights:?}");
}

#[gpui_kit::test]
fn the_numbers_are_as_many_as_the_lines_and_at_least_one(cx: &mut TestAppContext) {
    let opened = on(cx);
    let note = page(&opened, cx);
    let lines = note.note.split('\n').count();
    cx.update_window(opened.handle, |_, window, _| {
        assert!(window.try_find(("capture-line", lines)).is_some() && window.try_find(("capture-line", lines + 1)).is_none());
    })
    .unwrap();
    blur(cx, &opened);
    hear(cx, &opened, heard(CapturePage { note: String::new(), ..note }));
    cx.update_window(opened.handle, |_, window, _| assert!(window.try_find(("capture-line", 1usize)).is_some() && window.try_find(("capture-line", 2usize)).is_none())).unwrap();
}

// BOARD-CAPTURE-10: once the board holds the feature the note closes.
#[gpui_kit::test]
fn the_note_goes_when_the_session_says_it_is_shut(cx: &mut TestAppContext) {
    let opened = on(cx);
    let note = page(&opened, cx);
    hear(cx, &opened, heard(CapturePage { open: false, ..note }));
    assert!(!has_label(cx, &opened, "Feature note"));
}
