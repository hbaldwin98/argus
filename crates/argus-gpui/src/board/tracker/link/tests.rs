//! The link dialog in a window: up exactly while the session holds it, the keys of its fields and of the
//! dialog, what each press sends, and the fields that clear only once the board holds the link.

use argus_desktop_core::board::tracker::link_dialog::{LinkPage, LinkedRow, MapPage};
use argus_desktop_core::connection::page::{Event, Link};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{prelude::*, TestAppContext};

use crate::keys::root::tests::{asked, blur, click_label, drain, has_label, hear, open, overlays, press, type_text, Opened};
use crate::kit::overlay::kind::Overlay;

fn page() -> LinkPage {
    LinkPage { open: true, number: 41, title: "Trim".into(), ..LinkPage::default() }
}

fn link(key: &str, role: &str) -> Link {
    Link { tracker: "GitHub".into(), key: key.into(), url: format!("https://github.com/o/r/issues/{key}"), role: role.into() }
}

fn read(key: &str) -> Option<Link> {
    Some(Link { tracker: "Jira".into(), key: key.into(), url: String::new(), role: String::new() })
}

fn linked(rows: &[(&str, bool)]) -> Vec<LinkedRow> {
    rows.iter().map(|(key, on)| LinkedRow { link: link(key, "the work"), on: *on }).collect()
}

fn show(cx: &mut TestAppContext, opened: &Opened, page: LinkPage) {
    hear(cx, opened, vec![Event::Link(Box::new(page))]);
}

fn up(cx: &mut TestAppContext) -> bool {
    overlays(cx).contains(&Overlay::Link)
}

/// What the dialog asked of the session.
fn sent(opened: &Opened) -> Vec<String> {
    asked(opened).into_iter().filter(|call| call.starts_with("Link {") || call.starts_with("OpenUrl")).collect()
}

/// BOARD-LINK-1, -11: the dialog is up while the session holds it and goes when it does not.
#[gpui_kit::test]
fn the_dialog_is_up_exactly_while_the_session_holds_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert!(!up(cx));
    show(cx, &opened, page());
    assert!(up(cx));
    assert!(has_label(cx, &opened, "Link #41 to a ticket"));
    show(cx, &opened, LinkPage::default());
    assert!(!up(cx));
}

/// BOARD-LINK-4: the keys start in the URL field, and each letter sends both fields' text.
#[gpui_kit::test]
fn typing_sends_both_fields_text_and_starts_in_the_url(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, page());
    drain(&opened);
    type_text(cx, &opened, "ab");
    assert_eq!(sent(&opened), ["Link { gesture: Fields { url: \"a\", role: \"\" } }", "Link { gesture: Fields { url: \"ab\", role: \"\" } }"]);
}

/// BOARD-LINK-5: Enter in a field links with what is typed; Escape closes, and the dialog goes only
/// when the session says so.
#[gpui_kit::test]
fn enter_links_and_escape_closes_but_the_session_decides_when_it_goes(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, page());
    type_text(cx, &opened, "u");
    drain(&opened);
    press(cx, &opened, "enter");
    assert_eq!(sent(&opened), ["Link { gesture: Link { url: \"u\", role: \"\" } }"]);
    drain(&opened);
    press(cx, &opened, "escape");
    assert_eq!(sent(&opened), ["Link { gesture: Close }"]);
    assert!(up(cx), "a refusal is the session's: the dialog stays until it is sent closed");
    show(cx, &opened, LinkPage::default());
    assert!(!up(cx));
}

/// BOARD-LINK-2, -3: what the URL reads as, and why it does not.
#[gpui_kit::test]
fn the_reading_says_the_tracker_and_key_or_why_not(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, page());
    assert!(has_label(cx, &opened, "Link"), "the primary is there, off, before a URL reads");
    show(cx, &opened, LinkPage { read: read("ARG-142"), url: "x".into(), ..page() });
    assert!(has_label(cx, &opened, "Jira key ARG-142"));
    assert!(has_label(cx, &opened, "Link ARG-142"));
    show(cx, &opened, LinkPage { refusal: Some("that is no URL".into()), url: "x".into(), ..page() });
    assert!(has_label(cx, &opened, "that is no URL"));
}

/// BOARD-LINK-2: Link is pressable only while the URL reads and nothing waits.
#[gpui_kit::test]
fn link_sends_only_while_the_url_reads_and_nothing_waits(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, LinkPage { read: read("ARG-142"), ..page() });
    drain(&opened);
    click_label(cx, &opened, "Link ARG-142");
    assert_eq!(sent(&opened), ["Link { gesture: Link { url: \"\", role: \"\" } }"]);
    drain(&opened);
    show(cx, &opened, LinkPage { read: read("ARG-142"), pending: true, ..page() });
    assert!(has_label(cx, &opened, "Linking\u{2026}"));
    click_label(cx, &opened, "Linking\u{2026}");
    assert!(sent(&opened).is_empty(), "a link waiting on the board is not sent again");
}

/// BOARD-LINK-6: the fields empty once the page says the link is on the board and nothing waits.
#[gpui_kit::test]
fn the_fields_clear_once_the_board_holds_the_link(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, page());
    type_text(cx, &opened, "ab");
    show(cx, &opened, LinkPage { url: "ab".into(), read: read("A-1"), pending: true, ..page() });
    show(cx, &opened, LinkPage { linked: linked(&[("A-1", true)]), ..page() });
    drain(&opened);
    type_text(cx, &opened, "z");
    assert_eq!(sent(&opened), ["Link { gesture: Fields { url: \"z\", role: \"\" } }"], "the field was emptied, with the keys still in it");
}

/// BOARD-LINK-6: a refusal keeps what was typed to fix.
#[gpui_kit::test]
fn a_refusal_keeps_what_was_typed(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, page());
    type_text(cx, &opened, "ab");
    show(cx, &opened, LinkPage { url: "ab".into(), read: read("A-1"), pending: true, ..page() });
    show(cx, &opened, LinkPage { url: "ab".into(), read: read("A-1"), pending: false, ..page() });
    drain(&opened);
    type_text(cx, &opened, "z");
    assert_eq!(sent(&opened), ["Link { gesture: Fields { url: \"abz\", role: \"\" } }"]);
}

/// BOARD-LINK-7, -13: a row lists its ticket, and Open and Unlink are sent, the row selected as well.
#[gpui_kit::test]
fn open_and_unlink_act_on_their_row_and_select_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, LinkPage { linked: linked(&[("orion#212", true), ("orion#214", false)]), ..page() });
    assert!(has_label(cx, &opened, "GitHub orion#214"));
    drain(&opened);
    click_label(cx, &opened, "Open orion#214");
    let said = sent(&opened);
    assert!(said.contains(&"OpenUrl { url: \"https://github.com/o/r/issues/orion#214\" }".to_owned()), "{said:?}");
    assert!(said.contains(&"Link { gesture: Select { index: 1 } }".to_owned()), "the press selects the row too: {said:?}");
    drain(&opened);
    click_label(cx, &opened, "Unlink orion#214");
    let said = sent(&opened);
    assert!(said.contains(&"Link { gesture: Unlink { key: \"orion#214\" } }".to_owned()), "{said:?}");
}

/// BOARD-LINK-7: no ticket yet says so.
#[gpui_kit::test]
fn no_ticket_linked_says_so(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, page());
    assert!(has_label(cx, &opened, "Already linked"));
    show(cx, &opened, LinkPage { linked: linked(&[("A-1", true)]), ..page() });
    assert!(has_label(cx, &opened, "GitHub A-1"));
}

/// BOARD-LINK-8: with no field holding the keys, the dialog's keys go to the session.
#[gpui_kit::test]
fn the_keys_outside_a_field_are_the_dialogs(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, LinkPage { linked: linked(&[("A-1", true), ("A-2", false)]), ..page() });
    blur(cx, &opened);
    drain(&opened);
    for (key, action) in [("j", "Next"), ("down", "Next"), ("k", "Previous"), ("o", "OpenTicket"), ("x", "Unlink"), ("m", "TrackerMap"), ("shift-a", "Approve"), ("shift-d", "Discard"), ("escape", "CloseLayer"), ("enter", "Choose")] {
        press(cx, &opened, key);
        let said = drain(&opened);
        assert_eq!(said, [format!("Link {{ gesture: Action {{ action: {action} }} }}")], "{key}");
    }
}

/// BOARD-LINK-14: the stage digits and the window's commands are held and ignored over the dialog; `?`
/// opens the shortcuts over it.
#[gpui_kit::test]
fn the_stage_keys_are_held_and_the_question_mark_opens_the_sheet(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, page());
    blur(cx, &opened);
    drain(&opened);
    press(cx, &opened, "1");
    press(cx, &opened, "2");
    press(cx, &opened, "ctrl-n");
    assert!(sent(&opened).is_empty() && drain(&opened).iter().all(|call| !call.starts_with("Command")), "nothing reached the stage under the dialog");
    crate::keys::root::tests::question_mark(cx, &opened);
    assert!(overlays(cx).contains(&Overlay::Sheet));
    assert!(up(cx), "the dialog is still there under it");
}

/// BOARD-LINK-9: the map's summary: its first six lines, the draft pill, and Edit.
#[gpui_kit::test]
fn the_map_summary_shows_six_lines_the_draft_pill_and_edit(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, page());
    assert!(has_label(cx, &opened, "Edit the map\u{2026}"));
    let body: String = (1..=8).map(|n| format!("line {n}\n\n")).collect();
    let map = MapPage { body, draft: Some("d".into()), draft_by: Some("pane #32".into()) };
    show(cx, &opened, LinkPage { map, ..page() });
    assert!(has_label(cx, &opened, "draft from pane #32 waits"));
    drain(&opened);
    click_label(cx, &opened, "Edit the map\u{2026}");
    assert_eq!(sent(&opened), ["Link { gesture: EditMap }"]);
}

/// BOARD-LINK-10: the map's editor takes the keys on the text it opened with, sends each edit, saves on
/// Ctrl+Enter and cancels the editor, not the dialog, on Escape.
#[gpui_kit::test]
fn the_map_editor_sends_its_text_saves_with_ctrl_enter_and_escape_cancels_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, page());
    let map = MapPage { body: "old".into(), draft: None, draft_by: None };
    show(cx, &opened, LinkPage { map, editing_map: Some("old".into()), ..page() });
    assert!(has_label(cx, &opened, "Save the map"));
    drain(&opened);
    type_text(cx, &opened, "!");
    assert_eq!(sent(&opened), ["Link { gesture: MapText { text: \"old!\" } }"], "the caret is at the end of what it opened with");
    drain(&opened);
    press(cx, &opened, "ctrl-enter");
    assert_eq!(sent(&opened), ["Link { gesture: SaveMap { text: \"old!\" } }"]);
    drain(&opened);
    press(cx, &opened, "escape");
    assert_eq!(sent(&opened), ["Link { gesture: CancelMap }"]);
}

/// BOARD-LINK-10: with a draft waiting the editor says whose and approves as written; the draft can be
/// turned down.
#[gpui_kit::test]
fn a_waiting_draft_is_approved_as_written_or_turned_down(cx: &mut TestAppContext) {
    let opened = open(cx);
    let map = MapPage { body: "old".into(), draft: Some("new".into()), draft_by: Some("pane #32".into()) };
    show(cx, &opened, LinkPage { map, editing_map: Some("new".into()), ..page() });
    assert!(has_label(cx, &opened, "Tracker map \u{b7} draft from pane #32"));
    // The editor is taller than the window: the body scrolls to its buttons.
    let view = cx.update(|cx| cx.global::<crate::board::tracker::install::Dialogs>().link.clone());
    view.read_with(cx, |view, _| view.body.set_offset(gpui_kit::point(gpui_kit::px(0.0), -view.body.max_offset().y)));
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    drain(&opened);
    click_label(cx, &opened, "Approve as written");
    assert_eq!(sent(&opened), ["Link { gesture: SaveMap { text: \"new\" } }"]);
    drain(&opened);
    click_label(cx, &opened, "Turn the draft down");
    assert_eq!(sent(&opened), ["Link { gesture: DiscardDraft }"]);
}

/// BOARD-LINK-2, -13: Cancel and the close button send `Close`.
#[gpui_kit::test]
fn cancel_and_the_close_button_send_close(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, page());
    drain(&opened);
    click_label(cx, &opened, "Cancel");
    click_label(cx, &opened, "Close");
    assert_eq!(sent(&opened), ["Link { gesture: Close }", "Link { gesture: Close }"]);
    assert!(up(cx), "only the session closes it");
}

/// BOARD-LINK-4: a dialog that opens straight on the map editor (the `M` path) gives the keys to the
/// map's box, not the URL field.
#[gpui_kit::test]
fn a_dialog_opened_on_the_map_editor_takes_the_keys_in_the_map(cx: &mut TestAppContext) {
    let opened = open(cx);
    let map = MapPage { body: "old".into(), draft: None, draft_by: None };
    show(cx, &opened, LinkPage { map, editing_map: Some("old".into()), ..page() });
    drain(&opened);
    type_text(cx, &opened, "!");
    assert_eq!(sent(&opened), ["Link { gesture: MapText { text: \"old!\" } }"], "the typing landed in the map, not the URL");
}
