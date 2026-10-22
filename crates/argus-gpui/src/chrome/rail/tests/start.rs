//! A checkout's + menu: opened by the + under the pointer, drawn from what the session says, and
//! keyed through its own layer.

use super::*;

fn start_keys(opened: &Opened) -> Vec<String> {
    taken(opened).into_iter().filter(|call| call.starts_with("StartKey")).collect()
}

#[gpui_kit::test]
fn the_plus_asks_for_the_menu_and_the_menu_is_drawn_when_the_session_says_it_is_open(cx: &mut TestAppContext) {
    // SHELL-RAIL-9, 28.
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "checkout-110-start").as_deref(), Some("Start a shell or an agent on argus main"));
    hover(cx, &opened, "checkout-110");
    click(cx, &opened, "checkout-110-start");
    assert_eq!(taken(&opened), ["ToggleStart { checkout: 110, from: Rail }"]);
    assert!(!present(cx, &opened, "start-menu-110"), "nothing is drawn before the session says it");
    assert_eq!(picked(cx, &opened), None, "the + does not pick the row");
    hear(cx, &opened, vec![starting(110, 0)]);
    assert!(present(cx, &opened, "start-menu-110"));
    assert_eq!(label(cx, &opened, "start-menu-110").as_deref(), Some("Start on argus main"));
    assert_eq!(selected(cx, &opened, "start-110-0"), Some(true), "the first item is highlighted");
    assert_eq!(selected(cx, &opened, "start-110-1"), Some(false));
    assert_eq!(selected(cx, &opened, "checkout-110-start"), None, "the + is a button, not a selection");
}

#[gpui_kit::test]
fn the_menu_lists_a_shell_then_each_agent_with_the_highlight_where_the_session_put_it(cx: &mut TestAppContext) {
    // SHELL-RAIL-29.
    let opened = open(cx, vec![status(Connection::Connected), starting(110, 2)]);
    for (id, words) in [("start-110-0", "Shell"), ("start-110-1", "Agent \u{b7} claude"), ("start-110-2", "Agent \u{b7} codex")] {
        assert!(present(cx, &opened, id), "{words}");
    }
    assert!(!present(cx, &opened, "start-110-3"));
    assert_eq!(selected(cx, &opened, "start-110-2"), Some(true));
    assert_eq!(selected(cx, &opened, "start-110-0"), Some(false));
}

#[gpui_kit::test]
fn a_menu_belongs_to_the_checkout_it_was_opened_on(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connected), starting(112, 0)]);
    assert!(present(cx, &opened, "start-menu-112"));
    assert!(!present(cx, &opened, "start-menu-110"));
}

#[gpui_kit::test]
fn a_click_on_an_item_starts_it_without_picking_the_row(cx: &mut TestAppContext) {
    // SHELL-RAIL-30, 31.
    let opened = open(cx, vec![status(Connection::Connected), starting(110, 0)]);
    click(cx, &opened, "start-110-1");
    assert_eq!(taken(&opened), ["StartOn { checkout: 110, template: Some(\"claude\") }"]);
    click(cx, &opened, "start-110-0");
    assert_eq!(taken(&opened)[1], "StartOn { checkout: 110, template: None }");
}

#[gpui_kit::test]
fn the_menu_has_its_own_keys_before_the_rails_and_holds_the_rest(cx: &mut TestAppContext) {
    // SHELL-RAIL-30: `start` is a layer of its own, heard before the picked row's.
    let opened = open_shell(cx);
    // The menu was opened from the rail, so the rail has the keys.
    click(cx, &opened, "checkout-110");
    hear(cx, &opened, vec![starting(110, 0)]);
    forget(&opened);
    for (key, action) in [("j", "Next"), ("down", "Next"), ("k", "Previous"), ("up", "Previous"), ("enter", "Choose"), ("escape", "CloseLayer"), ("s", "StartShell"), ("a", "StartAgent")] {
        type_key(cx, &opened, key);
        assert_eq!(start_keys(&opened).last().map(String::as_str), Some(format!("StartKey {{ action: {action} }}").as_str()), "{key}");
    }
    forget(&opened);
    for key in ["x", "z", "r", "q", "3"] {
        type_key(cx, &opened, key);
    }
    assert!(taken(&opened).is_empty(), "a key the menu does not answer is held, not the rail's or the stage's: {:?}", taken(&opened));
}

#[gpui_kit::test]

#[gpui_kit::test]
fn a_held_rail_starts_nothing_from_its_menu(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connected), starting(110, 0)]);
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    click(cx, &opened, "start-110-0");
    hover(cx, &opened, "checkout-110");
    click(cx, &opened, "checkout-110-start");
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_press_outside_the_menu_shuts_it_and_a_press_on_the_plus_only_toggles_it(cx: &mut TestAppContext) {
    // SHELL-RAIL-28: a click anywhere else shuts it; the + that opened it toggles it itself.
    let opened = open_shell(cx);
    click(cx, &opened, "checkout-110");
    hear(cx, &opened, vec![starting(110, 0)]);
    hover(cx, &opened, "checkout-110");
    // The pointer is on the + itself when it is pressed, as a person's is.
    hover(cx, &opened, "checkout-110-start");
    forget(&opened);
    click(cx, &opened, "checkout-110-start");
    assert_eq!(taken(&opened), ["ToggleStart { checkout: 110, from: Rail }"], "one call for the + that opened it");
    forget(&opened);
    let away = point(px(900.0), px(300.0));
    cx.update_window(opened.handle, |_, window, cx| window.pointer_move(away, None, cx)).unwrap();
    press(cx, &opened, away, MouseButton::Left);
    assert_eq!(start_keys(&opened), ["StartKey { action: CloseLayer }"], "{:?}", taken(&opened));
}
