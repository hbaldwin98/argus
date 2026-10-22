//! The palette in a test window: opened by its key, searched by typing, moved with Down and Up,
//! and chosen with Enter or a click, as the person does it.

use argus_desktop_core::connection::page::{Event, Palette, PaletteCheckout, PaletteFeature, PalettePane};
use argus_desktop_core::keys::keymap::{commands, groups, LAYERS};
use gpui_kit::prelude::*;
use gpui_kit::TestAppContext;

use crate::keys::root::tests::{blur, click_label, drain, has_label, hear, open, overlays, press, type_text, Opened};
use crate::kit::overlay::kind::Overlay;

fn keymap() -> Event {
    Event::Keymap { groups: groups(), commands: commands(), layers: LAYERS }
}

fn pane(id: u64, title: &str) -> PalettePane {
    PalettePane { id, title: title.into(), agent: false, status: "idle", place: "argus / main".into() }
}

fn page(panes: Vec<PalettePane>) -> Palette {
    Palette { open: true, panes, features_loaded: true, ..Palette::default() }
}

/// The palette up, the session's word that the keymap is known heard, and the calls so far gone.
fn palette(cx: &mut TestAppContext) -> Opened {
    let opened = open(cx);
    hear(cx, &opened, vec![keymap()]);
    blur(cx, &opened);
    press(cx, &opened, "ctrl-k");
    assert_eq!(overlays(cx), [Overlay::Palette]);
    // The dialog slides in; what is clicked is where it has settled.
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    hear(cx, &opened, Vec::new());
    opened
}

/// SHELL-PALETTE-1: it opens from its key, tells the session it is open, and opening it again
/// changes nothing.
#[gpui_kit::test]
fn the_palette_opens_from_its_key_tells_the_session_and_opens_once(cx: &mut TestAppContext) {
    let opened = palette(cx);
    assert_eq!(drain(&opened), ["Palette { open: true }"]);
    press(cx, &opened, "ctrl-k");
    assert!(drain(&opened).is_empty(), "opening an open palette asks the session nothing");
    assert_eq!(overlays(cx), [Overlay::Palette]);
}

/// SHELL-PALETTE-4, 5: with no query a group shows its first six commands, under its heading.
#[gpui_kit::test]
fn with_no_query_the_commands_show_six_under_their_heading(cx: &mut TestAppContext) {
    let opened = palette(cx);
    assert!(has_label(cx, &opened, "Commands"));
    assert!(!has_label(cx, &opened, "Panes"), "a group with no rows has no heading");
    assert!(has_label(cx, &opened, "Go to the Workspace"));
    let shown = cx.update_window(opened.handle, |_, window, _| {
        use gpui_kit::test::TestQueryExt as _;
        window.find_all_by_role(gpui_kit::Role::ListBoxOption).len()
    });
    assert_eq!(shown.unwrap(), 6);
}

/// SHELL-PALETTE-5, 6: typing narrows the rows to those that hold the letters in order, and an
/// empty result says so.
#[gpui_kit::test]
fn typing_narrows_the_rows_and_none_left_says_no_match(cx: &mut TestAppContext) {
    let opened = palette(cx);
    type_text(cx, &opened, "console");
    assert!(has_label(cx, &opened, "Go to the Console"));
    assert!(!has_label(cx, &opened, "Go to the Workspace"));
    type_text(cx, &opened, "zzzz");
    assert!(has_label(cx, &opened, "No match."));
}

/// SHELL-PALETTE-9: Down moves the highlight and Enter runs what it is on, closing the palette and
/// telling the session first.
#[gpui_kit::test]
fn down_then_enter_runs_the_second_command_after_closing(cx: &mut TestAppContext) {
    let opened = palette(cx);
    drain(&opened);
    press(cx, &opened, "down");
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["Palette { open: false }", "Command { action: Feature }"]);
    assert!(overlays(cx).is_empty());
}

/// SHELL-PALETTE-9: Up from the first row wraps to the last shown.
#[gpui_kit::test]
fn up_from_the_first_row_wraps_to_the_last(cx: &mut TestAppContext) {
    let opened = palette(cx);
    drain(&opened);
    press(cx, &opened, "up");
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["Palette { open: false }", "Command { action: Browsers }"]);
}

/// SHELL-PALETTE-2: Escape closes it, tells the session, and runs nothing.
#[gpui_kit::test]
fn escape_closes_it_and_runs_nothing(cx: &mut TestAppContext) {
    let opened = palette(cx);
    drain(&opened);
    press(cx, &opened, "escape");
    assert_eq!(drain(&opened), ["Palette { open: false }"]);
    assert!(overlays(cx).is_empty());
}

/// SHELL-PALETTE-9: a click on a row does what Enter does to it.
#[gpui_kit::test]
fn a_click_on_a_row_runs_it(cx: &mut TestAppContext) {
    let opened = palette(cx);
    drain(&opened);
    click_label(cx, &opened, "Go to the Console");
    assert_eq!(drain(&opened), ["Palette { open: false }", "Command { action: Console }"]);
}

/// SHELL-PALETTE-5: `>` limits the search to commands and shows all of them.
#[gpui_kit::test]
fn a_leading_angle_bracket_shows_every_command_and_no_other_group(cx: &mut TestAppContext) {
    let opened = palette(cx);
    hear(cx, &opened, vec![Event::Palette(Box::new(page(vec![pane(3, "shell")])))]);
    assert!(has_label(cx, &opened, "Panes"), "unscoped, the pane's group is there");
    type_text(cx, &opened, ">");
    assert!(!has_label(cx, &opened, "Panes"));
    let shown = cx.update_window(opened.handle, |_, window, _| {
        use gpui_kit::test::TestQueryExt as _;
        window.find_all_by_role(gpui_kit::Role::ListBoxOption).len()
    });
    assert_eq!(shown.unwrap(), commands().len(), "a scope shows all of its group");
}

/// SHELL-PALETTE-11: under `#`, before the board has come, the list says it is being read.
#[gpui_kit::test]
fn under_the_hash_before_the_board_has_come_the_list_says_it_is_reading(cx: &mut TestAppContext) {
    let opened = palette(cx);
    hear(cx, &opened, vec![Event::Palette(Box::new(Palette { open: true, features_loaded: false, ..Palette::default() }))]);
    type_text(cx, &opened, "#");
    assert!(has_label(cx, &opened, "Reading the features\u{2026}"));
    let feature = PaletteFeature { slug: "trim".into(), number: 41, title: "Trim context".into(), standing: "active" };
    hear(cx, &opened, vec![Event::Palette(Box::new(Palette { open: true, features_loaded: true, features: vec![feature], ..Palette::default() }))]);
    assert!(has_label(cx, &opened, "#41 Trim context"));
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened).last().map(String::as_str), Some("GoToFeature { slug: \"trim\" }"));
}

/// SHELL-PALETTE-9: a checkout goes to it.
#[gpui_kit::test]
fn a_checkout_row_goes_to_the_checkout(cx: &mut TestAppContext) {
    let opened = palette(cx);
    let checkout = PaletteCheckout { id: 12, name: "spike".into(), repository: "argus".into(), branch: "spike".into(), git: None, clean: true };
    hear(cx, &opened, vec![Event::Palette(Box::new(Palette { open: true, checkouts: vec![checkout], features_loaded: true, ..Palette::default() }))]);
    type_text(cx, &opened, "argus / spike");
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened).last().map(String::as_str), Some("GoToCheckout { checkout: 12 }"));
}

/// SHELL-PALETTE-8: the session sends the lists again while it is open; the highlight stays on the
/// row it was on, by its key, and not on the place it was at.
#[gpui_kit::test]
fn the_highlight_stays_on_its_row_when_the_lists_change_under_it(cx: &mut TestAppContext) {
    let opened = palette(cx);
    hear(cx, &opened, vec![Event::Palette(Box::new(page(vec![pane(1, "zzp one"), pane(2, "zzp two")])))]);
    type_text(cx, &opened, "zzp");
    press(cx, &opened, "down");
    drain(&opened);
    // A better match arrives and ranks first: the old second row is now the third.
    hear(cx, &opened, vec![Event::Palette(Box::new(page(vec![pane(0, "zzp"), pane(1, "zzp one"), pane(2, "zzp two")])))]);
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["Palette { open: false }", "Select { pane: 2 }"]);
}

/// SHELL-PALETTE-8: typing puts the highlight back on the first row.
#[gpui_kit::test]
fn typing_puts_the_highlight_on_the_first_result(cx: &mut TestAppContext) {
    let opened = palette(cx);
    hear(cx, &opened, vec![Event::Palette(Box::new(page(vec![pane(1, "zzp one"), pane(2, "zzp two")])))]);
    type_text(cx, &opened, "zzp");
    press(cx, &opened, "down");
    type_text(cx, &opened, " ");
    drain(&opened);
    press(cx, &opened, "enter");
    assert_eq!(drain(&opened), ["Palette { open: false }", "Select { pane: 1 }"]);
}

/// SHELL-PALETTE-7: the row Enter would run says so.
#[gpui_kit::test]
fn the_highlighted_row_is_the_one_that_says_open(cx: &mut TestAppContext) {
    let opened = palette(cx);
    let selected = |cx: &mut TestAppContext, label: &str| {
        cx.update_window(opened.handle, |_, window, _| {
            use gpui_kit::test::TestQueryExt as _;
            window.find_by_label(label).selected()
        })
        .unwrap()
    };
    assert_eq!(selected(cx, "Go to the Workspace"), Some(true));
    assert_eq!(selected(cx, "Go to the Feature stage"), Some(false));
    press(cx, &opened, "down");
    assert_eq!(selected(cx, "Go to the Feature stage"), Some(true));
    assert_eq!(selected(cx, "Go to the Workspace"), Some(false));
}

/// SHELL-PALETTE-1: a palette opened again starts empty.
#[gpui_kit::test]
fn reopened_it_starts_empty_on_the_first_result(cx: &mut TestAppContext) {
    let opened = palette(cx);
    type_text(cx, &opened, "console");
    press(cx, &opened, "escape");
    blur(cx, &opened);
    press(cx, &opened, "ctrl-k");
    assert!(has_label(cx, &opened, "Go to the Workspace"), "the old query is gone");
}
