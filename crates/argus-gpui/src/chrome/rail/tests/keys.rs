//! Who has the keys, and what a key does while a row is picked.

use super::*;

fn keys_sent(opened: &Opened) -> usize {
    opened.calls.borrow().iter().filter(|call| matches!(call, Call::Key { .. })).count()
}

#[gpui_kit::test]
fn a_key_after_a_pane_row_click_is_the_rails_and_never_typed_into_the_shell(cx: &mut TestAppContext) {
    // SHELL-RAIL-13, SHELL-KEYS-16.
    let opened = open_shell(cx);
    type_key(cx, &opened, "a");
    assert_eq!(keys_sent(&opened), 1, "the stage has the keys at open");
    click(cx, &opened, "pane-2");
    type_key(cx, &opened, "a");
    assert_eq!(keys_sent(&opened), 1, "a key the rail has no action for is held");
    type_key(cx, &opened, "x");
    assert_eq!(keys_sent(&opened), 1);
    assert!(taken(&opened).contains(&"Close { pane: 2 }".to_owned()), "x closes the picked pane");
}

#[gpui_kit::test]
fn enter_on_a_picked_pane_gives_the_stage_the_keys_again(cx: &mut TestAppContext) {
    // SHELL-RAIL-24.
    let opened = open_shell(cx);
    click(cx, &opened, "pane-2");
    type_key(cx, &opened, "enter");
    type_key(cx, &opened, "a");
    assert_eq!(keys_sent(&opened), 1);
}

#[gpui_kit::test]
fn escape_unpicks_and_gives_the_stage_the_keys(cx: &mut TestAppContext) {
    // SHELL-RAIL-27.
    let opened = open_shell(cx);
    click(cx, &opened, "pane-2");
    type_key(cx, &opened, "escape");
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
    click(cx, &opened, "checkout-110");
    assert_eq!(picked(cx, &opened), Some(Picked { checkout: 110, pane: None }));
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 1, "the Terminal heard a key while a checkout was picked");
}

#[gpui_kit::test]
fn no_key_reaches_the_shell_while_a_row_is_picked(cx: &mut TestAppContext) {
    // SHELL-KEYS-16: every key the rail has no action for is held.
    let opened = open_showing(cx);
    click(cx, &opened, "checkout-110");
    for key in ["a", "j", "k", "s", "z", "space", "tab", "ctrl-c", "enter", "backspace"] {
        type_key(cx, &opened, key);
    }
    assert_eq!(typed(&opened), 1, "a key was typed into the shell: {:?}", taken(&opened));
}

#[gpui_kit::test]
fn x_on_a_picked_pane_closes_it_and_the_pick_falls_back_to_its_checkout(cx: &mut TestAppContext) {
    // SHELL-RAIL-23.
    let opened = open_showing(cx);
    click(cx, &opened, "pane-2");
    assert_eq!(picked(cx, &opened), Some(Picked { checkout: 110, pane: Some(2) }));
    type_key(cx, &opened, "x");
    assert!(taken(&opened).contains(&"Close { pane: 2 }".to_owned()), "{:?}", taken(&opened));
    assert_eq!(picked(cx, &opened), Some(Picked { checkout: 110, pane: None }));
    // The row stays until the daemon's tree drops it, and the checkout keeps the keys.
    assert!(present(cx, &opened, "pane-2"));
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 1);
}

#[gpui_kit::test]
fn losing_the_focus_unpicks_the_row(cx: &mut TestAppContext) {
    // SHELL-RAIL-19: the pick holds the keys, so it goes when they do.
    let opened = open_showing(cx);
    click(cx, &opened, "pane-2");
    assert!(picked(cx, &opened).is_some());
    blur(cx, &opened);
    assert_eq!(picked(cx, &opened), None);
}

#[gpui_kit::test]
fn a_press_on_the_stage_header_unpicks_and_gives_the_terminal_the_keys(cx: &mut TestAppContext) {
    // SHELL-SEAM-14, SHELL-RAIL-19.
    let opened = open_showing(cx);
    click(cx, &opened, "pane-2");
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 1);
    press(cx, &opened, point(px(600.0), px(17.0)), MouseButton::Left);
    assert_eq!(picked(cx, &opened), None);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 2, "a press on the header left the keys on the rail");
}

#[gpui_kit::test]
fn a_press_outside_the_rail_with_no_pane_shown_still_unpicks(cx: &mut TestAppContext) {
    let opened = open_shell(cx);
    click(cx, &opened, "checkout-110");
    assert!(picked(cx, &opened).is_some());
    press(cx, &opened, point(px(600.0), px(17.0)), MouseButton::Left);
    assert_eq!(picked(cx, &opened), None);
}

#[gpui_kit::test]
fn a_pane_newly_shown_takes_the_keys_only_when_no_row_is_picked(cx: &mut TestAppContext) {
    // SHELL-SEAM-9 g, f.
    let opened = open_showing(cx);
    click(cx, &opened, "checkout-110");
    blur(cx, &opened);
    assert_eq!(picked(cx, &opened), None);
    hear(cx, &opened, vec![showing(2, Stage::Workspace)]);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 2, "nothing picked: a new pane takes the keys");

    click(cx, &opened, "checkout-110");
    hear(cx, &opened, vec![showing(3, Stage::Workspace)]);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 2, "a row picked: the keys stay on the rail");
}

#[gpui_kit::test]
fn a_stage_change_with_a_pane_shown_takes_the_keys_only_when_no_row_is_picked(cx: &mut TestAppContext) {
    // SHELL-SEAM-9 g, f: the same pane, a different stage.
    let opened = open_showing(cx);
    click(cx, &opened, "checkout-110");
    blur(cx, &opened);
    hear(cx, &opened, vec![showing(1, Stage::Checkouts)]);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 2, "nothing picked: a stage change hands the Terminal the keys");

    click(cx, &opened, "checkout-110");
    hear(cx, &opened, vec![showing(1, Stage::Workspace)]);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 2, "a row picked: the keys stay on the rail");
}

#[gpui_kit::test]
fn the_composer_and_the_feature_stage_keep_the_keys_from_the_terminal(cx: &mut TestAppContext) {
    // SHELL-SEAM-9 d, e.
    for stage in [Stage::Composer, Stage::Feature] {
        let opened = open_showing(cx);
        click(cx, &opened, "checkout-110");
        blur(cx, &opened);
        hear(cx, &opened, vec![showing(1, stage)]);
        type_key(cx, &opened, "a");
        assert_eq!(typed(&opened), 1, "{stage:?} handed the Terminal the keys");
    }
}

#[gpui_kit::test]
fn a_capital_x_closes_a_picked_pane_as_the_table_binds_it(cx: &mut TestAppContext) {
    // SHELL-KEYS-16: the rail's bindings are the table's `Rail` place, which binds `X` as well as `x`.
    let opened = open_showing(cx);
    click(cx, &opened, "pane-2");
    type_key(cx, &opened, "shift-x");
    assert!(taken(&opened).contains(&"Close { pane: 2 }".to_owned()), "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_stage_key_runs_while_a_row_is_picked_and_the_row_keeps_the_keys(cx: &mut TestAppContext) {
    // SHELL-KEYS-16: a key with an action the rail does not answer is the stage's `run`.
    let opened = open_showing(cx);
    click(cx, &opened, "checkout-110");
    type_key(cx, &opened, "3");
    assert!(taken(&opened).contains(&"ShowCheckouts { open: true }".to_owned()), "{:?}", taken(&opened));
    assert_eq!(picked(cx, &opened), Some(Picked { checkout: 110, pane: None }), "running it did not let the pick go");
    assert_eq!(typed(&opened), 1, "and nothing was typed into the shell");
}

#[gpui_kit::test]
fn escape_hands_the_keys_back_through_the_tables_close_layer(cx: &mut TestAppContext) {
    // SHELL-RAIL-24, 27.
    let opened = open_showing(cx);
    click(cx, &opened, "checkout-110");
    type_key(cx, &opened, "escape");
    assert_eq!(picked(cx, &opened), None);
    type_key(cx, &opened, "a");
    assert_eq!(typed(&opened), 2, "the Terminal has the keys again");
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
    click(cx, &opened, "pane-2");
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
    click(cx, &opened, "checkout-110");
    cx.update(|cx| assert_eq!(opened.rail.read(cx).picked(), Some(Picked { checkout: 110, pane: None })));
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
    click(cx, &opened, "pane-2");
    type_key(cx, &opened, "3");
    assert!(taken(&opened).iter().all(|call| !call.starts_with("ExtensionCommand")), "{:?}", taken(&opened));
}
