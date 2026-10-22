//! The reconcile dialog in a window: up exactly while the session holds it, the ask and its agent, the
//! lines grouped as Rust lists them with their ticks, the keys of the dialog and of its two boxes, what
//! each press sends, the words of the line open that a push never overwrites, and the lines list that keeps
//! its scroll.

use argus_desktop_core::board::tracker::reconcile_dialog::{AgentChoice, LineGroup, ProposedRow, ReconcilePage, SelectedLine};
use argus_desktop_core::connection::page::{Event, Line};
use gpui_kit::TestAppContext;

use crate::board::tracker::install::Dialogs;
use crate::keys::root::tests::{asked, blur, click_label, drain, has_label, hear, open, overlays, press, type_text, Opened};
use crate::kit::overlay::kind::Overlay;

fn row(line: usize, key: &str, does: &str, ticked: bool, on: bool) -> ProposedRow {
    ProposedRow { line, key: key.into(), does: does.into(), why: format!("why {key}"), ticked, on, failed: None }
}

fn selected(line: usize, key: &str, text: &str, call: &str) -> SelectedLine {
    SelectedLine { line, heading: "Out to GitHub".into(), key: key.into(), does: "comment progress".into(), text: text.into(), edited: false, call: call.into(), ticked: false, failed: None }
}

/// A proposal of three lines under two headings, the first line open.
fn proposed() -> ReconcilePage {
    ReconcilePage {
        open: true,
        number: 41,
        title: "Trim".into(),
        tickets: "GitHub orion#212 and GitHub orion#214".into(),
        stage: "proposed",
        by: Some("pane #32".into()),
        ago: Some("15m".into()),
        groups: vec![
            LineGroup { heading: "Out to GitHub".into(), into_argus: false, lines: vec![row(0, "orion#212", "comment progress", false, true), row(1, "orion#214", "close as done", false, false)] },
            LineGroup { heading: "Into Argus".into(), into_argus: true, lines: vec![row(2, "task 5", "add a subtask", false, false)] },
        ],
        selected: Some(selected(0, "orion#212", "the brief route is in", "gh issue comment 212")),
        total: 3,
        ..ReconcilePage::default()
    }
}

fn nothing_under_way(agents: &[(u64, &str, bool, bool)]) -> ReconcilePage {
    let agents = agents.iter().map(|(pane, title, last, chosen)| AgentChoice { pane: *pane, title: (*title).into(), last: *last, chosen: *chosen }).collect();
    ReconcilePage { open: true, number: 41, title: "Trim".into(), stage: "none", agents, ..ReconcilePage::default() }
}

fn show(cx: &mut TestAppContext, opened: &Opened, page: ReconcilePage) {
    hear(cx, opened, vec![Event::Reconcile(Box::new(page))]);
}

fn up(cx: &mut TestAppContext) -> bool {
    overlays(cx).contains(&Overlay::Reconcile)
}

fn sent(opened: &Opened) -> Vec<String> {
    asked(opened).into_iter().filter(|call| call.starts_with("Reconcile {")).collect()
}

/// BOARD-RECONCILE-2, -17: up while the session holds it; the heading names the tickets.
#[gpui_kit::test]
fn the_dialog_is_up_exactly_while_the_session_holds_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert!(!up(cx));
    show(cx, &opened, proposed());
    assert!(up(cx));
    assert!(has_label(cx, &opened, "Reconcile #41 with GitHub orion#212 and GitHub orion#214"));
    show(cx, &opened, ReconcilePage::default());
    assert!(!up(cx));
    show(cx, &opened, ReconcilePage { tickets: String::new(), ..proposed() });
    assert!(has_label(cx, &opened, "Reconcile #41"), "with no ticket the heading names none");
}

/// BOARD-RECONCILE-3, -4, -5: with nothing under way the ask: the agents as a radio group, Ctrl+Enter or
/// the button sends it, and with no agent the button is off and the dialog says why.
#[gpui_kit::test]
fn the_ask_goes_to_the_agent_chosen_and_sends_what_was_typed(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, nothing_under_way(&[(7, "claude \u{b7} trim", true, true), (8, "codex", false, false)]));
    assert!(has_label(cx, &opened, "The agent to ask"));
    click_label(cx, &opened, "What to reconcile");
    type_text(cx, &opened, "go");
    drain(&opened);
    click_label(cx, &opened, "codex");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Agent { pane: 8 } }"]);
    drain(&opened);
    click_label(cx, &opened, "Send the ask");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Ask { ask: \"go\" } }"]);
    drain(&opened);
    click_label(cx, &opened, "What to reconcile");
    press(cx, &opened, "ctrl-enter");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Ask { ask: \"go\" } }"]);
}

#[gpui_kit::test]
fn with_no_agent_the_ask_cannot_be_sent_and_says_so(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, nothing_under_way(&[]));
    assert!(has_label(cx, &opened, "No agent runs in this repository. Start one, then ask it here."));
    drain(&opened);
    click_label(cx, &opened, "Send the ask");
    assert!(sent(&opened).is_empty());
}

/// BOARD-RECONCILE-3: once asked, the waiting line is over the same form.
#[gpui_kit::test]
fn an_ask_waiting_says_who_is_reading_and_can_be_taken_back(cx: &mut TestAppContext) {
    let opened = open(cx);
    let page = ReconcilePage { stage: "asked", by: Some("pane #32".into()), ago: Some("1m".into()), asked: "sync it".into(), ..nothing_under_way(&[(7, "claude", false, true)]) };
    show(cx, &opened, page);
    assert!(has_label(cx, &opened, "pane #32 is reading the map, the feature and the tickets; its proposal shows here when it sends one."));
    assert!(has_label(cx, &opened, "sync it"));
    drain(&opened);
    click_label(cx, &opened, "Take the ask back");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Discard }"]);
}

/// BOARD-RECONCILE-6, -7: the lines under their headings in Rust's order; a press on a row selects it and a
/// press on its box ticks it and does not also select.
#[gpui_kit::test]
fn a_row_selects_and_its_box_ticks_without_selecting(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, proposed());
    assert!(has_label(cx, &opened, "Out to GitHub") && has_label(cx, &opened, "Into Argus"));
    drain(&opened);
    click_label(cx, &opened, "orion#214 close as done");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Select { line: 1 } }"]);
    drain(&opened);
    click_label(cx, &opened, "Tick orion#214 close as done");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Tick { line: 1 } }"], "the box ticks and stops there");
}

/// BOARD-RECONCILE-6: while the agent carries them out the boxes cannot be pressed.
#[gpui_kit::test]
fn while_carrying_the_boxes_are_off_and_the_dialog_says_who_carries(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = proposed();
    page.stage = "carrying";
    page.ticked = 1;
    page.groups[0].lines[0].ticked = true;
    show(cx, &opened, page);
    assert!(has_label(cx, &opened, "pane #32 is carrying out the 1 you ticked; nothing more is ticked until it reports."));
    drain(&opened);
    click_label(cx, &opened, "Tick orion#214 close as done");
    assert!(sent(&opened).iter().all(|call| !call.contains("Tick")), "{:?}", sent(&opened));
    assert!(!has_label(cx, &opened, "Carry out the 1 ticked"), "carrying out is done once");
}

/// BOARD-RECONCILE-6: a line that failed carries the pill.
#[gpui_kit::test]
fn a_line_that_failed_says_so_and_why(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = proposed();
    page.groups[0].lines[1].failed = Some("404 from the tracker".into());
    show(cx, &opened, page);
    assert!(has_label(cx, &opened, "failed"));
}

/// BOARD-RECONCILE-8, -20: what the agent noticed shows only when it noticed anything.
#[gpui_kit::test]
fn what_was_noticed_is_listed_only_when_any(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, proposed());
    assert!(!has_label(cx, &opened, "Noticed, not proposed"));
    show(cx, &opened, ReconcilePage { noticed: vec![Line { label: "orion#212".into(), text: "a comment asks".into(), aside: None }], ..proposed() });
    assert!(has_label(cx, &opened, "Noticed, not proposed"));
}

/// BOARD-RECONCILE-9, -20: the line open in full; the call only when there is one; none open says the
/// agent proposed nothing.
#[gpui_kit::test]
fn the_line_open_shows_its_call_and_none_open_says_nothing_proposed(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, proposed());
    assert!(has_label(cx, &opened, "gh issue comment 212"));
    assert!(has_label(cx, &opened, "Afterwards"));
    let mut quiet = proposed();
    quiet.selected = Some(selected(0, "orion#212", "words", ""));
    show(cx, &opened, quiet);
    assert!(!has_label(cx, &opened, "What the agent will do"));
    show(cx, &opened, ReconcilePage { selected: None, groups: Vec::new(), ..proposed() });
    assert!(has_label(cx, &opened, "The agent noticed things, and proposes nothing."), "no line open");
}

/// BOARD-RECONCILE-11: the foot counts the ticks; carrying out is pressable only with some, and Later and
/// Discard are the dialog's ways out.
#[gpui_kit::test]
fn the_foot_counts_ticks_and_carries_out_only_what_is_ticked(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, proposed());
    assert!(has_label(cx, &opened, "Carry out the 0 ticked"));
    drain(&opened);
    click_label(cx, &opened, "Carry out the 0 ticked");
    assert!(sent(&opened).is_empty(), "nothing ticked, nothing to carry out");
    let mut page = proposed();
    page.ticked = 2;
    show(cx, &opened, page);
    click_label(cx, &opened, "Carry out the 2 ticked");
    assert_eq!(sent(&opened), ["Reconcile { gesture: CarryOut }"]);
    drain(&opened);
    click_label(cx, &opened, "Discard proposal");
    click_label(cx, &opened, "Later");
    click_label(cx, &opened, "Close");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Discard }", "Reconcile { gesture: Close }", "Reconcile { gesture: Close }"]);
    assert!(up(cx), "only the session closes it");
}

/// BOARD-RECONCILE-12: with no field holding the keys, the dialog's keys go to the session.
#[gpui_kit::test]
fn the_keys_outside_a_box_are_the_dialogs(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, proposed());
    blur(cx, &opened);
    drain(&opened);
    for (key, action) in [("j", "Next"), ("down", "Next"), ("k", "Previous"), ("space", "Tick"), ("enter", "CarryOut"), ("shift-d", "Discard"), ("g", "GoToPane"), ("escape", "CloseLayer")] {
        press(cx, &opened, key);
        assert_eq!(drain(&opened), [format!("Reconcile {{ gesture: Action {{ action: {action} }} }}")], "{key}");
    }
}

/// BOARD-RECONCILE-10, -12: `e` puts the keys in the open line's words with the caret at their end, each
/// keystroke is sent with the line, and Escape gives the keys back to the dialog and closes nothing.
#[gpui_kit::test]
fn e_edits_the_words_and_escape_leaves_the_box_without_closing(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, proposed());
    blur(cx, &opened);
    drain(&opened);
    press(cx, &opened, "e");
    assert!(sent(&opened).is_empty(), "`e` is the page's own: nothing is sent");
    type_text(cx, &opened, "!");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Text { line: 0, text: \"the brief route is in!\" } }"]);
    drain(&opened);
    press(cx, &opened, "escape");
    assert!(sent(&opened).is_empty(), "Escape in the box closes nothing");
    press(cx, &opened, "j");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Action { action: Next } }"], "the dialog has its keys back");
}

/// BOARD-RECONCILE-9: the words are read-only unless proposed, and `e` does nothing there.
#[gpui_kit::test]
fn the_words_cannot_be_edited_once_carrying(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut page = proposed();
    page.stage = "carrying";
    show(cx, &opened, page);
    blur(cx, &opened);
    drain(&opened);
    press(cx, &opened, "e");
    type_text(cx, &opened, "!");
    assert!(sent(&opened).iter().all(|call| !call.contains("Text")), "{:?}", sent(&opened));
}

/// BOARD-RECONCILE-10: the words typed are the box's own while the same line stays open, and another line
/// open shows its own.
#[gpui_kit::test]
fn a_push_never_overwrites_the_words_being_written(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, proposed());
    blur(cx, &opened);
    press(cx, &opened, "e");
    type_text(cx, &opened, "b");
    // The session's echo of the words as they were before the letter.
    show(cx, &opened, proposed());
    drain(&opened);
    type_text(cx, &opened, "c");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Text { line: 0, text: \"the brief route is inbc\" } }"]);
    let mut other = proposed();
    other.selected = Some(selected(1, "orion#214", "zzz", "gh issue close 214"));
    show(cx, &opened, other);
    drain(&opened);
    type_text(cx, &opened, "!");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Text { line: 1, text: \"zzz!\" } }"], "another line shows its own words");
}

/// BOARD-RECONCILE-5: Escape in the ask gives the dialog its keys back and does not close it.
#[gpui_kit::test]
fn escape_in_the_ask_leaves_the_box_without_closing(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, nothing_under_way(&[(7, "claude", false, true)]));
    click_label(cx, &opened, "What to reconcile");
    drain(&opened);
    press(cx, &opened, "escape");
    assert!(sent(&opened).is_empty());
    press(cx, &opened, "shift-d");
    assert_eq!(sent(&opened), ["Reconcile { gesture: Action { action: Discard } }"]);
}

fn many() -> ReconcilePage {
    let lines: Vec<ProposedRow> = (0..40).map(|n| row(n, &format!("orion#{n}"), "comment progress", false, n == 0)).collect();
    let total = lines.len();
    ReconcilePage { groups: vec![LineGroup { heading: "Out to GitHub".into(), into_argus: false, lines }], total, ..proposed() }
}


/// BOARD-RECONCILE-16: a push does not reset the lines list's scroll.
#[gpui_kit::test]
fn a_push_keeps_the_lines_list_where_it_was_scrolled(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, many());
    let view = cx.update(|cx| cx.global::<Dialogs>().reconcile.clone());
    let scroll = view.read_with(cx, |view, _| view.lines.clone());
    assert!(scroll.max_offset().y.as_f32() > 0.0, "forty lines overflow the list");
    scroll.set_offset(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(-200.0)));
    let mut again = many();
    again.ticked = 1;
    again.groups[0].lines[3].ticked = true;
    show(cx, &opened, again);
    assert_eq!(scroll.offset().y.as_f32(), -200.0);
}

/// BOARD-RECONCILE-16: the line under the cursor is brought into view by the least movement when it moves.
#[gpui_kit::test]
fn the_cursor_line_is_brought_into_view_when_it_moves(cx: &mut TestAppContext) {
    let opened = open(cx);
    show(cx, &opened, many());
    let view = cx.update(|cx| cx.global::<Dialogs>().reconcile.clone());
    let scroll = view.read_with(cx, |view, _| view.lines.clone());
    let mut far = many();
    far.groups[0].lines[0].on = false;
    far.groups[0].lines[35].on = true;
    far.selected = Some(selected(35, "orion#35", "words", "call"));
    show(cx, &opened, far);
    assert!(scroll.offset().y.as_f32() < 0.0, "the list scrolled down to the line");
}
