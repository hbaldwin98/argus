use super::*;

#[gpui_kit::test]
fn the_rename_dialog_asks_for_the_new_name_on_enter_and_gives_the_keys_back(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(has_keys(cx, &opened, "second"));
    rename(cx, &opened);
    assert!(!has_keys(cx, &opened, "second"), "the dialog has the keys while it is open");
    press(cx, &opened, "b");
    press(cx, &opened, "u");
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened), ["RenamePane { pane: 6, title: \"bu\" }"], "the old name was selected, so typing replaced it");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx, &opened);
    assert!(has_keys(cx, &opened, "second"), "closing it gives the keys back to what had them");
    assert!(!cx.update_window(opened.handle, |_, window, cx| gpui_kit::component::WindowExt::has_active_dialog(window, cx)).unwrap());
}

#[gpui_kit::test]
fn escape_closes_the_rename_dialog_asking_nothing_and_gives_the_keys_back(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    rename(cx, &opened);
    press(cx, &opened, "x");
    press(cx, &opened, "escape");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx, &opened);
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
    assert!(has_keys(cx, &opened, "second"));
    assert!(!cx.update_window(opened.handle, |_, window, cx| gpui_kit::component::WindowExt::has_active_dialog(window, cx)).unwrap());
}

#[gpui_kit::test]
fn the_rename_dialog_is_modal_and_tab_stays_inside_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    rename(cx, &opened);
    for _ in 0..6 {
        press(cx, &opened, "tab");
        assert!(!has_keys(cx, &opened, "first") && !has_keys(cx, &opened, "second") && !has_keys(cx, &opened, "stage"), "Tab left the dialog");
    }
    assert!(taken(&opened).is_empty(), "keys in the dialog reach no pane: {:?}", taken(&opened));
}

#[gpui_kit::test]
fn the_rename_button_asks_and_cancel_does_not(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    rename(cx, &opened);
    press(cx, &opened, "z");
    click(cx, &opened, "rename-cancel");
    assert!(taken(&opened).is_empty());

    rename(cx, &opened);
    press(cx, &opened, "z");
    click(cx, &opened, "rename-ok");
    assert_eq!(taken(&opened), ["RenamePane { pane: 6, title: \"z\" }"]);
}

#[gpui_kit::test]
fn the_tab_menu_offers_rename_and_close_for_that_tab(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    let at = centre(cx, &opened, "console-tab-pick-7");
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Right, cx);
        window.pointer_up(at, MouseButton::Right, cx);
    })
    .unwrap();
    settle(cx, &opened);
    click_menu_item(cx, &opened, 3);
    assert_eq!(taken(&opened), ["Console { gesture: Close { pane: 7 } }"]);
}
