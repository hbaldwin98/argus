use argus_desktop_core::connection::page::Event;
use argus_desktop_core::extensions::views::ExtensionsPage;
use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::{asked, blur, open, press};

fn chord(key: &str) -> ExtChord {
    ExtChord { key: key.into(), ctrl: false, alt: false, shift: false, meta: false, leader: false }
}

fn command(name: &str, key: &str, scope: &'static str) -> ExtCommandPage {
    ExtCommandPage {
        extension: "docker".into(),
        command: name.into(),
        title: name.into(),
        chords: vec![chord(key)],
        scope,
        view: None,
    }
}

/// The commands a session lists: one per scope on a free key, and one on a key the window binds.
pub(crate) fn listed() -> Event {
    let commands = vec![
        command("up", "y", ANYWHERE),
        command("down", "u", CHECKOUT),
        command("logs", "w", PANE),
        command("shadow", "3", ANYWHERE),
    ];
    Event::Extensions(Box::new(ExtensionsPage { commands, ..ExtensionsPage::default() }))
}

fn key(name: &str) -> PageKey {
    PageKey { key: name.into(), ..PageKey::default() }
}

/// SHELL-KEYS-21: a chord is the key it names, with the modifiers it names; a character carries
/// its own shift (`?` is typed with Shift), a named key does not.
#[test]
fn a_chord_is_its_key_with_its_modifiers_and_a_characters_shift_is_its_own() {
    assert!(matches(&chord("y"), &key("y")));
    assert!(!matches(&chord("y"), &key("u")));
    assert!(!matches(&chord("y"), &PageKey { ctrl: true, ..key("y") }), "a modifier the chord does not name");
    assert!(!matches(&ExtChord { alt: true, ..chord("y") }, &key("y")), "a modifier the key lacks");
    assert!(!matches(&chord("y"), &PageKey { meta: true, ..key("y") }));
    assert!(matches(&chord("?"), &PageKey { shift: true, ..key("?") }));
    assert!(!matches(&chord("ArrowUp"), &PageKey { shift: true, ..key("ArrowUp") }));
    assert!(matches(&ExtChord { shift: true, ..chord("ArrowUp") }, &PageKey { shift: true, ..key("ArrowUp") }));
}

/// SHELL-KEYS-21: the first command heard in the scopes asked is the key's; one in another scope is not.
#[test]
fn a_command_is_found_only_in_the_scopes_asked() {
    let commands = Commands::default();
    let Event::Extensions(page) = listed() else { unreachable!() };
    commands.set(page.commands);
    assert_eq!(commands.find(&[ANYWHERE], &key("y")), Some(("docker".into(), "up".into())));
    assert_eq!(commands.find(&[ANYWHERE], &key("u")), None);
    assert_eq!(commands.find(&[ANYWHERE, CHECKOUT], &key("u")), Some(("docker".into(), "down".into())));
    assert_eq!(commands.find(&[ANYWHERE, CHECKOUT], &key("w")), None);
}

fn hear(cx: &mut TestAppContext, opened: &crate::keys::root::tests::Opened) {
    opened.events.send(vec![listed()]).unwrap();
    cx.run_until_parked();
}

/// SHELL-KEYS-17, 21: a key the stage binds nothing to runs the extension command anywhere that
/// has it, with no row picked; a command scoped to a checkout or a pane is not heard there.
#[gpui_kit::test]
fn the_stage_runs_an_anywhere_command_and_hears_no_other_scope(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened);
    blur(cx, &opened);
    press(cx, &opened, "y");
    assert_eq!(asked(&opened), ["ExtensionCommand { extension: \"docker\", command: \"up\", checkout: None, pane: None }"]);
    press(cx, &opened, "u");
    press(cx, &opened, "w");
    assert_eq!(asked(&opened).len(), 1, "the checkout's and the pane's commands are the rail's: {:?}", asked(&opened));
}

/// SHELL-KEYS-21: the window's own keys come first, so a command cannot shadow one.
#[gpui_kit::test]
fn a_key_the_window_binds_is_not_an_extension_command(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened);
    blur(cx, &opened);
    press(cx, &opened, "3");
    assert_eq!(asked(&opened), ["ShowCheckouts { open: true }"]);
}

/// A pane with the keys is sent every key; the commands are for the layers that bind keys.
#[gpui_kit::test]
fn a_pane_with_the_keys_is_sent_a_commands_key_as_a_key(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened);
    press(cx, &opened, "y");
    assert!(asked(&opened).iter().all(|call| call.starts_with("Key {")), "{:?}", asked(&opened));
}
