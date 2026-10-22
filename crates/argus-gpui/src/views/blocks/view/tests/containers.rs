//! The blocks that hold blocks: tabs, details, trees, columns and a timeline's lanes, and what the
//! window keeps of each across the agent showing the view again.

use gpui_kit::TestAppContext;
use serde_json::{json, Value};

use super::*;

fn note(text: &str) -> Value {
    json!({"kind": "callout", "tone": "info", "text": text})
}

fn agent() -> ViewCtx {
    ViewCtx { can_press: true, ..ViewCtx::agent(7, "v") }
}

fn tabs() -> Value {
    json!([{"kind": "tabs", "tabs": [{"title": "One", "blocks": [note("first")]}, {"title": "Two", "blocks": [note("second")]}]}])
}

// BLOCKS-2 (tabs kept per path), CROSS-13.
#[gpui_kit::test]
fn the_tab_the_person_chose_survives_the_agent_showing_the_view_again(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, tabs());
    assert!(exists(cx, &rig, "7/v.0.0.0-callout"), "the first tab is the one shown until one is chosen");
    assert!(!exists(cx, &rig, "7/v.0.1.0-callout"));
    click(cx, &rig, "7/v.0-tab1");
    assert!(exists(cx, &rig, "7/v.0.1.0-callout") && !exists(cx, &rig, "7/v.0.0.0-callout"));
    show(cx, &rig, tabs(), agent());
    assert!(exists(cx, &rig, "7/v.0.1.0-callout"), "a republish must not put the tab back");
}

#[gpui_kit::test]
fn a_view_that_comes_again_with_fewer_tabs_shows_its_last(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, tabs());
    click(cx, &rig, "7/v.0-tab1");
    show(cx, &rig, json!([{"kind": "tabs", "tabs": [{"title": "Only", "blocks": [note("only")]}]}]), agent());
    assert!(exists(cx, &rig, "7/v.0.0.0-callout"), "held to the tabs there are now, not to none");
}

#[gpui_kit::test]
fn the_tab_chosen_is_the_one_whose_tab_was_pressed_and_marks_itself_selected(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, tabs());
    let selected = |cx: &mut TestAppContext, id: &str| {
        cx.update_window(rig.handle, |_, window, _| window.find(id.to_owned()).selected()).unwrap()
    };
    assert_eq!((selected(cx, "7/v.0-tab0"), selected(cx, "7/v.0-tab1")), (Some(true), Some(false)));
    click(cx, &rig, "7/v.0-tab1");
    assert_eq!((selected(cx, "7/v.0-tab0"), selected(cx, "7/v.0-tab1")), (Some(false), Some(true)));
}

#[gpui_kit::test]
fn what_a_tab_switched_away_from_held_is_let_go_of(cx: &mut TestAppContext) {
    let both = json!([{"kind": "tabs", "tabs": [
        {"title": "One", "blocks": [{"kind": "markdown", "text": "first"}]},
        {"title": "Two", "blocks": [{"kind": "markdown", "text": "second"}]}
    ]}]);
    let rig = open(cx, 700.0, both);
    assert_eq!(cx.update(|cx| rig.view.read(cx).kept()), 1);
    click(cx, &rig, "7/v.0-tab1");
    assert_eq!(cx.update(|cx| rig.view.read(cx).kept()), 1, "the first tab's Markdown went with the tab, it is not kept for ever");
}

fn fold() -> Value {
    json!([{"kind": "details", "summary": "More", "open": false, "blocks": [note("inside")]}])
}

#[gpui_kit::test]
fn a_fold_opens_when_pressed_and_stays_as_the_person_left_it_when_the_agent_says_otherwise(cx: &mut TestAppContext) {
    let rig = open(cx, 700.0, fold());
    assert!(!exists(cx, &rig, "7/v.0.0-callout"), "the agent's `open` is only how it starts");
    click(cx, &rig, "7/v.0-summary");
    assert!(exists(cx, &rig, "7/v.0.0-callout"));
    show(cx, &rig, fold(), agent());
    assert!(exists(cx, &rig, "7/v.0.0-callout"), "the one being read must not close under the reader");
    click(cx, &rig, "7/v.0-summary");
    assert!(!exists(cx, &rig, "7/v.0.0-callout"));
    let started_open = json!([{"kind": "details", "summary": "More", "open": true, "blocks": [note("inside")]}]);
    show(cx, &rig, started_open, agent());
    assert!(!exists(cx, &rig, "7/v.0.0-callout"), "and one the person closed stays closed");
}

#[gpui_kit::test]
fn a_node_with_children_folds_and_unfolds_and_remembers_it(cx: &mut TestAppContext) {
    let nodes = json!([{"kind": "tree", "nodes": [{"label": "root", "open": false, "children": [{"label": "child", "children": [{"label": "leaf"}]}]}]}]);
    let rig = open(cx, 700.0, nodes.clone());
    assert!(exists(cx, &rig, "7/v.0/0-fold"));
    assert!(!exists(cx, &rig, "7/v.0/0/0-fold"), "folded, its children are not drawn");
    click(cx, &rig, "7/v.0/0-fold");
    assert!(exists(cx, &rig, "7/v.0/0/0-fold"));
    show(cx, &rig, nodes, agent());
    assert!(exists(cx, &rig, "7/v.0/0/0-fold"), "kept across a redraw");
}

fn two_columns() -> Value {
    json!([{"kind": "columns", "columns": [[note("left")], [note("right")]]}])
}

// BLOCKS-3: the columns stack when the box is narrow, by the box's own width.
#[gpui_kit::test]
fn columns_sit_side_by_side_when_there_is_room_and_stack_when_there_is_not(cx: &mut TestAppContext) {
    let wide = open(cx, 700.0, two_columns());
    let (left, right) = (bounds(cx, &wide, "7/v.0.0.0-callout").unwrap(), bounds(cx, &wide, "7/v.0.1.0-callout").unwrap());
    assert_eq!(left.origin.y, right.origin.y, "one row");
    assert!(right.origin.x > left.origin.x);
    let narrow = open(cx, 380.0, two_columns());
    let (left, right) = (bounds(cx, &narrow, "7/v.0.0.0-callout").unwrap(), bounds(cx, &narrow, "7/v.0.1.0-callout").unwrap());
    assert_eq!(left.origin.x, right.origin.x, "one column");
    assert!(right.origin.y > left.origin.y);
}

#[gpui_kit::test]
fn a_box_that_grows_is_drawn_again_at_its_new_width(cx: &mut TestAppContext) {
    let rig = open(cx, 380.0, two_columns());
    set_width(cx, &rig, 700.0);
    let (left, right) = (bounds(cx, &rig, "7/v.0.0.0-callout").unwrap(), bounds(cx, &rig, "7/v.0.1.0-callout").unwrap());
    assert_eq!(left.origin.y, right.origin.y, "the columns found their room");
}

fn lanes() -> Value {
    json!([{"kind": "timeline", "events": [
        {"title": "build", "lane": "ci", "at": "Oct 12"},
        {"title": "review", "lane": "people", "at": "Oct 13"},
        {"title": "no lane"}
    ]}])
}

#[gpui_kit::test]
fn lanes_are_columns_in_a_wide_window_and_stack_in_a_narrow_one(cx: &mut TestAppContext) {
    let wide = open(cx, 900.0, lanes());
    let at = |cx: &mut TestAppContext, rig: &Rig, k: usize| bounds(cx, rig, &format!("7/v.0-event{k}")).unwrap();
    let (build, review, plain) = (at(cx, &wide, 0), at(cx, &wide, 1), at(cx, &wide, 2));
    assert!(review.origin.x > build.origin.x, "another lane is another column");
    assert_eq!(plain.origin.x, build.origin.x, "an event naming no lane is in the first lane's");
    assert!(review.origin.y > build.origin.y, "one row per event, so the lanes read side by side");
    let narrow = open(cx, 520.0, lanes());
    let (build, review) = (at(cx, &narrow, 0), at(cx, &narrow, 1));
    assert_eq!(review.origin.x, build.origin.x, "stacked");
}

// BLOCKS-30: the lanes stack by the box the timeline is drawn in, not by the window.
#[gpui_kit::test]
fn lanes_stack_by_their_own_box_inside_a_wide_window(cx: &mut TestAppContext) {
    let rig = open(cx, 1200.0, lanes());
    let at = |cx: &mut TestAppContext, k: usize| bounds(cx, &rig, &format!("7/v.0-event{k}")).unwrap();
    set_width(cx, &rig, 900.0);
    assert!(at(cx, 1).origin.x > at(cx, 0).origin.x, "side by side in a wide box");
    set_width(cx, &rig, 500.0);
    assert_eq!(at(cx, 1).origin.x, at(cx, 0).origin.x, "stacked in a narrow box of the same window");
    set_width(cx, &rig, 900.0);
    assert!(at(cx, 1).origin.x > at(cx, 0).origin.x, "and side by side again when the box grows");
}
