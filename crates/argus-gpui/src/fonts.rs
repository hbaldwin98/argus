//! Which typefaces the window ships and how they reach GPUI's text system.
//!
//! The page bundles its faces so every machine draws the same window
//! (`argus-desktop/assets/fonts`); GPUI's `add_fonts` wants TTF or OTF, so the
//! same families sit here converted from those woff2 files with their licences
//! beside them. Which family a theme names for the interface and the code is
//! its token (`font_ui`, `font_mono`); Ledger's are Inter and JetBrains
//! Mono, with the Nerd Font symbols behind it for the private-use icons the
//! mono face lacks, as in the page's `--font-mono-bundled`. That they load
//! and resolve is `tests/fonts.rs`: GPUI's test platform has no text system.

use std::borrow::Cow;

use gpui_kit::App;

/// Icon glyphs in the private-use area, drawn only where [`MONO`] has none.
pub const SYMBOLS: &str = "Symbols Nerd Font Mono";

/// Every shipped face. Inter is one variable file that covers each weight, so
/// the page's 510 and 590 draw as themselves; the latin subset, as the page's
/// first `unicode-range` file, because a second file of the same family would
/// not be consulted for the glyphs the first lacks.
static FACES: [&[u8]; 6] = [
    include_bytes!("../assets/fonts/Inter-Variable.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
    include_bytes!("../assets/fonts/SymbolsNerdFontMono-Regular.ttf"),
];

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
    fn the_page_faces_weigh_what_the_notes_say() {
        // Recorded in the task report; a face dropped or swapped moves this.
        assert!(bundled_bytes() > 3_000_000 && bundled_bytes() < 4_500_000);
    }
}
