//! The cells: one for each pane the rail's grid names, placed by its sizes, each with the header
//! and the screen that belongs to it.

use std::time::Duration;

use argus_desktop_core::connection::page::Side;
use gpui_kit::{px, TestAppContext};

use super::*;
use crate::panes::terminal::room::SETTLE;

fn settle(cx: &mut TestAppContext, opened: &Opened) {
    cx.executor().advance_clock(SETTLE + Duration::from_millis(10));
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

#[gpui_kit::test]
fn each_pane_the_rail_names_has_a_cell_at_the_place_its_sizes_give(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let (zero, one, two) = (bounds(cx, &opened, "cell-0"), bounds(cx, &opened, "cell-1"), bounds(cx, &opened, "cell-2"));
    assert!((zero.size.width - one.size.width).abs() < px(1.0), "two halves of the first row");
    assert!((one.origin.x - (zero.origin.x + zero.size.width)).abs() < px(1.0), "the second sits where the first ends");
    assert!((two.size.width - (zero.size.width + one.size.width)).abs() < px(1.0), "the third takes the whole row");
    assert!((two.origin.y - (zero.origin.y + zero.size.height)).abs() < px(1.0), "under the first row");
    assert!(!has(cx, &opened, "cell-3"));
}

#[gpui_kit::test]
fn a_cell_follows_the_sizes_the_rail_changes(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let before = bounds(cx, &opened, "cell-0").size.width;
    let mut row = three();
    row.widths = vec![vec![250, 750], vec![1000]];
    hear(cx, &opened, vec![rail(row)]);
    let after = bounds(cx, &opened, "cell-0").size.width;
    assert!(after < before * 0.6, "{before:?} -> {after:?}");
}

#[gpui_kit::test]
fn with_two_or_more_panes_every_cell_has_a_header_and_one_pane_has_none(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    for n in 0..3 {
        assert!(has(cx, &opened, &format!("cell-head-{n}")), "cell {n}");
    }
    assert!(!has(cx, &opened, "cell-close-0"), "the Workspace's own pane cannot be taken out");
    assert!(has(cx, &opened, "cell-close-1") && has(cx, &opened, "cell-close-2"));
    hear(cx, &opened, vec![rail(alone())]);
    assert!(!has(cx, &opened, "cell-head-0"), "SHELL-GRID-3: one pane has no header");
    assert!(has(cx, &opened, "cell-0"));
    assert!(!has(cx, &opened, "cell-1"));
    assert!(!has(cx, &opened, "gutter-across-0-0"), "and no gutter");
}

#[gpui_kit::test]
fn the_cell_with_the_keys_has_its_header_marked(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let marked = |cx: &mut TestAppContext, n: usize| {
        cx.update_window(opened.handle, |_, window, _| window.find(format!("cell-head-{n}")).selected()).unwrap()
    };
    assert_eq!([marked(cx, 0), marked(cx, 1), marked(cx, 2)], [Some(true), Some(false), Some(false)]);
    let mut row = three();
    row.keyed = Some(2);
    hear(cx, &opened, vec![rail(row)]);
    assert_eq!([marked(cx, 0), marked(cx, 1), marked(cx, 2)], [Some(false), Some(false), Some(true)]);
}

#[gpui_kit::test]
fn closing_a_split_asks_the_session_to_take_it_out_and_predicts_nothing(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    cx.update_window(opened.handle, |_, window, cx| window.click("cell-close-2", cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(asked(&opened), ["CloseBeside { side: Beside(1) }"]);
    assert!(has(cx, &opened, "cell-2"), "the cell goes when the rail says so");
}

#[gpui_kit::test]
fn each_split_reports_the_room_of_its_own_cell(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    settle(cx, &opened);
    let rooms = rooms(&opened);
    for side in [Side::Workspace, Side::Beside(0), Side::Beside(1)] {
        assert!(rooms.iter().any(|(reported, ..)| *reported == side), "{side:?} in {rooms:?}");
    }
    let cols = |side| rooms.iter().rev().find(|(s, ..)| *s == side).map(|(_, _, cols)| *cols).unwrap();
    assert!(cols(Side::Beside(1)) > cols(Side::Beside(0)), "the third cell has the whole row");
}

#[gpui_kit::test]
fn a_cell_that_goes_and_comes_back_reports_its_room_again(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    settle(cx, &opened);
    hear(cx, &opened, vec![rail(alone())]);
    settle(cx, &opened);
    opened.calls.borrow_mut().clear();
    hear(cx, &opened, vec![rail(three())]);
    settle(cx, &opened);
    let again = rooms(&opened);
    assert!(again.iter().any(|(side, ..)| *side == Side::Beside(1)), "the session forgot its size: {again:?}");
}

#[gpui_kit::test]
fn a_browser_pane_gets_its_view_in_the_cell_not_a_terminal(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let mut row = three();
    row.beside[0] = Some(Shown { browser: true, ..shown(2, "docs") });
    hear(cx, &opened, vec![rail(row)]);
    assert!(has(cx, &opened, "browser-2"), "the browser's own view fills the cell");
    assert!(has(cx, &opened, "cell-head-1"), "it keeps its header and its place in the grid");
    assert!(!has(cx, &opened, "browser-3"), "the others stay terminals");
}

#[gpui_kit::test]
fn the_stage_with_no_pane_draws_the_workspaces_screen_alone(cx: &mut TestAppContext) {
    let opened = opened(cx, three());
    let none = Event::Rail(Box::new(Rail { loaded: true, workspace_shown: true, stage: Stage::Workspace, ..Rail::default() }));
    hear(cx, &opened, vec![none]);
    assert!(!has(cx, &opened, "cell-0") && !has(cx, &opened, "cell-head-0"));
    assert!(has(cx, &opened, "split"));
}

/// Everything a fresh window would hear in the core's scene `name`, and the first frames apart: a
/// frame for a cell that has no screen yet is dropped, so the cells come first.
fn scene(name: &str) -> (Vec<Event>, Vec<Event>) {
    let found = argus_desktop_core::screens::scenes().into_iter().find(|scene| scene.name == name).expect("the scene exists");
    let mut session = (found.build)();
    let mut events = session.everything(argus_desktop_core::screens::NOW);
    events.extend(session.take_frames());
    events.into_iter().partition(|event| !matches!(event, Event::Frame(_)))
}

#[gpui_kit::test]
fn the_three_pane_scene_draws_three_cells_and_the_screens_of_the_panes_beside(cx: &mut TestAppContext) {
    let opened = open(cx);
    let (rest, frames) = scene("workspace-grid");
    hear(cx, &opened, rest);
    hear(cx, &opened, frames);
    for n in 0..3 {
        assert!(has(cx, &opened, &format!("cell-head-{n}")), "cell {n}");
    }
    assert!(!has(cx, &opened, "cell-3"));
    let beside = cx.update(|cx| [Side::Beside(0), Side::Beside(1)].map(|side| opened.heard.read(cx).screen_of(side).pane()));
    assert_eq!(beside, [Some(2), Some(3)], "each split draws the pane the session put in it");
    assert!(bounds(cx, &opened, "cell-2").size.width > bounds(cx, &opened, "cell-0").size.width * 1.9, "the third has the whole row");
}

#[gpui_kit::test]
fn the_four_pane_scene_is_a_grid_of_two_rows_with_the_keys_on_the_last(cx: &mut TestAppContext) {
    let opened = open(cx);
    let (rest, frames) = scene("workspace-grid-four");
    hear(cx, &opened, rest);
    hear(cx, &opened, frames);
    for n in 0..4 {
        assert!(has(cx, &opened, &format!("cell-head-{n}")), "cell {n}");
    }
    let marked = cx.update_window(opened.handle, |_, window, _| window.find("cell-head-3".to_owned()).selected()).unwrap();
    assert_eq!(marked, Some(true));
    let (top, below) = (bounds(cx, &opened, "cell-0"), bounds(cx, &opened, "cell-3"));
    assert!(below.origin.y > top.origin.y, "the fourth is on the second row");
}
