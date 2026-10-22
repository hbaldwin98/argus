//! The look the choices give the window, in the pure and in a window that hears them.

use argus_desktop_core::connection::page::{BackdropOffer, Event, FontFamily, SettingsPage};
use argus_desktop_core::settings::remembered::{Backdrop, Preferences, Theme as Chosen};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{prelude::*, px, TestAppContext, WindowBackgroundAppearance};

use super::*;
use crate::keys::root::tests::{hear, open};

fn chosen(theme: Chosen) -> Preferences {
    Preferences { theme, ..Preferences::default() }
}

/// What the person who has chosen nothing sees is the page's default: the system's theme, which is Mocha in the dark.
#[test]
fn nothing_chosen_draws_the_page_default_in_the_dark_and_latte_in_the_light() {
    let dark = Look::of(&Preferences::default(), true);
    assert_eq!(dark.tokens().name, "mocha");
    assert!(dark.dark());
    let light = Look::of(&Preferences::default(), false);
    assert_eq!(light.tokens().name, "latte");
    assert!(!light.dark());
}

#[test]
fn a_flavour_chosen_ignores_the_systems_light_or_dark() {
    for system in [true, false] {
        assert_eq!(Look::of(&chosen(Chosen::Nord), system).tokens().name, "nord");
    }
}

/// SHELL-LOOK-3, 5: the window is always Transparent for Clear, and the platform's own blur for the other two.
#[test]
fn the_backdrop_chosen_is_the_platforms_appearance_at_any_opacity() {
    for (backdrop, want) in [
        (Backdrop::Clear, WindowBackgroundAppearance::Transparent),
        (Backdrop::Mica, WindowBackgroundAppearance::MicaBackdrop),
        (Backdrop::Acrylic, WindowBackgroundAppearance::Blurred),
    ] {
        for opacity in [30, 100] {
            let look = Look::of(&Preferences { backdrop, opacity, ..Preferences::default() }, true);
            assert_eq!(look.appearance(), want, "{backdrop:?} at {opacity}");
        }
    }
}

#[test]
fn the_opacity_fades_only_the_large_fills_of_the_tokens() {
    let look = Look::of(&Preferences { opacity: 50, ..Preferences::default() }, true);
    let tokens = look.tokens();
    assert_eq!(tokens.see_sunken.a, 0.5);
    assert_eq!(tokens.bg_sunken.a, 1.0);
}

/// SHELL-KEYS-3, 4: the zoom is the rem size, a rem being `--fs-md`, 13px, at 100%.
#[test]
fn a_rem_is_the_font_size_the_scale_names() {
    assert_eq!(crate::theme::themes::ledger::tokens().fs_md, REM);
    assert_eq!(rem_of_tokens(26.0), 2.0);
}

fn rem_of_tokens(v: f32) -> f32 {
    crate::theme::scale::rem(v).0
}

#[test]
fn the_zoom_is_the_rem_size() {
    for (zoom, rem) in [(50, 6.5), (100, 13.0), (150, 19.5), (200, 26.0)] {
        assert_eq!(Look::of(&Preferences { zoom, ..Preferences::default() }, true).rem_size(), px(rem), "{zoom}%");
    }
}

/// SHELL-LOOK-4: a family chosen goes in front of the theme's own, which stays what draws the rest.
#[test]
fn a_chosen_family_leads_and_the_themes_own_stays_behind_it() {
    let look = Look::of(&Preferences { font: "Cascadia Code".into(), ui_font: "Segoe UI".into(), theme: Chosen::Nord, ..Preferences::default() }, true);
    let tokens = look.tokens();
    assert_eq!((tokens.font_mono, tokens.font_ui), ("Cascadia Code", "Segoe UI"));
    assert_eq!((tokens.font_mono_bundled, tokens.font_ui_bundled), ("Source Code Pro", "Rubik"));
    let bare = Look::of(&chosen(Chosen::Nord), true).tokens();
    assert_eq!((bare.font_mono, bare.font_ui), ("Source Code Pro", "Rubik"), "none chosen draws the theme's own");
}

#[test]
fn the_border_is_the_themes_sidebar_rule() {
    assert_eq!(Look::of(&chosen(Chosen::Nord), true).border(), 0x434c5e);
    assert_eq!(Look::of(&Preferences::default(), false).border(), 0xccd0da, "Latte's, following a light system");
}

fn page(chosen: Preferences) -> Event {
    Event::Settings(Box::new(SettingsPage {
        chosen,
        min_opacity: 30,
        backdrops: vec![BackdropOffer { backdrop: Backdrop::Clear, unavailable: None }],
        fonts: vec![FontFamily { name: "Cascadia Mono".into(), mono: true }],
        version: "0.0.0".into(),
    }))
}

fn tokens_name(cx: &mut TestAppContext) -> &'static str {
    cx.update(|cx| Tokens::global(cx).name)
}

/// SHELL-SETTINGS-6: the whole window is drawn in the theme the session says, the dialog open or not,
/// and the kit's own colours follow it.
#[gpui_kit::test]
fn the_window_follows_the_theme_the_session_sends_with_the_dialog_closed(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert_eq!(tokens_name(cx), "latte", "the test platform's system is light, and nothing is chosen");
    hear(cx, &opened, vec![page(chosen(Chosen::Nord))]);
    assert_eq!(tokens_name(cx), "nord");
    cx.update(|cx| {
        assert_eq!(cx.theme().background, gpui_kit::Hsla::transparent_black());
        assert!(cx.theme().is_dark());
    });
    hear(cx, &opened, vec![page(chosen(Chosen::Latte))]);
    assert_eq!(tokens_name(cx), "latte");
    cx.update(|cx| assert!(!cx.theme().is_dark()));
}

#[gpui_kit::test]
fn the_zoom_is_put_on_the_window_as_its_rem_size_and_taken_off_again(cx: &mut TestAppContext) {
    let opened = open(cx);
    let rem = |cx: &mut TestAppContext| cx.update_window(opened.handle, |_, window, _| window.rem_size()).unwrap();
    assert_eq!(rem(cx), px(13.0));
    hear(cx, &opened, vec![page(Preferences { zoom: 150, ..Preferences::default() })]);
    assert_eq!(rem(cx), px(19.5));
    hear(cx, &opened, vec![page(Preferences { zoom: 100, ..Preferences::default() })]);
    assert_eq!(rem(cx), px(13.0));
}

#[gpui_kit::test]
fn a_font_chosen_reaches_the_kits_theme_and_the_tokens(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![page(Preferences { font: "Cascadia Mono".into(), ..Preferences::default() })]);
    cx.update(|cx| {
        assert_eq!(cx.theme().mono_font_family, "Cascadia Mono");
        assert_eq!(Tokens::global(cx).font_mono, "Cascadia Mono");
        assert_eq!(Tokens::global(cx).font_mono_bundled, "JetBrains Mono");
    });
}

/// The same choices heard again change nothing: the theme is not installed over itself on every event.
#[gpui_kit::test]
fn the_same_choices_heard_twice_are_put_on_once(cx: &mut TestAppContext) {
    let opened = open(cx);
    let first = cx.update_window(opened.handle, |_, window, cx| {
        let Event::Settings(page) = page(chosen(Chosen::Nord)) else { unreachable!() };
        (follow(&page, window, cx), follow(&page, window, cx))
    });
    assert_eq!(first.unwrap(), (true, false));
}

/// SHELL-LOOK-6: a theme that follows the system draws the other when the system goes dark.
#[gpui_kit::test]
fn the_system_going_dark_changes_a_theme_that_follows_it_and_no_other(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert_eq!(tokens_name(cx), "latte");
    cx.update(|cx| cx.set_global(Forced(true)));
    cx.update_window(opened.handle, |_, window, cx| assert!(system_changed(window, cx))).unwrap();
    assert_eq!(tokens_name(cx), "mocha");
    hear(cx, &opened, vec![page(chosen(Chosen::Radix))]);
    cx.update(|cx| cx.set_global(Forced(false)));
    cx.update_window(opened.handle, |_, window, cx| {
        system_changed(window, cx);
    })
    .unwrap();
    assert_eq!(tokens_name(cx), "radix", "a flavour chosen is the system's no more");
}

fn width_of(cx: &mut TestAppContext, opened: &crate::keys::root::tests::Opened, id: &'static str) -> f32 {
    use gpui_kit::test::TestWindowExt as _;
    cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds().size.width.as_f32()).unwrap()
}

/// SHELL-KEYS-3, 4: the whole window follows the zoom, the sidebar as wide as the page's is at that
/// zoom (288 of its pixels, so 576 of the window's at 200%) and back at 100%.
#[gpui_kit::test]
fn the_sidebar_is_twice_as_wide_at_200_percent_and_the_same_again_at_100(cx: &mut TestAppContext) {
    // Wide enough that even at 200% the window is past the page's 1200px breakpoint, where it folds.
    let opened = crate::keys::root::tests::open_sized(cx, 3000.0);
    assert_eq!(width_of(cx, &opened, "rail"), 288.0);
    hear(cx, &opened, vec![page(Preferences { zoom: 200, ..Preferences::default() })]);
    assert_eq!(width_of(cx, &opened, "rail"), 576.0);
    hear(cx, &opened, vec![page(Preferences { zoom: 50, ..Preferences::default() })]);
    assert_eq!(width_of(cx, &opened, "rail"), 144.0);
    hear(cx, &opened, vec![page(Preferences { zoom: 100, ..Preferences::default() })]);
    assert_eq!(width_of(cx, &opened, "rail"), 288.0);
}

/// A window zoomed in is narrower in the page's pixels: past its 1200px breakpoint it folds the sidebar to icons.
#[gpui_kit::test]
fn zooming_in_folds_the_sidebar_when_the_window_is_narrow_in_the_pages_pixels(cx: &mut TestAppContext) {
    let opened = open(cx);
    assert_eq!(width_of(cx, &opened, "rail"), 288.0, "1400 wide at 100%");
    hear(cx, &opened, vec![page(Preferences { zoom: 150, ..Preferences::default() })]);
    assert_eq!(width_of(cx, &opened, "rail"), 56.0 * 1.5, "933px of the page's, so folded");
}
