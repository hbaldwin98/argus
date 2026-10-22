//! Every theme's data held to the stylesheet it was written from, and the table from a choice to it.

use argus_desktop_core::settings::flavours::FLAVOURS;
use argus_desktop_core::settings::remembered::Theme as Chosen;

use super::*;
use crate::theme::colour::Colour;
use crate::theme::define::Mode;
use crate::theme::scale::Mark;
use crate::theme::stylesheet::{blocks, bytes, colour_of, decl, hex_bytes, layers_in, root_block, type_in, value_in};

fn serde_name(theme: Chosen) -> String {
    serde_json::to_value(theme).unwrap().as_str().unwrap().to_owned()
}

fn css(theme: &str, name: &str) -> String {
    value_in(theme, name).unwrap_or_else(|| panic!("{theme}: --{name} is set nowhere"))
}

fn length(theme: &str, name: &str) -> f32 {
    let v = css(theme, name);
    v.strip_suffix("px").unwrap_or(&v).parse().unwrap_or_else(|_| panic!("{theme}: --{name} is `{v}`"))
}

fn palette_colour(theme: &str, name: &str) -> Colour {
    let block = blocks(theme).into_iter().rev().find(|block| decl(block, name).is_some()).unwrap_or_else(|| panic!("{theme}: --{name}"));
    colour_of(block, name)
}

#[test]
fn every_choice_but_system_draws_its_own_theme_under_the_name_the_file_keeps() {
    for (chosen, _) in FLAVOURS {
        assert_eq!(theme(chosen, true).name, serde_name(chosen), "{chosen:?}");
        assert_eq!(theme(chosen, false).name, serde_name(chosen), "a flavour chosen ignores the system");
    }
}

/// SHELL-SETTINGS-5: following the system is Mocha when it is dark and Latte when it is light.
#[test]
fn the_system_draws_mocha_in_the_dark_and_latte_in_the_light() {
    assert_eq!(theme(Chosen::System, true).name, "mocha");
    assert_eq!(theme(Chosen::System, false).name, "latte");
}

/// The page's default theme is the system's; its screenshots are Mocha, so Mocha is the default here.
#[test]
fn the_default_choice_is_the_page_default_in_the_dark() {
    assert_eq!(theme(Chosen::default(), true).name, "mocha");
}

#[test]
fn only_latte_is_light() {
    for (chosen, _) in FLAVOURS {
        let light = blocks(&serde_name(chosen)).iter().any(|block| block.contains("color-scheme: light"));
        assert_eq!(theme(chosen, true).mode == Mode::Light, light, "{chosen:?}");
        assert_eq!(tokens(chosen, true, 100).dark, !light, "{chosen:?}");
    }
}

#[test]
fn each_themes_palette_and_ansi_sixteen_are_the_stylesheets() {
    for (chosen, _) in FLAVOURS {
        let (name, t) = (serde_name(chosen), tokens(chosen, true, 100));
        for (key, ours) in [
            ("crust", t.crust), ("mantle", t.mantle), ("base", t.base), ("surface0", t.surface0), ("surface1", t.surface1),
            ("overlay", t.overlay), ("subtext", t.subtext), ("text", t.text), ("mauve", t.mauve), ("blue", t.blue),
            ("green", t.green), ("yellow", t.yellow), ("red", t.red), ("peach", t.peach), ("teal", t.teal),
            ("lilac", t.lilac), ("fg-on-accent", t.fg_on_accent),
        ] {
            assert_eq!(bytes(ours), hex_bytes(palette_colour(&name, key)), "{name} --{key}");
        }
        for (slot, ours) in t.ansi.into_iter().enumerate() {
            assert_eq!(bytes(ours), hex_bytes(palette_colour(&name, &format!("ansi-{slot}"))), "{name} --ansi-{slot}");
        }
    }
}

#[test]
fn each_themes_shape_is_the_stylesheets() {
    for (chosen, _) in FLAVOURS {
        let (name, t) = (serde_name(chosen), tokens(chosen, true, 100));
        for (key, ours) in [
            ("r-xs", t.r_xs), ("r-sm", t.r_sm), ("r-md", t.r_md), ("r-lg", t.r_lg), ("r-pill", t.r_pill),
            ("control-h", t.control_h), ("control-h-sm", t.control_h_sm), ("icon-btn", t.icon_btn),
        ] {
            assert_eq!(ours, length(&name, key), "{name} --{key}");
        }
        for (key, ours) in [("type-display", t.type_display), ("type-title", t.type_title), ("type-heading", t.type_heading), ("type-label", t.type_label)] {
            let want = type_in(&css(&name, key));
            assert_eq!((ours.weight, ours.size), (want.weight, want.size), "{name} --{key}");
            assert!((ours.leading - want.leading).abs() < 1e-6, "{name} --{key} leading");
        }
        assert_eq!(t.shadow_pop, layers_in(&css(&name, "shadow-pop"), "shadow-pop"), "{name} --shadow-pop");
        assert_eq!(t.shadow_dialog, layers_in(&css(&name, "shadow-dialog"), "shadow-dialog"), "{name} --shadow-dialog");
    }
}

#[test]
fn each_themes_selection_mark_is_the_stylesheets() {
    for (chosen, _) in FLAVOURS {
        let (name, t) = (serde_name(chosen), tokens(chosen, true, 100));
        let mark = css(&name, "mark-sel");
        let want = if mark == "none" {
            Mark::None
        } else if mark.starts_with("inset 0 0 0 1px") {
            Mark::Hairline
        } else {
            assert!(mark.starts_with("inset 3px 0 0"), "{name}: {mark}");
            Mark::Bar
        };
        assert_eq!(t.mark_sel, want, "{name}");
        // A bar is lit by the ring; every other mark takes the hairline's colour.
        let edge = css(&name, "mark-sel-edge");
        assert_eq!(edge, if want == Mark::Bar { "var(--ring)" } else { "var(--line-strong)" }, "{name}");
        assert_eq!(t.mark_sel_edge, if want == Mark::Bar { t.ring } else { t.line_strong }, "{name}");
    }
}

/// SHELL-LOOK-4: each theme brings its own interface and code faces.
#[test]
fn each_themes_own_faces_lead_its_stacks() {
    for (chosen, _) in FLAVOURS {
        let (name, t) = (serde_name(chosen), tokens(chosen, true, 100));
        let first = |key: &str| css(&name, key).split(',').next().unwrap().trim().trim_matches('\'').to_owned();
        assert_eq!(t.font_ui, first("font-ui-bundled"), "{name} interface");
        assert_eq!(t.font_mono, first("font-mono-bundled"), "{name} code");
    }
}

#[test]
fn what_a_theme_pins_instead_of_deriving_is_the_stylesheets() {
    for (chosen, _) in FLAVOURS {
        let (name, t) = (serde_name(chosen), tokens(chosen, true, 100));
        let pinned = |key: &str| value_in(&name, key).filter(|v| !v.starts_with("var(") && !v.starts_with("color-mix"));
        let solid = |key: &str| pinned(key).filter(|v| v.starts_with('#')).map(|v| u32::from_str_radix(&v[1..], 16).unwrap());
        for (key, ours) in [("fill", t.fill), ("line-strong", t.line_strong), ("ring", t.ring), ("scrim", t.scrim)] {
            if let Some(v) = value_in(&name, key).filter(|v| v.starts_with('#')) {
                assert_eq!(bytes(ours), hex_bytes(Colour::hex(u32::from_str_radix(&v[1..], 16).unwrap())), "{name} --{key}");
            }
        }
        if let Some(rgb) = solid("bg-raised") {
            assert_eq!(bytes(t.bg_raised), hex_bytes(Colour::hex(rgb)), "{name} --bg-raised");
        }
    }
    // Argus Night's lines are the accent at low alpha, so every border is tinted alike.
    let night = tokens(Chosen::Night, true, 100);
    assert_eq!(css("night", "line"), "rgba(152, 192, 239, 0.12)");
    assert!((night.line.a - 0.12).abs() < 1e-6 && (night.line_strong.a - 0.24).abs() < 1e-6);
    assert_eq!(bytes(night.line), [152, 192, 239]);
    assert_eq!(css("night", "scrim"), "rgba(4, 5, 10, 0.64)");
    assert_eq!(bytes(night.scrim), [4, 5, 10]);
}

/// Latte's raised surface is lighter than its stage: `color-mix(in srgb, var(--bg-surface) 40%, white)`.
#[test]
fn lattes_raised_surface_is_the_surface_mixed_toward_white() {
    assert_eq!(css("latte", "bg-raised"), "color-mix(in srgb, var(--bg-surface) 40%, white)");
    let base = hex_bytes(palette_colour("latte", "base"));
    let want = [0, 1, 2].map(|i| (f32::from(base[i]) * 0.4 + 255.0 * 0.6).round() as u8);
    assert_eq!(bytes(tokens(Chosen::Latte, true, 100).bg_raised), want);
}

/// The roles a theme leaves to `:root` are aliases of the palette.
#[test]
fn the_roles_every_theme_leaves_to_the_stylesheet_are_its_palette() {
    for (chosen, _) in FLAVOURS {
        let (name, t) = (serde_name(chosen), tokens(chosen, true, 100));
        for (role, palette, ours) in [
            ("bg-sunken", "crust", t.bg_sunken), ("bg-chrome", "mantle", t.bg_chrome), ("bg-surface", "base", t.bg_surface),
            ("bg-selected", "lilac", t.bg_selected), ("fg", "text", t.fg), ("fg-muted", "subtext", t.fg_muted),
            ("fg-faint", "overlay", t.fg_faint), ("accent", "mauve", t.accent), ("ref", "blue", t.ref_), ("ok", "green", t.ok),
            ("attention", "yellow", t.attention), ("danger", "red", t.danger), ("changed", "peach", t.changed), ("info", "teal", t.info),
        ] {
            // `:root`'s aliases are in the first block of the stylesheet, the one Mocha shares.
            let alias = value_in(&name, role).or_else(|| decl(blocks("mocha")[0], role)).unwrap();
            assert_eq!(alias, format!("var(--{palette})"), "{name} --{role}");
            assert_eq!(bytes(ours), hex_bytes(palette_colour(&name, palette)), "{name} --{role}");
        }
    }
    assert!(root_block().contains("--font-ui-bundled"));
}

#[test]
fn the_opacity_rides_on_the_large_fills_of_the_theme_chosen() {
    let t = tokens(Chosen::Mocha, true, 60);
    assert!((t.see_sunken.a - 0.6).abs() < 1e-6 && t.bg_sunken.a == 1.0);
}
