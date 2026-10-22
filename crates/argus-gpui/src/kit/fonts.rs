//! Which typefaces the window ships and how they reach GPUI's text system.
//!
//! The page bundles its faces so every machine draws the same window
//! (`argus-desktop/assets/fonts`); GPUI's `add_fonts` wants TTF or OTF, so the
//! same families sit here converted from those woff2 files with their licences
//! beside them (decision #68: each theme's own faces ship with the theme). Which
//! family a theme names for the interface and the code is its token (`font_ui`,
//! `font_mono`): Geist and JetBrains Mono for the Catppuccin flavours and Night,
//! and the family each other theme brings, with the Nerd Font symbols behind the
//! code face for the private-use icons the mono face lacks, as in the page's
//! `--font-mono-bundled`. Only each family's latin file is converted: a second
//! file of one family would not be consulted for the glyphs the first lacks. That
//! they load and resolve is `tests/fonts.rs`: GPUI's test platform has no text system.

use std::borrow::Cow;

use std::sync::Mutex;

use gpui_kit::{App, FontFallbacks};

/// Icon glyphs in the private-use area, drawn only where [`MONO`] has none.
pub const SYMBOLS: &str = "Symbols Nerd Font Mono";

/// Every shipped face. The variable files (Inter, Geist and the other interface families,
/// Source Code Pro) cover each weight, so the page's 510 and 590 draw as themselves; a family
/// whose weights are static files (JetBrains Mono, IBM Plex Mono, DM Mono) ships the ones the
/// terminal draws.
static FACES: [&[u8]; 22] = [
    include_bytes!("../../assets/fonts/Geist-Variable.ttf"),
    include_bytes!("../../assets/fonts/Inter-Variable.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexSans-wght.ttf"),
    include_bytes!("../../assets/fonts/InstrumentSans-wght.ttf"),
    include_bytes!("../../assets/fonts/NotoSans-wght.ttf"),
    include_bytes!("../../assets/fonts/DMSans-wght.ttf"),
    include_bytes!("../../assets/fonts/Rubik-wght.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexMono-400.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexMono-400-italic.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexMono-700.ttf"),
    include_bytes!("../../assets/fonts/IBMPlexMono-700-italic.ttf"),
    include_bytes!("../../assets/fonts/DMMono-400.ttf"),
    include_bytes!("../../assets/fonts/DMMono-400-italic.ttf"),
    include_bytes!("../../assets/fonts/DMMono-500.ttf"),
    include_bytes!("../../assets/fonts/DMMono-500-italic.ttf"),
    include_bytes!("../../assets/fonts/SourceCodePro-wght.ttf"),
    include_bytes!("../../assets/fonts/SourceCodePro-wght-italic.ttf"),
    include_bytes!("../../assets/fonts/SymbolsNerdFontMono-Regular.ttf"),
];

/// The family of each of [`FACES`], by the name a theme gives it (and the font picker leaves out:
/// they are the window's, not the machine's).
pub const BUNDLED: [&str; 12] = [
    "Geist", "Inter", "IBM Plex Sans", "Instrument Sans", "Noto Sans", "DM Sans", "Rubik", "JetBrains Mono", "IBM Plex Mono", "DM Mono",
    "Source Code Pro", SYMBOLS,
];

/// The family in the token space for a theme's face and the person's choice, which goes in front
/// of it: the token space holds `&'static str`, so a chosen name is kept once per distinct name
/// for the life of the process, and a person chooses few.
pub fn ahead(chosen: &str, bundled: &'static str) -> &'static str {
    static KEPT: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    if chosen.is_empty() {
        return bundled;
    }
    let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(name) = kept.iter().find(|name| **name == chosen) {
        return name;
    }
    let name: &'static str = Box::leak(chosen.to_owned().into_boxed_str());
    kept.push(name);
    name
}

/// The faces that draw what the code face a person chose lacks: the theme's own, then the Nerd
/// Font symbols (the page's `"<chosen>", var(--font-mono-bundled)`).
pub fn mono_behind(bundled: &str) -> FontFallbacks {
    FontFallbacks::from_fonts(vec![bundled.to_owned(), SYMBOLS.to_owned()])
}

/// What the faces add to the binary, in bytes.
pub fn bundled_bytes() -> usize {
    FACES.iter().map(|face| face.len()).sum()
}

/// Registers the faces. A failure is logged and the theme falls back to a
/// system family, which is worse than the page but never a missing window.
pub fn install(cx: &App) {
    let faces = FACES.iter().map(|face| Cow::Borrowed(*face)).collect();
    if let Err(error) = cx.text_system().add_fonts(faces) {
        tracing::warn!(%error, "the bundled fonts did not load");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_family_goes_in_front_and_an_empty_one_leaves_the_themes_own() {
        assert_eq!(ahead("", "Geist"), "Geist");
        let chosen = ahead("Segoe UI", "Geist");
        assert_eq!(chosen, "Segoe UI");
        assert!(std::ptr::eq(chosen, ahead("Segoe UI", "Inter")), "one name is kept once");
    }

    #[test]
    fn the_symbols_come_last_behind_the_themes_own_code_face() {
        assert_eq!(mono_behind("IBM Plex Mono").fallback_list(), ["IBM Plex Mono", SYMBOLS]);
    }

    #[test]
    fn the_page_faces_weigh_what_the_notes_say() {
        // Recorded in the task report; a face dropped or swapped moves this.
        assert!(bundled_bytes() > 4_000_000 && bundled_bytes() < 5_500_000);
    }
}
