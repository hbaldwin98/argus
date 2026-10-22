//! The simple blocks, each drawn from what an agent wrote: what a reader is told of it, and what a
//! press in it asks Rust for.

use argus_desktop_core::connection::page::Call;
use gpui_kit::{point, px, ScrollDelta, TestAppContext};
use serde_json::json;

use super::*;
use crate::views::memory::LogPlace;

#[gpui_kit::test]
fn a_callout_says_its_words_and_a_checklist_item_says_whether_it_is_done(cx: &mut TestAppContext) {
    let rig = open(
        cx,
        700.0,
        json!([
            {"kind": "callout", "tone": "warn", "text": "Decision #3 is open"},
            {"kind": "checklist", "items": [{"label": "Add a route", "done": true}, {"label": "Wire it", "done": false, "note": "soon"}]}
        ]),
    );
    assert_eq!(label(cx, &rig, "7/v.0-callout").as_deref(), Some("Decision #3 is open"));
    assert_eq!(label(cx, &rig, "7/v.1-check0").as_deref(), Some("Done: Add a route"));
    assert_eq!(label(cx, &rig, "7/v.1-check1").as_deref(), Some("Not done: Wire it"));
}

#[gpui_kit::test]
fn a_matrix_cell_is_titled_by_its_row_and_column_and_a_missing_cell_is_not_drawn(cx: &mut TestAppContext) {
    let rig = open(
        cx,
        700.0,
        json!([{"kind": "matrix", "rows": ["Linux"], "columns": ["build", "test"], "cells": [[{"tone": "warn", "weight": 0.6, "label": "1 flaky"}]]}]),
    );
    assert_eq!(label(cx, &rig, "7/v.0-matrix-0-0").as_deref(), Some("Linux \u{b7} build: 1 flaky"));
    assert!(!exists(cx, &rig, "7/v.0-matrix-0-1"), "an agent that sent no cell leaves it empty");
}

#[gpui_kit::test]
fn a_code_blocks_copy_hands_its_text_to_rust(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, json!([{"kind": "code", "lang": "rust", "text": "fn main() {}\n"}]));
    click(cx, &rig, "7/v.0|copy");
    assert_eq!(calls(&rig), [said(Call::CopyText { text: "fn main() {}\n".into() })]);
}

#[gpui_kit::test]
fn a_progress_block_says_the_agents_note_else_its_percentage(cx: &mut TestAppContext) {
    let rig = open(
        cx,
        700.0,
        json!([
            {"kind": "progress", "label": "Tests", "value": 412, "max": 1000, "note": "412 / 1,000"},
            {"kind": "progress", "label": "Overshot", "value": 12, "max": 10}
        ]),
    );
    let value = |cx: &mut TestAppContext, id: &str| cx.update_window(rig.handle, |_, window, _| window.find(id.to_owned()).value().map(str::to_owned)).unwrap();
    assert_eq!(value(cx, "7/v.0-progress").as_deref(), Some("412 / 1,000"));
    assert_eq!(value(cx, "7/v.1-progress").as_deref(), Some("100%"), "an overshoot is a full bar, not a broken one");
}

#[gpui_kit::test]
fn a_stat_that_opens_a_saved_view_asks_rust_to_open_it_from_the_column_it_is_in(cx: &mut TestAppContext) {
    let from = argus_desktop_core::views::saved::SavedColumn { panel: argus_desktop_core::layout::docks::Panel::Saved("home".into()), column: 1 };
    let ctx = ViewCtx { saved: Some(from.clone()), ..ViewCtx::agent(7, "v") };
    let rig = open_as(cx, 700.0, json!([{"kind": "stats", "stats": [{"label": "Failing", "value": "3", "opens": "ci"}]}]), ctx);
    click(cx, &rig, "7/v.0-stat0-opens");
    assert_eq!(calls(&rig), [said(Call::OpenSavedView { id: "ci".into(), from: Some(from) })]);
}

#[gpui_kit::test]
fn a_mark_for_a_view_the_window_does_not_hold_is_still_pressed_since_it_may_be_saved_later(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, json!([{"kind": "section", "heading": "Later", "opens": "nothing-yet", "blocks": []}]));
    assert_eq!(label(cx, &rig, "7/v.0-opens").as_deref(), Some("Open the saved view nothing-yet"));
    click(cx, &rig, "7/v.0-opens");
    assert_eq!(calls(&rig), [said(Call::OpenSavedView { id: "nothing-yet".into(), from: None })]);
}

#[gpui_kit::test]
fn a_list_item_with_a_person_is_a_comment_and_one_without_is_an_event(cx: &mut TestAppContext) {
    let rig = open(
        cx,
        700.0,
        json!([{"kind": "list", "items": [{"who": "Ada Lovelace", "title": "Looks right"}, {"title": "Merged"}]}]),
    );
    let (comment, event) = (bounds(cx, &rig, "7/v.0-item0").unwrap(), bounds(cx, &rig, "7/v.0-item1").unwrap());
    assert!(exists(cx, &rig, "avatar-Ada Lovelace"));
    assert!(event.origin.y > comment.origin.y);
    assert!(comment.size.height > event.size.height, "a comment has its person's line above the title; an event is one line");
}

fn lines(n: usize) -> serde_json::Value {
    json!([{"kind": "log", "lines": (0..n).map(|k| json!({"level": "info", "text": format!("line {k}")})).collect::<Vec<_>>()}])
}

fn place(rig: &Rig) -> Option<LogPlace> {
    rig.memory.borrow().log("7/v.0")
}

// BLOCKS-9: a log is a tail.
#[gpui_kit::test]
fn a_log_nobody_scrolled_up_in_follows_the_newest_line(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, lines(60));
    let before = place(&rig).expect("remembered once laid out");
    assert!(before.tail && before.top > 0.0, "{before:?}");
    show(cx, &rig, lines(90), ViewCtx { can_press: true, ..ViewCtx::agent(7, "v") });
    let after = place(&rig).unwrap();
    assert!(after.tail && after.top > before.top, "it followed the new lines: {before:?} then {after:?}");
}

#[gpui_kit::test]
fn a_log_the_person_scrolled_up_in_keeps_its_place_when_lines_are_added(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, lines(60));
    cx.update_window(rig.handle, |_, window, cx| window.scroll("7/v.0-log".to_owned(), ScrollDelta::Pixels(point(px(0.0), px(300.0))), cx)).unwrap();
    settle(cx, &rig);
    let held = place(&rig).unwrap();
    assert!(!held.tail, "scrolled up from the end: {held:?}");
    show(cx, &rig, lines(90), ViewCtx { can_press: true, ..ViewCtx::agent(7, "v") });
    let after = place(&rig).unwrap();
    assert!(!after.tail && (after.top - held.top).abs() < 1.0, "kept its place across the redraw: {held:?} then {after:?}");
}

// BLOCKS-29: the wheel chains. A box takes it while it can move that way and hands it to the page
// around it when it cannot; a box with nowhere to go does not capture it at all.
fn tall_page_with(block: serde_json::Value) -> serde_json::Value {
    let mut blocks = vec![block];
    blocks.extend((0..30).map(|k| json!({"kind": "callout", "tone": "info", "text": format!("note {k}")})));
    json!(blocks)
}

fn wheel(cx: &mut TestAppContext, rig: &Rig, id: &str, y: f32) {
    cx.update_window(rig.handle, |_, window, cx| window.scroll(id.to_owned(), ScrollDelta::Pixels(point(px(0.0), px(y))), cx)).unwrap();
    settle(cx, rig);
}

#[gpui_kit::test]
fn a_log_at_its_end_hands_the_wheel_to_the_page_and_one_that_can_move_keeps_it(cx: &mut TestAppContext) {
    let block = json!({"kind": "log", "lines": (0..60).map(|k| json!({"level": "info", "text": format!("line {k}")})).collect::<Vec<_>>()});
    let (rig, page) = open_in_page(cx, 700.0, tall_page_with(block));
    assert!(page.max_offset().y > px(0.0), "the page has room to scroll");
    wheel(cx, &rig, "7/v.0-log", 100.0);
    assert_eq!(page.offset().y, px(0.0), "the log could move up, so it kept the wheel and the page stayed");
    assert!(!place(&rig).unwrap().tail, "and it moved");
    wheel(cx, &rig, "7/v.0-log", -100.0);
    assert_eq!(page.offset().y, px(0.0), "back down it is the log that moves");
    assert!(place(&rig).unwrap().tail);
    wheel(cx, &rig, "7/v.0-log", -200.0);
    assert!(page.offset().y < px(0.0), "at its end the log hands the wheel on, and the page took it: {:?}", page.offset());
}

#[gpui_kit::test]
fn a_code_block_with_nothing_to_scroll_does_not_take_the_wheel(cx: &mut TestAppContext) {
    let (rig, page) = open_in_page(cx, 700.0, tall_page_with(json!({"kind": "code", "lang": "rust", "text": "fn main() {}\n"})));
    wheel(cx, &rig, "7/v.0-code-body", -200.0);
    assert!(page.offset().y < px(0.0), "a short code block cannot move, so the page scrolls: {:?}", page.offset());
}

#[gpui_kit::test]
fn a_log_too_short_to_scroll_does_not_take_the_wheel(cx: &mut TestAppContext) {
    let (rig, page) = open_in_page(cx, 700.0, tall_page_with(lines(2)[0].clone()));
    wheel(cx, &rig, "7/v.0-log", -200.0);
    assert!(page.offset().y < px(0.0), "{:?}", page.offset());
}
