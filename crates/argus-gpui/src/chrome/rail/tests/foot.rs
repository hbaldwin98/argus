//! What the foot says of the connection, and the buttons in it.

use super::*;

#[gpui_kit::test]
fn the_foot_counts_what_needs_you_while_connected(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "foot-need-you").as_deref(), Some("▲ 1 need you"));
    assert_eq!(label(cx, &opened, "foot-working").as_deref(), Some("⋮ 1 working"));
    assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · local"));
}

#[gpui_kit::test]
fn the_foot_says_reconnecting_and_the_rail_goes_on_showing_the_last_tree_held(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    hear(cx, &opened, vec![status(Connection::Connecting)]);
    assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · reconnecting"));
    assert!(!present(cx, &opened, "foot-need-you"), "tallies are for a connected daemon");
    // The last tree stays; it answers no press, since nothing would reach a daemon.
    assert!(present(cx, &opened, "pane-2"));
    click(cx, &opened, "pane-2");
    click(cx, &opened, "nav-checkouts");
    assert!(taken(&opened).is_empty(), "a held rail sent {:?}", taken(&opened));
    cx.update(|cx| assert_eq!(opened.rail.read(cx).picked(), None, "a held rail picked a row"));
    // Back, it answers again.
    hear(cx, &opened, vec![status(Connection::Connected)]);
    click(cx, &opened, "pane-2");
    assert_eq!(taken(&opened), ["RailPick { checkout: 110, pane: Some(2) }", "Select { pane: 2 }"]);
}

#[gpui_kit::test]
fn the_foot_says_stopped_and_holds_the_rail(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · stopped"));
    click(cx, &opened, "pane-2");
    assert!(taken(&opened).is_empty());
}

#[gpui_kit::test]
fn a_connection_before_any_tree_says_connecting(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connecting)]);
    assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · connecting"));
}

#[gpui_kit::test]
fn the_foot_names_a_remote_host_and_shows_the_daemons_alarm(cx: &mut TestAppContext) {
    let status = Event::Status(Status {
        connection: Connection::Connected,
        detail: None,
        alarm: Some("argusd 0.1, window 0.2: run argus server restart".into()),
        chime: true,
        host: Some("dev-box".into()),
    });
    let opened = open(cx, vec![status, Event::Rail(Box::new(busy()))]);
    assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · dev-box"));
    assert_eq!(
        label(cx, &opened, "foot-alarm").as_deref(),
        Some("argusd 0.1, window 0.2: run argus server restart")
    );
}

fn console_at(place: ConsolePlace) -> Event {
    Event::Console(Box::new(argus_desktop_core::panes::console::ConsolePage { place, ..Default::default() }))
}

fn with_chime(chime: bool) -> Event {
    Event::Status(Status { connection: Connection::Connected, detail: None, alarm: None, chime, host: None })
}

// SHELL-FOOT: the page's status bar toggles the console and the chime by call,
// and draws from what the daemon said, never from the press.
#[gpui_kit::test]
fn the_foot_console_chip_asks_for_the_toggle_and_is_named_by_what_was_heard(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "foot-console").as_deref(), Some("Show the console"));
    click(cx, &opened, "foot-console");
    assert_eq!(taken(&opened), ["Console { gesture: Toggle }"]);
    assert_eq!(
        label(cx, &opened, "foot-console").as_deref(),
        Some("Show the console"),
        "the press alone does not show the console"
    );
    hear(cx, &opened, vec![console_at(ConsolePlace::Docked)]);
    assert_eq!(label(cx, &opened, "foot-console").as_deref(), Some("Hide the console"));
    click(cx, &opened, "foot-console");
    assert_eq!(taken(&opened), ["Console { gesture: Toggle }", "Console { gesture: Toggle }"]);
}

#[gpui_kit::test]
fn the_foot_chime_switch_asks_for_the_opposite_of_what_was_heard(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "foot-chime").as_deref(), Some("Mute the chime when an agent needs you"));
    click(cx, &opened, "foot-chime");
    assert_eq!(taken(&opened), ["SetChime { on: false }"]);
    assert_eq!(
        label(cx, &opened, "foot-chime").as_deref(),
        Some("Mute the chime when an agent needs you"),
        "the bell follows the daemon's word, not the press"
    );
    hear(cx, &opened, vec![with_chime(false)]);
    assert_eq!(label(cx, &opened, "foot-chime").as_deref(), Some("Chime when an agent needs you"));
    click(cx, &opened, "foot-chime");
    assert_eq!(taken(&opened), ["SetChime { on: false }", "SetChime { on: true }"]);
}

#[gpui_kit::test]
fn the_foot_buttons_send_nothing_while_the_daemon_is_not_there(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    click(cx, &opened, "foot-console");
    click(cx, &opened, "foot-chime");
    assert!(taken(&opened).is_empty(), "a held foot sent {:?}", taken(&opened));
}

#[gpui_kit::test]
fn the_settings_gear_opens_the_settings_whether_or_not_the_daemon_is_there(cx: &mut TestAppContext) {
    // SHELL-FOOT-6: the gear is the window's own, so it is not held with the daemon.
    use crate::kit::overlay::host::Overlays;
    use crate::kit::overlay::kind::Overlay;
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "foot-settings").as_deref(), Some("Settings"));
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    click(cx, &opened, "foot-settings");
    assert!(cx.update(|cx| Overlays::get(cx).is_open(Overlay::Settings)), "the settings did not open");
    assert!(taken(&opened).is_empty(), "the gear asked the daemon something: {:?}", taken(&opened));
}

#[test]
fn the_footers_hover_text_names_the_keys_the_keymap_gives_each_button() {
    // SHELL-FOOT-4, 6: `Hide the console (\`)`, `Settings (Ctrl ,)`; the keys are the keymap's, so none are
    // named before it has been heard.
    use argus_desktop_core::keys::keymap::{commands, groups, LAYERS};
    let mut heard = Heard::new();
    heard.hear(status(Connection::Connected));
    heard.hear(Event::Rail(Box::new(busy())));
    let before = super::super::slice::Slice::of(&heard, None).foot;
    assert_eq!((before.console_title.as_str(), before.settings_title.as_str()), ("Show the console", "Settings"));
    heard.hear(Event::Keymap { groups: groups(), commands: commands(), layers: LAYERS });
    let after = super::super::slice::Slice::of(&heard, Some(&super::super::slice::Slice::of(&heard, None))).foot;
    assert_eq!(after.console_title, "Show the console (`)");
    assert!(after.settings_title.starts_with("Settings (") && after.settings_title.contains("Ctrl"), "{}", after.settings_title);
}
