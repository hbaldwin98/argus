//! Which glyphs the window draws that the kit's Lucide set lacks: the page's
//! own icon set (`page/src/kit/dom.ts`, `assets/index.html`), as SVG files.
//!
//! The page's glyphs are 1.5px strokes on a 16 grid and Lucide's are 2px on a
//! 24 one, so a stand-in beside the page's reads as a different hand; these
//! are the same paths. They are handed to the kit's `Icon` as bytes, so no
//! asset source has to know them.

use gpui_kit::component::Icon;
use gpui_kit::px;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Logo,
    Search,
    Plus,
    Workspace,
    Features,
    Branch,
    Commit,
    Code,
    Console,
    Down,
    Right,
    Close,
    /// Three dots: a menu's button.
    More,
    DockLeft,
    DockRight,
    DockBottom,
    Bell,
    BellOff,
    /// A spinner's three quarters of a ring; the caller turns it.
    Arc,
    /// A hollow dot: a ring of 1.5px inside its box, in the 9, 8 and 6px boxes the page draws.
    Ring9,
    Ring8,
    Ring6,
    WinMin,
    WinMax,
    WinRestore,
    WinClose,
    Refresh,
    FoldAll,
}

impl Glyph {
    fn bytes(self) -> &'static [u8] {
        match self {
            Glyph::Logo => include_bytes!("../../assets/icons/logo.svg"),
            Glyph::Search => include_bytes!("../../assets/icons/search.svg"),
            Glyph::Plus => include_bytes!("../../assets/icons/plus.svg"),
            Glyph::Workspace => include_bytes!("../../assets/icons/workspace.svg"),
            Glyph::Features => include_bytes!("../../assets/icons/features.svg"),
            Glyph::Branch => include_bytes!("../../assets/icons/branch.svg"),
            Glyph::Commit => include_bytes!("../../assets/icons/commit.svg"),
            Glyph::Code => include_bytes!("../../assets/icons/code.svg"),
            Glyph::Console => include_bytes!("../../assets/icons/console.svg"),
            Glyph::Down => include_bytes!("../../assets/icons/down.svg"),
            Glyph::Right => include_bytes!("../../assets/icons/right.svg"),
            Glyph::Close => include_bytes!("../../assets/icons/close.svg"),
            Glyph::More => include_bytes!("../../assets/icons/more.svg"),
            Glyph::DockLeft => include_bytes!("../../assets/icons/dock-left.svg"),
            Glyph::DockRight => include_bytes!("../../assets/icons/dock-right.svg"),
            Glyph::DockBottom => include_bytes!("../../assets/icons/dock-bottom.svg"),
            Glyph::Bell => include_bytes!("../../assets/icons/bell.svg"),
            Glyph::BellOff => include_bytes!("../../assets/icons/bell-off.svg"),
            Glyph::Arc => include_bytes!("../../assets/icons/arc.svg"),
            Glyph::Ring9 => include_bytes!("../../assets/icons/ring9.svg"),
            Glyph::Ring8 => include_bytes!("../../assets/icons/ring8.svg"),
            Glyph::Ring6 => include_bytes!("../../assets/icons/ring6.svg"),
            Glyph::WinMin => include_bytes!("../../assets/icons/win-min.svg"),
            Glyph::WinMax => include_bytes!("../../assets/icons/win-max.svg"),
            Glyph::WinRestore => include_bytes!("../../assets/icons/win-restore.svg"),
            Glyph::WinClose => include_bytes!("../../assets/icons/win-close.svg"),
            Glyph::Refresh => include_bytes!("../../assets/icons/refresh.svg"),
            Glyph::FoldAll => include_bytes!("../../assets/icons/fold-all.svg"),
        }
    }

    /// The glyph at `size` CSS pixels, in the text colour it is put under.
    pub fn at(self, size: f32) -> Icon {
        use gpui_kit::prelude::*;
        Icon::default().data(self.bytes()).size(px(size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_is_an_svg_with_a_stroke_or_a_fill_to_draw() {
        for glyph in [
            Glyph::Logo,
            Glyph::Search,
            Glyph::Plus,
            Glyph::Workspace,
            Glyph::Features,
            Glyph::Branch,
            Glyph::Commit,
            Glyph::Code,
            Glyph::Console,
            Glyph::Down,
            Glyph::Right,
            Glyph::Close,
            Glyph::More,
            Glyph::DockLeft,
            Glyph::DockRight,
            Glyph::DockBottom,
            Glyph::Bell,
            Glyph::BellOff,
            Glyph::Arc,
            Glyph::Ring9,
            Glyph::Ring8,
            Glyph::Ring6,
            Glyph::WinMin,
            Glyph::WinMax,
            Glyph::WinRestore,
            Glyph::WinClose,
            Glyph::Refresh,
            Glyph::FoldAll,
        ] {
            let text = std::str::from_utf8(glyph.bytes()).expect("utf-8");
            assert!(text.starts_with("<svg"), "{glyph:?}");
            assert!(text.contains("currentColor"), "{glyph:?} would draw in a colour of its own");
        }
    }
}
