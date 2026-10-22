//! Who has the keys, and what a key does while a row is picked.
//!
//! The pick is the session's, so a test that picks a row clicks it and then hears the rail event
//! that says so (`picked_event`), as the window does; and what a key does is the call it sends, since
//! the session's own answer is `chrome::cursor`'s and `session`'s tests.

use super::*;

fn keys_sent(opened: &Opened) -> usize {
    opened.calls.borrow().iter().filter(|call| matches!(call, Call::Key { .. })).count()
}

/// The session's word that a row is picked, with pane 1 still on screen.
fn picked_event(checkout: u64, pane: Option<u64>) -> Event {
    let Event::Rail(mut rail) = showing(1, Stage::Workspace) else { unreachable!() };
    rail.cursor.pick = Some(Pick { checkout, pane });
    Event::Rail(rail)
}

/// Clicks a row and hears the session pick it.
fn pick_pane(cx: &mut TestAppContext, opened: &Opened, checkout: u64, pane: u64) {
    click(cx, opened, &format!("pane-{pane}"));
    hear(cx, opened, vec![picked_event(checkout, Some(pane))]);
}

fn pick_checkout(cx: &mut TestAppContext, opened: &Opened, checkout: u64) {
    click(cx, opened, &format!("checkout-{checkout}"));
    hear(cx, opened, vec![picked_event(checkout, None)]);
}

#[gpui_kit::test]
fn a_key_after_a_pane_row_click_is_the_rails_and_never_typed_into_the_shell(cx: &mut TestAppContext) {
    // SHELL-RAIL-13, SHELL-KEYS-16.
    let opened = open_shell(cx);
    type_key(cx, &opened, "a");
    assert_eq!(keys_sent(&opened), 1, "the stage has the keys at open");
    pick_pane(cx, &opened, 110, 2);
    forget(&opened);
    type_key(cx, &opened, "q");
    assert!(taken(&opened).is_empty(), "a key the rail has no action for is held: {:?}", taken(&opened));
    type_key(cx, &opened, "x");
    assert_eq!(taken(&opened), ["RailKey { action: Close }"], "x is the rail's, sent to the session as it is");
}

#[gpui_kit::test]
fn the_keys_of_the_rail_place_are_sent_to_the_session_as_the_action_they_are(cx: &mut TestAppContext) {
    // SHELL-RAIL-20..27: what each does is the cursor's, which the session and core tests hold.
    let opened = open_shell(cx);
    pick_checkout(cx, &opened, 110);
    forget(&opened);
    for (key, action) in [
        ("j", "Next"),
        ("down", "Next"),
        ("k", "Previous"),
        ("up", "Previous"),
        ("s", "StartShell"),
        ("shift-a", "StartAgent"),
        ("r", "OpenReview"),
        ("h", "OpenHistory"),
        ("z", "Fold"),
        ("x", "Close"),
        ("enter", "Choose"),
    ] {
        type_key(cx, &opened, key);
        assert_eq!(taken(&opened).last().map(String::as_str), Some(format!("RailKey {{ action: {action} }}").as_str()), "{key}");
    }
    assert_eq!(keys_sent(&opened), 0, "none was typed into the shell");
}

#[gpui_kit::test]
fn enter_on_a_picked_pane_gives_the_stage_the_keys_once_the_session_lets_the_pick_go(cx: &mut TestAppContext) {
    // SHELL-RAIL-24: the session lets it go; the keys follow it, not the press.
    let opened = open_shell(cx);
    hear(cx, &opened, vec![showing(1, Stage::Workspace)]);
    pick_pane(cx, &opened, 110, 2);
    type_key(cx, &opened, "enter");
    assert_eq!(taken(&opened).last().map(String::as_str), Some("RailKey { action: Choose }"));
    type_key(cx, &opened, "a");
    assert_eq!(keys_sent(&opened), 0, "until the session says so, the rail still has them");
    hear(cx, &opened, vec![showing(1, Stage::Workspace)]);
    type_key(cx, &opened, "a");
    assert_eq!(keys_sent(&opened), 1, "the pick was let go with the rail holding the keys: they go to the stage");
}

#[gpui_kit::test]
fn escape_is_the_close_layer_action_and_gives_the_stage_the_keys_when_the_pick_is_let_go(cx: &mut TestAppContext) {
    // SHELL-RAIL-27.
    let opened = open_shell(cx);
    hear(cx, &opened, vec![showing(1, Stage::Workspace)]);
    pick_pane(cx, &opened, 110, 2);
    type_key(cx, &opened, "escape");
    assert_eq!(taken(&opened).last().map(String::as_str), Some("RailKey { action: CloseLayer }"));
    hear(cx, &opened, vec![showing(1, Stage::Workspace)]);
    type_key(cx, &opened, "a");
    assert_eq!(keys_sent(&opened), 1);
}

fn showing(pane: u64, stage: Stage) -> Event {
    let mut rail = busy();
    rail.stage = stage;
    rail.shown = Some(Shown {
        pane,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "main".into(),
        agent: false,
        browser: false,
        title: "zsh".into(),
        status: "idle",
        resumed: None,
    });
    Event::Rail(Box::new(rail))
}

/// `showing` with a row picked, as the session says it.
fn showing_picked(pane: u64, stage: Stage, checkout: u64) -> Event {
    let Event::Rail(mut rail) = showing(pane, stage) else { unreachable!() };
    rail.cursor.pick = Some(Pick { checkout, pane: None });
    Event::Rail(rail)
}

/// Key and text calls: what reaches a shell.
fn typed(opened: &Opened) -> usize {
    opened.calls.borrow().iter().filter(|call| matches!(call, Call::Key { .. } | Call::Text { .. })).count()
}

/// Takes the keys from everything, as a press on nothing would.
fn blur(cx: &mut TestAppContext, opened: &Opened) {
    cx.update_window(opened.handle, |_, window, cx| window.blur(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// An open shell showing pane 1, whose Terminal has the keys.
fn open_showing(cx: &mut TestAppContext) -> Opened {
    let opened = open_shell(cx);
    hear(cx, &opened, vec![showing(1, Stage::Workspace)]);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 1, "the Terminal has the keys while nothing is picked");
    opened
}

#[gpui_kit::test]
fn a_checkout_row_click_takes_the_keys_off_the_terminal(cx: &mut TestAppContext) {
    // SHELL-RAIL-19.
    let opened = open_showing(cx);
    pick_checkout(cx, &opened, 110);
    assert_eq!(picked(cx, &opened), Some((110, None)));
    type_key(cx, &opened, "q");
    assert_eq!(typed(&opened), 1, "the Terminal heard a key while a checkout was picked");
}

#[gpui_kit::test]
fn no_key_reaches_the_shell_while_a_row_is_picked(cx: &mut TestAppContext) {
    // SHELL-KEYS-16: every key the rail has no action for is held.
    let opened = open_showing(cx);
    pick_checkout(cx, &opened, 110);
    for key in ["q", "j", "k", "s", "z", "space", "tab", "ctrl-c", "enter", "backspace"] {
        type_key(cx, &opened, key);
    }
    assert_eq!(typed(&opened), 1, "a key was typed into the shell: {:?}", taken(&opened));
}

#[gpui_kit::test]
fn losing_the_focus_lets_the_pick_go_without_taking_the_keys_back_from_where_they_went(cx: &mut TestAppContext) {
    // SHELL-RAIL-19: the pick holds the keys, so it goes when they do; the cursor is the session's.
    let opened = open_showing(cx);
    pick_pane(cx, &opened, 110, 2);
    forget(&opened);
    blur(cx, &opened);
    assert_eq!(taken(&opened), ["RailUnpick"]);
}

#[gpui_kit::test]
fn the_pick_is_let_go_once_not_for_every_loss_of_focus(cx: &mut TestAppContext) {
    let opened = open_showing(cx);
    forget(&opened);
    blur(cx, &opened);
    assert!(taken(&opened).is_empty(), "nothing was picked: {:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_press_on_the_stage_header_lets_the_pick_go_and_gives_the_terminal_the_keys(cx: &mut TestAppContext) {
    // SHELL-SEAM-14, SHELL-RAIL-19.
    let opened = open_showing(cx);
    pick_pane(cx, &opened, 110, 2);
    type_key(cx, &opened, "q");
    assert_eq!(typed(&opened), 1);
    forget(&opened);
    press(cx, &opened, point(px(600.0), px(17.0)), MouseButton::Left);
    assert!(taken(&opened).contains(&"RailUnpick".to_owned()), "{:?}", taken(&opened));
    hear(cx, &opened, vec![showing(1, Stage::Workspace)]);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 1, "a press on the header left the keys on the rail (the calls were forgotten before it)");
}

#[gpui_kit::test]
fn a_press_outside_the_rail_with_nothing_picked_asks_nothing_of_the_cursor(cx: &mut TestAppContext) {
    let opened = open_shell(cx);
    forget(&opened);
    press(cx, &opened, point(px(600.0), px(17.0)), MouseButton::Left);
    assert!(!taken(&opened).contains(&"RailUnpick".to_owned()), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_pane_newly_shown_takes_the_keys_only_when_no_row_is_picked(cx: &mut TestAppContext) {
    // SHELL-SEAM-9 g, f.
    let opened = open_showing(cx);
    hear(cx, &opened, vec![picking(110, None)]);
    blur(cx, &opened);
    hear(cx, &opened, vec![showing(2, Stage::Workspace)]);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 2, "nothing picked: a new pane takes the keys");

    click(cx, &opened, "checkout-110");
    hear(cx, &opened, vec![showing_picked(3, Stage::Workspace, 110)]);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 2, "a row picked: the keys stay on the rail");
}

#[gpui_kit::test]
fn a_stage_change_with_a_pane_shown_takes_the_keys_only_when_no_row_is_picked(cx: &mut TestAppContext) {
    // SHELL-SEAM-9 g, f: the same pane, a different stage.
    let opened = open_showing(cx);
    blur(cx, &opened);
    // The Browsers stage, which has no surface of its own yet: the Checkouts and Code stages take the keys themselves.
    hear(cx, &opened, vec![showing(1, Stage::Browsers)]);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 2, "nothing picked: a stage change hands the Terminal the keys");

    click(cx, &opened, "checkout-110");
    hear(cx, &opened, vec![showing_picked(1, Stage::Workspace, 110)]);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 2, "a row picked: the keys stay on the rail");
}

#[gpui_kit::test]
fn the_composer_and_the_feature_stage_keep_the_keys_from_the_terminal(cx: &mut TestAppContext) {
    // SHELL-SEAM-9 d, e.
    for stage in [Stage::Composer, Stage::Feature] {
        let opened = open_showing(cx);
        blur(cx, &opened);
        hear(cx, &opened, vec![showing(1, stage)]);
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 1, "{stage:?} handed the Terminal the keys");
    }
}

#[gpui_kit::test]
fn a_capital_x_is_the_close_action_as_the_table_binds_it(cx: &mut TestAppContext) {
    // SHELL-KEYS-16: the rail's bindings are the table's `Rail` place, which binds `X` as well as `x`.
    let opened = open_showing(cx);
    pick_pane(cx, &opened, 110, 2);
    forget(&opened);
    type_key(cx, &opened, "shift-x");
    assert_eq!(taken(&opened), ["RailKey { action: Close }"]);
}

#[gpui_kit::test]
fn a_stage_key_runs_while_a_row_is_picked_and_the_row_keeps_the_keys(cx: &mut TestAppContext) {
    // SHELL-KEYS-16: a key with an action the rail does not answer is the stage's `run`.
    let opened = open_showing(cx);
    pick_checkout(cx, &opened, 110);
    type_key(cx, &opened, "3");
    assert!(taken(&opened).contains(&"ShowCheckouts { open: true }".to_owned()), "{:?}", taken(&opened));
    assert_eq!(picked(cx, &opened), Some((110, None)), "running it did not let the pick go");
    assert_eq!(typed(&opened), 1, "and nothing was typed into the shell");
}

#[gpui_kit::test]
fn a_held_rail_answers_the_keys_that_need_the_daemon_with_nothing(cx: &mut TestAppContext) {
    // The daemon is gone: closing a pane or starting something would reach no one, and is not queued.
    let opened = open_showing(cx);
    pick_pane(cx, &opened, 110, 2);
    // Reconnecting holds the rail as a stopped daemon does, without the card that would take the keys.
    hear(cx, &opened, vec![status(Connection::Connecting)]);
    forget(&opened);
    for key in ["x", "s", "r"] {
        type_key(cx, &opened, key);
    }
    assert!(taken(&opened).is_empty(), "a held rail sent {:?}", taken(&opened));
    type_key(cx, &opened, "escape");
    assert_eq!(taken(&opened), ["RailKey { action: CloseLayer }"], "letting go is the window's own");
}

fn extensions(cx: &mut TestAppContext, opened: &Opened) {
    hear(cx, opened, vec![crate::keys::extension::tests::listed()]);
}

/// SHELL-KEYS-16, 21: a key the picked pane's rail binds nothing to runs the extension command it
/// is, in every scope there is: anywhere, the checkout's, and the pane's, with the row picked.
#[gpui_kit::test]
fn a_picked_pane_runs_the_commands_of_every_scope_with_its_row(cx: &mut TestAppContext) {
    let opened = open_shell(cx);
    extensions(cx, &opened);
    pick_pane(cx, &opened, 110, 2);
    for (key, command) in [("y", "up"), ("u", "down"), ("w", "logs")] {
        type_key(cx, &opened, key);
        let sent = format!("ExtensionCommand {{ extension: \"docker\", command: \"{command}\", checkout: Some(110), pane: Some(2) }}");
        assert_eq!(taken(&opened).last(), Some(&sent), "{key}");
    }
    assert_eq!(keys_sent(&opened), 0, "none was typed into the shell");
}

/// A picked checkout has no pane, so a pane's command is not heard.
#[gpui_kit::test]
fn a_picked_checkout_hears_no_panes_command(cx: &mut TestAppContext) {
    let opened = open_shell(cx);
    extensions(cx, &opened);
    pick_checkout(cx, &opened, 110);
    let before = taken(&opened).len();
    type_key(cx, &opened, "w");
    type_key(cx, &opened, "u");
    let sent: Vec<_> = taken(&opened).split_off(before);
    assert_eq!(sent, ["ExtensionCommand { extension: \"docker\", command: \"down\", checkout: Some(110), pane: None }"]);
}

/// A key the rail binds is its action, not a command that asks for the same key.
#[gpui_kit::test]
fn a_key_the_picked_rail_binds_is_not_an_extension_command(cx: &mut TestAppContext) {
    let opened = open_shell(cx);
    extensions(cx, &opened);
    pick_pane(cx, &opened, 110, 2);
    type_key(cx, &opened, "3");
    assert!(taken(&opened).iter().all(|call| !call.starts_with("ExtensionCommand")), "{:?}", taken(&opened));
}
