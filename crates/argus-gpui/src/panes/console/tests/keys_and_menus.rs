use super::*;

#[gpui_kit::test]
fn each_screen_reports_its_room_for_its_own_side(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    cx.executor().advance_clock(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    let mut sides: Vec<String> = opened
        .calls
        .borrow()
        .iter()
        .filter_map(|call| match call {
            // The host's own Workspace screen reports its room too; it is not the console's.
            Call::Resize { side, .. } if *side != Side::Workspace => Some(format!("{side:?}")),
            _ => None,
        })
        .collect();
    sides.sort();
    assert_eq!(sides, ["First", "Second"], "one report each, and none from the console for the Workspace's side");
}

#[gpui_kit::test]
fn a_press_on_the_workspace_while_a_console_has_the_keys_asks_the_session_and_waits(cx: &mut TestAppContext) {
    let opened = open(cx);
    let shown = Shown {
        pane: 9,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "main".into(),
        agent: false,
        browser: false,
        title: "shell".into(),
        status: "idle",
        resumed: None,
    };
    hear(cx, &opened, vec![connected(), Event::Rail(Box::new(Rail { shown: Some(shown), ..Rail::default() })), single()]);
    assert!(has_keys(cx, &opened, "first"));
    let at = centre(cx, &opened, "terminal");
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, &opened);
    assert_eq!(taken(&opened), ["Select { pane: 9 }"]);
    assert!(has_keys(cx, &opened, "first"), "nothing predicted: the keys stay where the session routes them");
    let workspace_focused = cx.update_window(opened.handle, |_, window, cx| opened.workspace.read(cx).focus_handle().is_focused(window)).unwrap();
    assert!(!workspace_focused);
}

#[gpui_kit::test]
fn the_menus_rename_item_opens_the_dialog_that_asks_on_enter(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    click_with(cx, &opened, "console-tab-pick-7", MouseButton::Right);
    click_menu_item(cx, &opened, 1);
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx, &opened);
    assert!(cx.update_window(opened.handle, |_, window, cx| gpui_kit::component::WindowExt::has_active_dialog(window, cx)).unwrap());
    press(cx, &opened, "x");
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened), ["RenamePane { pane: 7, title: \"x\" }"]);
}

#[gpui_kit::test]
fn a_right_or_middle_press_on_a_screen_without_the_keys_asks_for_them_too(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), page(vec![tab(5, "pwsh", "idle", true), tab(6, "cargo test", "working", true)], Some(5), Some(6), Some(5))]);
    // SHELL-INPUT-5, -9: any button asks, none moves the keys before the event says so.
    for button in [MouseButton::Right, MouseButton::Middle] {
        opened.calls.borrow_mut().clear();
        click_with(cx, &opened, "terminal-second", button);
        assert_eq!(taken(&opened), ["Console { gesture: Select { pane: 6 } }"], "{button:?}");
        assert!(has_keys(cx, &opened, "first"), "{button:?}");
    }
}

#[gpui_kit::test]
fn a_field_that_has_the_keys_keeps_them_when_the_console_moves_them(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    rename(cx, &opened);
    assert!(!has_keys(cx, &opened, "second"));
    // The session moves the keys to the first side while the dialog's field has GPUI's focus.
    hear(cx, &opened, vec![page(vec![tab(5, "pwsh", "idle", true), tab(6, "cargo test", "working", true)], Some(5), Some(6), Some(5))]);
    assert!(
        !has_keys(cx, &opened, "first") && !has_keys(cx, &opened, "second") && !has_keys(cx, &opened, "stage"),
        "a dialog's field keeps the keys"
    );
    press(cx, &opened, "q");
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened), ["RenamePane { pane: 6, title: \"q\" }"], "the field still took the typing");
}

#[gpui_kit::test]
fn the_menus_rename_dialog_gives_the_keys_back_to_what_had_them_before_the_menu(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(has_keys(cx, &opened, "second"));
    click_with(cx, &opened, "console-tab-pick-7", MouseButton::Right);
    click_menu_item(cx, &opened, 1);
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx, &opened);
    press(cx, &opened, "escape");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx, &opened);
    assert!(has_keys(cx, &opened, "second"), "not the menu's handle, which is gone");
}

#[gpui_kit::test]
fn the_more_menus_other_items_send_theirs(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    // Place Tab: 0 Split, 1 dock, 2 Fullscreen, 3 rule, then the dock moves.
    for (nth, call) in [(1, "Console { gesture: Dock }"), (2, "Console { gesture: Fullscreen }")] {
        opened.calls.borrow_mut().clear();
        click(cx, &opened, "console-menu");
        click_menu_item(cx, &opened, nth);
        assert_eq!(taken(&opened), [call], "item {nth}");
    }
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "console-menu");
    click_menu_item(cx, &opened, 4);
    assert!(taken(&opened)[0].starts_with("Docks { gesture: Move { panel: Console"), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn the_tab_menus_close_the_others_closes_every_other_tab(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    click_with(cx, &opened, "console-tab-pick-7", MouseButton::Right);
    click_menu_item(cx, &opened, 4);
    assert_eq!(taken(&opened), ["Console { gesture: Close { pane: 5 } }"]);
}

#[gpui_kit::test]
fn a_split_sides_menu_takes_it_out_of_the_split(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    click_with(cx, &opened, "console-side-pick-6", MouseButton::Right);
    click_menu_item(cx, &opened, 2);
    assert_eq!(taken(&opened), ["Console { gesture: CloseSide { pane: 6 } }"]);
}

/// SHELL-CONSOLE-11/12: the keys follow the screen that is typed in, not only
/// the pane id, so an unsplit or a trade of sides moves them too.
#[gpui_kit::test]
fn an_unsplit_that_keeps_the_second_side_moves_the_keys_to_the_first_screen(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(has_keys(cx, &opened, "second"));
    hear(cx, &opened, vec![page(vec![tab(6, "cargo test", "working", true), tab(7, "node", "idle", false)], Some(6), None, Some(6))]);
    assert!(has_keys(cx, &opened, "first"), "the typed console is the first screen now");
    opened.calls.borrow_mut().clear();
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened).len(), 1, "{:?}", taken(&opened));
    assert!(taken(&opened)[0].starts_with("Key {"), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_trade_of_sides_moves_the_keys_with_the_typed_console(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(has_keys(cx, &opened, "second"));
    let tabs = || vec![tab(5, "pwsh", "idle", true), tab(6, "cargo test", "working", true)];
    hear(cx, &opened, vec![page(tabs(), Some(6), Some(5), Some(6))]);
    assert!(has_keys(cx, &opened, "first"), "pane 6 is the first side now");
    hear(cx, &opened, vec![page(tabs(), Some(5), Some(6), Some(6))]);
    assert!(has_keys(cx, &opened, "second"), "and back");
}

#[gpui_kit::test]
fn the_tab_menus_close_the_others_sends_a_close_for_each_of_the_others(cx: &mut TestAppContext) {
    let opened = open(cx);
    let tabs = vec![tab(5, "pwsh", "idle", true), tab(7, "node", "idle", false), tab(8, "vim", "idle", false)];
    hear(cx, &opened, vec![connected(), page(tabs, Some(5), None, Some(5))]);
    click_with(cx, &opened, "console-tab-pick-7", MouseButton::Right);
    click_menu_item(cx, &opened, 4);
    assert_eq!(taken(&opened), ["Console { gesture: Close { pane: 5 } }", "Console { gesture: Close { pane: 8 } }"]);
}

#[gpui_kit::test]
fn only_a_console_that_is_doing_something_wears_a_status_dot(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(present(cx, &opened, "console-tab-dot-6"), "a working console wears one");
    assert!(!present(cx, &opened, "console-tab-dot-5"), "an idle one wears none");
    assert!(present(cx, &opened, "console-side-dot-6"));
    assert!(!present(cx, &opened, "console-side-dot-5"));
}

/// A press elsewhere released over a console tab is not a click on it, as the
/// page's `auxclick` is not.
#[gpui_kit::test]
fn a_middle_press_that_began_elsewhere_and_ended_on_a_tab_closes_nothing(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    let (away, over) = (centre(cx, &opened, "terminal-first"), centre(cx, &opened, "console-tab-pick-7"));
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(away, MouseButton::Middle, cx);
        window.pointer_up(over, MouseButton::Middle, cx);
    })
    .unwrap();
    settle(cx, &opened);
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}

/// The hide button's tooltip and name are one text: the keymap's key for the
/// action once it is heard, none before.
#[gpui_kit::test]
fn the_hide_button_names_the_key_the_session_binds(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    let named = |cx: &mut TestAppContext| {
        cx.update_window(opened.handle, |_, window, _| window.find("console-hide-label".to_owned()).label().map(str::to_owned)).unwrap()
    };
    assert_eq!(named(cx).as_deref(), Some("Hide the console"));
    hear(cx, &opened, vec![Event::Keymap { groups: argus_desktop_core::keys::keymap::groups(), commands: Vec::new(), layers: &[] }]);
    assert_eq!(named(cx).as_deref(), Some("Hide the console (`)"));
}
