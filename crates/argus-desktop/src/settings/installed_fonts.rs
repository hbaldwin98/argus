//! The font families this machine has installed, read from its font files:
//! every name the webview might know each by, and which are monospaced.
//! Which of them the Settings dialog offers is `argus_desktop_core::settings::fonts`.

use ttf_parser::name_id::{FAMILY, TYPOGRAPHIC_FAMILY};
use ttf_parser::Language;

use argus_desktop_core::connection::page::FontFamily;
use argus_desktop_core::settings::fonts::families;

/// Read from every font file on the machine, so slow enough to be done
/// once, off the window's thread.
pub fn installed() -> Vec<FontFamily> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let mut faces = Vec::new();
    for face in db.faces() {
        db.with_face_data(face.id, |data, index| {
            if let Ok(parsed) = ttf_parser::Face::parse(data, index) {
                faces.push((names(&parsed), parsed.is_monospaced()));
            }
        });
    }
    families(faces)
}

/// A face's family as both its legacy and its typographic name: Windows
/// knows a Nerd Font as `JetBrainsMono NFM`, fontconfig as `JetBrainsMono
/// Nerd Font Mono`, and the page keeps whichever its webview draws.
fn names(face: &ttf_parser::Face) -> Vec<String> {
    // Only a Unicode record decodes; a Mac Roman copy of the same name is
    // skipped rather than chosen and lost.
    let english = |id: u16| {
        let named = || {
            face.names()
                .into_iter()
                .filter(move |n| n.name_id == id)
                .filter_map(|n| Some((n.language(), n.to_string()?)))
        };
        named()
            .find(|(language, _)| *language == Language::English_UnitedStates)
            .or_else(|| named().next())
            .map(|(_, name)| name)
    };
    [FAMILY, TYPOGRAPHIC_FAMILY].into_iter().filter_map(english).collect()
}
