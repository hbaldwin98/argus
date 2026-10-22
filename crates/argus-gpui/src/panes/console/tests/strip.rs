use super::*;

#[gpui_kit::test]
fn a_console_snapshot_draws_its_tabs_and_screens(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);

    for id in ["console-tab-5", "console-tab-6", "console-tab-7"] {
        assert!(present(cx, &opened, id), "{id}");
    }
    assert_eq!(selected(cx, &opened, "console-tab-5"), Some(true), "chosen for the first side");
    assert_eq!(selected(cx, &opened, "console-tab-6"), Some(true), "chosen for the second");
    assert_eq!(selected(cx, &opened, "console-tab-7"), Some(false));
    // Two screens at once, each under its own header while split; the one with the keys marked.
    assert!(present(cx, &opened, "terminal-first") && present(cx, &opened, "terminal-second"));
    assert_eq!(selected(cx, &opened, "console-side-head-6"), Some(true));
    assert_eq!(selected(cx, &opened, "console-side-head-5"), Some(false));

    let (first, second) = (paints(cx, &opened, "first"), paints(cx, &opened, "second"));
    hear_parked(cx, &opened, vec![frame(5, Side::First, "first screen"), frame(6, Side::Second, "second screen")]);
    cx.update(|cx| {
        let heard = opened.heard.read(cx);
        assert_eq!(heard.screen_of(Side::First).row_text(0), "first screen");
        assert_eq!(heard.screen_of(Side::Second).row_text(0), "second screen");
    });
    assert!(paints(cx, &opened, "first") > first, "a frame for the first side repaints it");
    assert!(paints(cx, &opened, "second") > second, "and the second");
}

#[gpui_kit::test]
fn a_console_that_is_not_split_draws_one_screen_and_no_side_headers(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    assert!(present(cx, &opened, "terminal-first"));
    assert!(!present(cx, &opened, "terminal-second"));
    assert!(!present(cx, &opened, "console-side-head-5"));
}

#[gpui_kit::test]
fn a_tab_that_asks_the_daemon_changes_nothing_until_the_console_event_says_so(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    click(cx, &opened, "console-tab-pick-7");
    assert_eq!(taken(&opened), ["Console { gesture: Select { pane: 7 } }"]);
    assert_eq!(selected(cx, &opened, "console-tab-7"), Some(false), "nothing predicted: the daemon has not answered");
    assert_eq!(selected(cx, &opened, "console-tab-5"), Some(true));
    hear(cx, &opened, vec![page(vec![tab(5, "pwsh", "idle", false), tab(7, "node", "idle", true)], Some(7), None, Some(7))]);
    assert_eq!(selected(cx, &opened, "console-tab-7"), Some(true), "the event moved it");
}

#[gpui_kit::test]
fn a_middle_click_and_the_tabs_x_both_close_the_console(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    click_with(cx, &opened, "console-tab-pick-7", MouseButton::Middle);
    assert_eq!(taken(&opened), ["Console { gesture: Close { pane: 7 } }"], "a middle press selects nothing");
    opened.calls.borrow_mut().clear();
    click(cx, &opened, "console-tab-close-7");
    assert_eq!(taken(&opened), ["Console { gesture: Close { pane: 7 } }"]);
    assert!(present(cx, &opened, "console-tab-7"), "the tab goes when the daemon says it ended");
}

#[gpui_kit::test]
fn the_strips_buttons_send_theirs(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    click(cx, &opened, "console-new");
    click(cx, &opened, "console-hide");
    assert_eq!(taken(&opened), ["Console { gesture: Spawn }", "Console { gesture: Hide }"]);
}

#[gpui_kit::test]
fn an_older_daemon_offers_no_new_console_button(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut older = ConsolePage { place: ConsolePlace::Tab, tabs: vec![tab(5, "pwsh", "idle", true)], showing: Some(5), ..ConsolePage::default() };
    older.can_spawn = false;
    hear(cx, &opened, vec![connected(), Event::Console(Box::new(older))]);
    assert!(!present(cx, &opened, "console-new"));
    assert!(present(cx, &opened, "console-hide"));
}

#[gpui_kit::test]
fn the_more_menu_sends_what_its_item_says(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    click(cx, &opened, "console-menu");
    click_menu_item(cx, &opened, 0);
    assert_eq!(taken(&opened), ["Console { gesture: Split }"]);
}

#[gpui_kit::test]
fn a_split_sides_header_selects_and_its_close_leaves_the_split(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    click(cx, &opened, "console-side-pick-5");
    click(cx, &opened, "console-side-close-6");
    assert_eq!(taken(&opened), ["Console { gesture: Select { pane: 5 } }", "Console { gesture: CloseSide { pane: 6 } }"]);
}

#[gpui_kit::test]
fn with_no_console_on_screen_the_body_says_how_to_get_one(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), page(Vec::new(), None, None, None)]);
    assert!(present(cx, &opened, "console-empty"));
    click(cx, &opened, "console-start");
    assert_eq!(taken(&opened), ["Console { gesture: Spawn }"]);

    opened.calls.borrow_mut().clear();
    hear(cx, &opened, vec![page(vec![tab(5, "pwsh", "idle", false), tab(6, "node", "idle", true)], None, None, None)]);
    click(cx, &opened, "console-type");
    assert_eq!(taken(&opened), ["Console { gesture: Select { pane: 6 } }"], "the chosen tab");
}

#[gpui_kit::test]
fn a_stopped_daemon_leaves_the_console_answering_no_press(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    hear(cx, &opened, vec![Event::Status(Status { connection: Connection::Stopped, detail: None, alarm: None, chime: true, host: None })]);
    click(cx, &opened, "console-tab-pick-7");
    click(cx, &opened, "console-hide");
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn the_keys_move_to_the_screen_the_console_says_is_typed_in(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert!(has_keys(cx, &opened, "stage"), "the Workspace's pane has them to begin with");
    hear(cx, &opened, vec![connected(), single()]);
    assert!(has_keys(cx, &opened, "first"), "a console shown takes them");
    hear(cx, &opened, vec![split()]);
    assert!(has_keys(cx, &opened, "second"));
    hear(cx, &opened, vec![page(vec![tab(5, "pwsh", "idle", true)], None, None, None)]);
    assert!(has_keys(cx, &opened, "stage"), "the console letting its panes go gives them back");
}

#[gpui_kit::test]
fn a_press_on_a_screen_without_the_keys_asks_for_them_and_waits_for_the_answer(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split(), page(vec![tab(5, "pwsh", "idle", true), tab(6, "cargo test", "working", true)], Some(5), Some(6), Some(5))]);
    assert!(has_keys(cx, &opened, "first"));
    let at = centre(cx, &opened, "terminal-second");
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, &opened);
    assert_eq!(taken(&opened), ["Console { gesture: Select { pane: 6 } }"]);
    assert!(has_keys(cx, &opened, "first"), "nothing predicted: the keys are where the console says");
    hear(cx, &opened, vec![split()]);
    assert!(has_keys(cx, &opened, "second"), "the event moved them");

    opened.calls.borrow_mut().clear();
    let at = centre(cx, &opened, "terminal-second");
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    })
    .unwrap();
    settle(cx, &opened);
    assert!(taken(&opened).is_empty(), "a screen that has the keys asks for nothing");
}

#[gpui_kit::test]
fn a_key_typed_into_the_second_screen_is_sent_once_and_from_that_screen_only(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), split()]);
    assert!(has_keys(cx, &opened, "second") && !has_keys(cx, &opened, "first"));
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened).len(), 1, "one key, one call: {:?}", taken(&opened));
    assert!(taken(&opened)[0].starts_with("Key {"), "{:?}", taken(&opened));

    opened.calls.borrow_mut().clear();
    hear(cx, &opened, vec![single()]);
    assert!(has_keys(cx, &opened, "first"));
    press(cx, &opened, "enter");
    assert_eq!(taken(&opened).len(), 1, "typed into the first now, still once");
}

fn wait(cx: &mut TestAppContext, opened: &Opened, by: std::time::Duration) {
    cx.executor().advance_clock(by);
    settle(cx, opened);
}

#[gpui_kit::test]
fn a_tab_being_switched_to_shows_bars_after_250_ms_until_its_first_frame(cx: &mut TestAppContext) {
    // SHELL-SCREEN-12 / SHELL-CONSOLE-11: the console's screens expect their pane while on screen.
    use crate::panes::terminal::skeleton::DELAY;
    let opened = open(cx);
    hear(cx, &opened, vec![connected(), single()]);
    hear(cx, &opened, vec![frame(5, Side::First, "pwsh")]);
    assert!(!present(cx, &opened, "terminal-skeleton"), "the pane on screen has its rows");
    // The node tab is picked: the session says so, and its first frame has not come.
    hear(cx, &opened, vec![page(vec![tab(5, "pwsh", "idle", false), tab(7, "node", "idle", true)], Some(7), None, Some(7))]);
    wait(cx, &opened, DELAY - std::time::Duration::from_millis(10));
    assert!(!present(cx, &opened, "terminal-skeleton"), "a pane that answers inside the delay never flashes bars");
    wait(cx, &opened, std::time::Duration::from_millis(20));
    assert!(present(cx, &opened, "terminal-skeleton"), "no frame came, so bars stand in for the tab");
    hear(cx, &opened, vec![frame(7, Side::First, "node")]);
    assert!(!present(cx, &opened, "terminal-skeleton"), "its first frame takes them away");
}

#[gpui_kit::test]
fn a_strip_with_more_tabs_than_room_scrolls_and_keeps_its_buttons_and_the_chosen_tab_in_view(cx: &mut TestAppContext) {
    let opened = open(cx);
    // Thirty tabs of 100+ px cannot fit 1000 px; the last is the one chosen.
    let tabs: Vec<ConsoleTab> = (1..=30).map(|pane| tab(pane, &format!("shell number {pane}"), "idle", pane == 30)).collect();
    hear(cx, &opened, vec![connected(), page(tabs, Some(30), None, Some(30))]);
    let bounds = |cx: &mut TestAppContext, id: &str| cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds()).unwrap();
    let console = bounds(cx, "console");
    for button in ["console-new", "console-menu", "console-hide"] {
        let at = bounds(cx, button);
        assert!(at.right() <= console.right() && at.left() >= console.left(), "{button} was pushed off the strip: {at:?} in {console:?}");
    }
    // Observed bounds are of the whole element, scrolled or not: what is shown is what is within the strip.
    let strip = bounds(cx, "console-tabs");
    assert!(strip.right() <= bounds(cx, "console-new").left() + gpui_kit::px(1.0), "the tabs are held to their own box");
    let chosen = bounds(cx, "console-tab-30");
    assert!(chosen.left() >= strip.left() && chosen.right() <= strip.right() + gpui_kit::px(1.0), "the tab chosen is scrolled into view: {chosen:?} in {strip:?}");
}
