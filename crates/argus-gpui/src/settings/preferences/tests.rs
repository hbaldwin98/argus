//! The Settings sheet in a test window: what it lists, that every control sends its choice at once
//! and draws only what comes back, and the keys that close it.

use argus_desktop_core::connection::page::{BackdropOffer, Event, FontFamily, SettingsPage};
use argus_desktop_core::settings::remembered::{Backdrop, EditorMode, Preferences, Theme as Chosen};
use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{prelude::*, TestAppContext};

use crate::keys::over::Surfaces;
use crate::keys::root::tests::{blur, click_label, drain, has_label, hear, open, overlays, press, type_text, Opened};
use crate::kit::overlay::kind::Overlay;

fn offers(mica: Option<&str>) -> Vec<BackdropOffer> {
    vec![
        BackdropOffer { backdrop: Backdrop::Clear, unavailable: None },
        BackdropOffer { backdrop: Backdrop::Mica, unavailable: mica.map(str::to_owned) },
        BackdropOffer { backdrop: Backdrop::Acrylic, unavailable: None },
    ]
}

fn page(chosen: Preferences, backdrops: Vec<BackdropOffer>) -> Event {
    let font = |name: &str, mono| FontFamily { name: name.into(), mono };
    Event::Settings(Box::new(SettingsPage {
        chosen,
        min_opacity: 30,
        backdrops,
        fonts: vec![font("Cascadia Mono", true), font("Consolas", true), font("Segoe UI", false)],
        version: "9.9.9".into(),
    }))
}

fn sheet(cx: &mut TestAppContext, chosen: Preferences) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, vec![page(chosen, offers(None))]);
    blur(cx, &opened);
    press(cx, &opened, "ctrl-,");
    assert_eq!(overlays(cx), [Overlay::Settings]);
    drain(&opened);
    opened
}

fn state(cx: &mut TestAppContext, opened: &Opened, label: &str) -> (Option<bool>, Option<bool>) {
    cx.update_window(opened.handle, |_, window, _| {
        let found = window.find_by_label(label);
        (found.selected(), found.checked())
    })
    .unwrap()
}

fn asked(opened: &Opened) -> Vec<String> {
    drain(opened)
}

/// Scrolls the body to the top of section `at`, as a press on its name in the nav does, and draws it.
fn scroll_to(cx: &mut TestAppContext, opened: &Opened, at: usize) {
    let view = cx.update(|cx| Surfaces::get(cx).expect("the shell built the surfaces").settings);
    cx.update(|cx| view.update(cx, |view, cx| view.go(at, cx)));
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// Scrolls the body down `y` pixels from the top and draws it.
fn scroll_by(cx: &mut TestAppContext, opened: &Opened, y: f32) {
    let view = cx.update(|cx| Surfaces::get(cx).expect("the shell built the surfaces").settings);
    cx.update(|cx| view.update(cx, |view, cx| {
        view.body.set_offset(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(-y)));
        cx.notify();
    }));
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// SHELL-SETTINGS-1: six sections, each named in the nav and in the body.
#[gpui_kit::test]
fn the_sheet_has_its_six_sections_and_the_version(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences::default());
    for (at, name) in ["Appearance", "Editor", "Notifications", "Features", "Views", "Connection"].into_iter().enumerate() {
        let (nav, section) = cx
            .update_window(opened.handle, |_, window, _| (window.find(("settings-nav-item", at)).label().map(str::to_owned), window.find(("settings-section", at)).label().map(str::to_owned)))
            .unwrap();
        assert_eq!((nav.as_deref(), section.as_deref()), (Some(name), Some(name)), "section {at}");
    }
    assert!(has_label(cx, &opened, "Saved as you change them"));
    let title = cx.update_window(opened.handle, |_, window, _| window.find("settings".to_owned()).label().map(str::to_owned)).unwrap();
    assert_eq!(title.as_deref(), Some("Settings"));
}

/// SHELL-SETTINGS-1: it opens at the top, on Appearance, with its first section where the body starts.
#[gpui_kit::test]
fn the_sheet_opens_at_the_top_of_appearance(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences::default());
    let (section, body) = cx
        .update_window(opened.handle, |_, window, _| {
            (window.find(("settings-section", 0usize)).bounds().origin.y.as_f32(), window.find("settings-body".to_owned()).bounds().origin.y.as_f32())
        })
        .unwrap();
    assert!(section >= body, "the first section starts at {section}, above the body at {body}");
    assert!(section - body < 40.0, "the first section starts at {section}, the body at {body}");
}

/// SHELL-SETTINGS-5, 6: twelve cards, the one drawn lit, a press asking for another and drawing nothing
/// until the session says it.
#[gpui_kit::test]
fn a_card_asks_for_its_theme_and_the_lit_one_is_the_one_the_session_says(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences::default());
    let cards = ["Argus Night", "Ledger", "Carbon", "Flexoki", "Kanagawa", "Ros\u{e9} Pine", "Nord", "Radix", "Mocha", "Macchiato", "Frapp\u{e9}", "Latte"];
    // Latte is what the test platform's light system draws when nothing is chosen.
    for card in cards {
        assert_eq!(state(cx, &opened, card).0, Some(card == "Latte"), "{card}");
    }
    click_label(cx, &opened, "Nord");
    assert_eq!(asked(&opened), ["Setting { change: Theme { theme: Nord } }"]);
    assert_eq!(state(cx, &opened, "Nord").0, Some(false), "the client does not predict the result");
    hear(cx, &opened, vec![page(Preferences { theme: Chosen::Nord, ..Preferences::default() }, offers(None))]);
    assert_eq!(state(cx, &opened, "Nord").0, Some(true));
    assert_eq!(state(cx, &opened, "Latte").0, Some(false));
}

/// SHELL-SETTINGS-6: following the system is a switch; leaving it keeps the flavour that was drawn.
#[gpui_kit::test]
fn following_the_system_keeps_the_flavour_it_drew_when_it_is_left(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences::default());
    assert_eq!(state(cx, &opened, "Follow the system").1, Some(true));
    click_label(cx, &opened, "Follow the system");
    assert_eq!(asked(&opened), ["Setting { change: Theme { theme: Latte } }"], "the light system drew Latte");
    hear(cx, &opened, vec![page(Preferences { theme: Chosen::Frappe, ..Preferences::default() }, offers(None))]);
    assert_eq!(state(cx, &opened, "Follow the system").1, Some(false));
    click_label(cx, &opened, "Follow the system");
    assert_eq!(asked(&opened), ["Setting { change: Theme { theme: System } }"]);
}

/// SHELL-SETTINGS-6c: a backdrop this machine cannot draw is dimmed with its reason, and a press asks nothing.
#[gpui_kit::test]
fn a_backdrop_that_cannot_be_drawn_says_why_and_asks_nothing(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![page(Preferences::default(), offers(Some("Mica needs Windows 11")))]);
    blur(cx, &opened);
    press(cx, &opened, "ctrl-,");
    scroll_by(cx, &opened, 330.0);
    assert!(has_label(cx, &opened, "Mica"), "dimmed, not left out");
    click_label(cx, &opened, "Mica");
    assert!(asked(&opened).is_empty(), "Mica cannot be drawn here");
    click_label(cx, &opened, "Acrylic");
    assert_eq!(asked(&opened), ["Setting { change: Backdrop { backdrop: Acrylic } }"]);
}

/// SHELL-SETTINGS-6d: where Clear is all this system draws, there is no choice and the row is left out.
#[gpui_kit::test]
fn the_backdrop_row_is_left_out_where_clear_is_all_there_is(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![page(Preferences::default(), vec![BackdropOffer { backdrop: Backdrop::Clear, unavailable: None }])]);
    blur(cx, &opened);
    press(cx, &opened, "ctrl-,");
    assert!(!has_label(cx, &opened, "What shows through"));
    assert!(!has_label(cx, &opened, "Acrylic"));
}

/// SHELL-SETTINGS-7..10: the Editor section's choices, switches and command, each sent at once.
#[gpui_kit::test]
fn the_editor_section_sends_each_choice_and_draws_what_comes_back(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences::default());
    scroll_to(cx, &opened, 1);
    assert_eq!(state(cx, &opened, "Vim's keys in the Code stage").1, Some(true));
    assert_eq!(state(cx, &opened, "Install known grammars without asking").1, Some(false));
    click_label(cx, &opened, "Vim's keys in the Code stage");
    click_label(cx, &opened, "Install known grammars without asking");
    click_label(cx, &opened, "Separately");
    assert_eq!(
        asked(&opened),
        ["Setting { change: Vim { on: false } }", "Setting { change: InstallGrammars { on: true } }", "Setting { change: Editor { mode: Separately } }"]
    );
    assert_eq!(state(cx, &opened, "Vim's keys in the Code stage").1, Some(true), "nothing is drawn until the session says it");
    hear(cx, &opened, vec![page(Preferences { vim: false, install_grammars: true, editor: EditorMode::Separately, ..Preferences::default() }, offers(None))]);
    assert_eq!(state(cx, &opened, "Vim's keys in the Code stage").1, Some(false));
    assert_eq!(state(cx, &opened, "Install known grammars without asking").1, Some(true));
    assert_eq!(state(cx, &opened, "Separately").0, Some(true));
    assert_eq!(state(cx, &opened, "Floating").0, Some(false));
}

#[gpui_kit::test]
fn the_other_sections_send_theirs(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences::default());
    scroll_to(cx, &opened, 2);
    click_label(cx, &opened, "Notify when an agent needs you");
    click_label(cx, &opened, "Only while this window is in the background");
    scroll_to(cx, &opened, 3);
    click_label(cx, &opened, "Board");
    scroll_to(cx, &opened, 4);
    click_label(cx, &opened, "Column");
    assert_eq!(
        asked(&opened),
        [
            "Setting { change: Notify { on: false } }",
            "Setting { change: NotifyInBackground { only: false } }",
            "Setting { change: FeatureLayout { layout: Board } }",
            "Setting { change: SavedLinks { opens: Column } }",
        ]
    );
}

/// SHELL-SETTINGS-9: with notifications off, the background-only switch cannot be pressed.
#[gpui_kit::test]
fn the_background_switch_waits_on_notifications(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences { notify: false, ..Preferences::default() });
    scroll_to(cx, &opened, 2);
    click_label(cx, &opened, "Only while this window is in the background");
    assert!(asked(&opened).is_empty());
}

/// SHELL-SETTINGS-8: the editor command is sent as it is typed, and is not overwritten while the field has the keys.
#[gpui_kit::test]
fn the_editor_command_is_sent_as_typed_and_keeps_its_text_under_an_echo(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences::default());
    scroll_to(cx, &opened, 1);
    click_label(cx, &opened, "Editor command");
    type_text(cx, &opened, "ed");
    assert_eq!(asked(&opened), ["Setting { change: EditorCommand { command: \"e\" } }", "Setting { change: EditorCommand { command: \"ed\" } }"]);
    // The session's echo of the first letter lands after the second was typed.
    hear(cx, &opened, vec![page(Preferences { editor_command: "e".into(), ..Preferences::default() }, offers(None))]);
    type_text(cx, &opened, "x");
    assert_eq!(asked(&opened), ["Setting { change: EditorCommand { command: \"edx\" } }"], "the text being typed won over the echo");
}

/// SHELL-SETTINGS-1: the nav presses scroll to the section, and Escape and the chord put the sheet away.
#[gpui_kit::test]
fn escape_and_the_chord_put_the_sheet_away(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences::default());
    press(cx, &opened, "escape");
    assert!(overlays(cx).is_empty());
    blur(cx, &opened);
    press(cx, &opened, "ctrl-,");
    assert_eq!(overlays(cx), [Overlay::Settings]);
    press(cx, &opened, "ctrl-,");
    assert!(overlays(cx).is_empty(), "the chord is a toggle");
}

#[gpui_kit::test]
fn the_close_button_puts_the_sheet_away(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences::default());
    click_label(cx, &opened, "Close");
    assert!(overlays(cx).is_empty());
}

/// SHELL-SETTINGS-1: a press on a name in the nav goes to its section.
#[gpui_kit::test]
fn a_nav_press_scrolls_the_body_to_its_section(cx: &mut TestAppContext) {
    let opened = sheet(cx, Preferences::default());
    let top = |cx: &mut TestAppContext, id: usize| cx.update_window(opened.handle, |_, window, _| window.find(("settings-section", id)).bounds().origin.y.as_f32()).unwrap();
    let before = top(cx, 3);
    let nav = cx.update_window(opened.handle, |_, window, _| window.find(("settings-nav-item", 3usize)).bounds().center()).unwrap();
    use gpui_kit::test::TestEventExt as _;
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_move(nav, None, cx);
        window.pointer_down(nav, gpui_kit::MouseButton::Left, cx);
        window.pointer_up(nav, gpui_kit::MouseButton::Left, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let after = top(cx, 3);
    assert!(after < before, "Features moved up from {before} to {after}");
}

/// SHELL-SETTINGS-1: before the first `settings` event the chord does nothing, as the page's does.
#[gpui_kit::test]
fn the_chord_does_nothing_before_the_session_has_sent_the_settings(cx: &mut TestAppContext) {
    let opened = open(cx);
    blur(cx, &opened);
    press(cx, &opened, "ctrl-,");
    assert!(overlays(cx).is_empty());
}
