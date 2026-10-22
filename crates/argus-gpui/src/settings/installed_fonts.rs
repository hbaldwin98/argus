//! Which font families this machine has, for the Settings dialog's font fields to offer: every
//! name the text system lists, once, with whether the family is monospaced.
//!
//! The text system lists names and not what a face is, so a family is monospaced when its narrow
//! `i` and its wide `M` are one advance wide: the same answer the page's picker is given from the
//! font files (`argus-desktop`'s `installed_fonts`). The window's own faces are left out (they
//! are the themes', not the machine's). Reading a thousand families loads each, so it is done off
//! the UI thread, and the dialog offers none until it is done ("Reading the installed fonts").

use argus_desktop_core::connection::page::FontFamily;
use argus_desktop_core::settings::fonts::families;
use gpui_kit::{font, px, App, AppContext as _, AsyncApp};

use crate::kit::fonts::BUNDLED;

/// A narrow `i` and a wide `M` of one width: the family is monospaced.
fn monospaced(narrow: f32, wide: f32) -> bool {
    narrow > 0.0 && (narrow - wide).abs() < 0.01
}

/// The faces `names` are, as `settings::fonts::families` takes them: each name with whether it is
/// monospaced. `advances` is the width of `i` and `M` in a family at one size, `None` where the
/// family does not load.
pub fn faces(names: Vec<String>, advances: impl Fn(&str) -> Option<(f32, f32)>) -> Vec<(Vec<String>, bool)> {
    names
        .into_iter()
        .filter(|name| !BUNDLED.contains(&name.as_str()))
        .map(|name| {
            let mono = advances(&name).is_some_and(|(narrow, wide)| monospaced(narrow, wide));
            (vec![name], mono)
        })
        .collect()
}

/// Reads the installed families off the UI thread and hands them to `found` on it.
pub fn read(cx: &mut App, found: impl FnOnce(Vec<FontFamily>) + 'static) {
    let text = cx.text_system().clone();
    let reading = cx.background_spawn(async move {
        let advances = |name: &str| {
            let id = text.resolve_font(&font(name.to_owned()));
            let narrow = text.advance(id, px(16.0), 'i').ok()?.width;
            let wide = text.advance(id, px(16.0), 'M').ok()?.width;
            Some((f32::from(narrow), f32::from(wide)))
        };
        families(faces(text.all_font_names(), advances))
    });
    cx.spawn(async move |_: &mut AsyncApp| found(reading.await)).detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(name: &str) -> Option<(f32, f32)> {
        match name {
            "Cascadia Mono" => Some((9.6, 9.6)),
            "Segoe UI" => Some((4.2, 14.9)),
            "Broken" => None,
            _ => Some((8.0, 8.0)),
        }
    }

    #[test]
    fn a_family_is_monospaced_when_its_narrow_and_wide_letters_are_one_width() {
        let names = ["Cascadia Mono", "Segoe UI", "Broken"].map(String::from).to_vec();
        let got = faces(names, fake);
        assert_eq!(got, [(vec!["Cascadia Mono".to_owned()], true), (vec!["Segoe UI".to_owned()], false), (vec!["Broken".to_owned()], false)]);
    }

    #[test]
    fn the_windows_own_faces_are_not_the_machines() {
        let got = faces(vec!["Geist".into(), "JetBrains Mono".into(), "Consolas".into()], fake);
        assert_eq!(got, [(vec!["Consolas".to_owned()], true)]);
    }

    #[test]
    fn a_family_with_no_width_is_not_monospaced() {
        assert!(!monospaced(0.0, 0.0));
    }
}
