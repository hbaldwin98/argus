//! What the strip's foot says of the connection: the server dot, its ring, its menu and the gear.

use super::*;

fn alarmed(alarm: Option<&str>, chime: bool) -> Event {
    Event::Status(Status { connection: Connection::Connected, detail: None, alarm: alarm.map(str::to_owned), chime, host: None })
}

#[gpui_kit::test]
fn the_dot_names_the_connection_to_a_reader_that_cannot_see_it(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · local"));
    hear(cx, &opened, vec![status(Connection::Connecting)]);
    assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · reconnecting"));
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · stopped"));
}

#[gpui_kit::test]
fn a_connection_before_any_tree_says_connecting(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connecting)]);
    assert_eq!(label(cx, &opened, "foot-status").as_deref(), Some("argusd · connecting"));
}

#[gpui_kit::test]
fn the_tallies_are_the_chips_and_workspaces_badge_and_the_foot_has_no_text(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "rail-filter-need").as_deref(), Some("1 need you"));
    assert_eq!(label(cx, &opened, "rail-filter-working").as_deref(), Some("1 working"));
    assert_eq!(label(cx, &opened, "nav-workspace").as_deref(), Some("Workspace 1 needs you"));
    for gone in ["foot-need-you", "foot-working", "foot-alarm", "foot-console", "foot-chime"] {
        assert!(!present(cx, &opened, gone), "{gone}");
    }
}

#[gpui_kit::test]
fn a_ring_marks_the_dot_only_while_the_daemon_is_another_build(cx: &mut TestAppContext) {
    let opened = open(cx, vec![alarmed(None, true), Event::Rail(Box::new(busy()))]);
    let plain = cx.update_window(opened.handle, |_, window, _| window.find("foot-ring".to_owned()).bounds()).unwrap();
    hear(cx, &opened, vec![alarmed(Some("argusd 0.1, window 0.2: run `argus server restart`"), true)]);
    let ringed = cx.update_window(opened.handle, |_, window, _| window.find("foot-ring".to_owned()).bounds()).unwrap();
    assert_eq!(plain, ringed, "the ring is drawn in the space the dot always has, so nothing moves");
    assert!(present(cx, &opened, "foot-dot"));
}

#[gpui_kit::test]
fn the_dots_menu_asks_for_the_console_and_the_opposite_of_the_chime_heard(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    click(cx, &opened, "foot-status");
    menus::choose(cx, &opened, "Mute the chime when an agent needs you");
    assert_eq!(taken(&opened), ["SetChime { on: false }"]);
    click(cx, &opened, "foot-status");
    menus::choose(cx, &opened, "Show the console");
    assert_eq!(taken(&opened), ["SetChime { on: false }", "Console { gesture: Toggle }"]);
    hear(cx, &opened, vec![alarmed(None, false)]);
    click(cx, &opened, "foot-status");
    menus::choose(cx, &opened, "Chime when an agent needs you");
    assert_eq!(taken(&opened).last().map(String::as_str), Some("SetChime { on: true }"));
}

#[gpui_kit::test]
fn the_dots_menu_sends_nothing_while_the_daemon_is_not_there(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    click(cx, &opened, "foot-status");
    menus::choose(cx, &opened, "Show the console");
    assert!(taken(&opened).is_empty(), "a held menu sent {:?}", taken(&opened));
}

#[gpui_kit::test]
fn the_console_count_is_a_badge_on_the_strip(cx: &mut TestAppContext) {
    let mut events = connected_and_busy();
    events.push(Event::Console(Box::new(argus_desktop_core::panes::console::ConsolePage {
        tabs: (1..=2).map(|pane| argus_desktop_core::panes::console::ConsoleTab { pane, title: "pwsh".into(), status: "idle", on: false }).collect(),
        ..Default::default()
    })));
    let opened = open(cx, events);
    assert_eq!(label(cx, &opened, "nav-console").as_deref(), Some("Console, 2 open"));
    assert!(present(cx, &opened, "nav-console-badge"));
    assert!(!present(cx, &opened, "nav-code-badge"), "no count, no badge");
}

#[gpui_kit::test]
fn the_settings_gear_opens_the_settings_whether_or_not_the_daemon_is_there(cx: &mut TestAppContext) {
    // SHELL-FOOT-6: the gear is the window's own, so it is not held with the daemon.
    use crate::kit::overlay::host::Overlays;
    use crate::kit::overlay::kind::Overlay;
    let mut events = connected_and_busy();
    events.push(crate::keys::root::tests::settings_event());
    let opened = open(cx, events);
    assert_eq!(label(cx, &opened, "foot-settings").as_deref(), Some("Settings"));
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    click(cx, &opened, "foot-settings");
    assert!(cx.update(|cx| Overlays::get(cx).is_open(Overlay::Settings)), "the settings did not open");
    assert!(taken(&opened).is_empty(), "the gear asked the daemon something: {:?}", taken(&opened));
}

#[test]
fn the_tooltips_name_the_keys_the_keymap_gives_each_stage_and_the_gear() {
    // SHELL-FOOT-4, 6: the keys are the keymap's, so none are named before it has been heard.
    use argus_desktop_core::keys::keymap::{commands, groups, LAYERS};
    let mut heard = Heard::new();
    heard.hear(status(Connection::Connected));
    heard.hear(Event::Rail(Box::new(busy())));
    let before = super::super::slice::Slice::of(&heard, None);
    assert_eq!((before.server.console_title.as_str(), before.server.settings_title.as_str()), ("Show the console", "Settings"));
    heard.hear(Event::Keymap { groups: groups(), commands: commands(), layers: LAYERS });
    let after = super::super::slice::Slice::of(&heard, Some(&before));
    assert_eq!(after.server.console_title, "Show the console (`)");
    assert!(after.server.settings_title.starts_with("Settings (") && after.server.settings_title.contains("Ctrl"), "{}", after.server.settings_title);
    assert!(after.titles.iter().any(|title| title.starts_with("Workspace (")), "{:?}", after.titles);
}
