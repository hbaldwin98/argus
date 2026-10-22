//! Which glyphs the window draws that the kit's Lucide set lacks: the page's
//! own icon set (`page/src/kit/dom.ts`, `assets/index.html`), as SVG files.
//!
//! The page's glyphs are 1.5px strokes on a 16 grid and Lucide's are 2px on a
//! 24 one, so a stand-in beside the page's reads as a different hand; these
//! are the same paths. They are handed to the kit's `Icon` as bytes, so no
//! asset source has to know them.

use gpui_kit::component::Icon;
use crate::theme::scale::rem;

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
    /// A circle with an i: a notice.
    Info,
    /// The settings' gear.
    Gear,
    /// The empty stages' marks, drawn at the tile's icon size: a folder, a terminal, and the turning ring of a wait.
    Folder,
    Terminal,
    Spinner,
    /// Two stacked units with a light each: the daemon.
    Server,
    /// Two heads: how many agents share a checkout.
    People,
    /// A clock face: recent sessions.
    Clock,
    /// The marks an extension may name for its view (`ExtTab::icon`), and the puzzle piece it gets when it names none.
    Box,
    Database,
    Globe,
    List,
    Rocket,
    Puzzle,
    /// An arrow out of a box: open as a page.
    External,
    /// The palette's mark for the shortcuts, an arrow up for the folder list, the warning triangle and the bin.
    Keyboard,
    Up,
    Warn,
    Trash,
    /// A tick (a chosen card, a saved dialog) and the two panes of a split (the Views settings).
    Check,
    Split,
    /// The marks of a callout, a block of an agent's view in a tone: the page's own paths, which the shared set has no match for.
    NoteWarn,
    NoteBad,
    NoteGood,
    NoteInfo,
    /// A checklist's tick, heavier than `Check`.
    Tick,
    /// The corner button that opens a drawing larger.
    Expand,
    /// The file tree's marks at the page's small size, a folder and a file; the large file is an
    /// empty stage's tile; and the help circle.
    FolderSm,
    FileSm,
    File,
    Help,
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
            Glyph::Info => include_bytes!("../../assets/icons/info.svg"),
            Glyph::Gear => include_bytes!("../../assets/icons/gear.svg"),
            Glyph::Folder => include_bytes!("../../assets/icons/folder.svg"),
            Glyph::Terminal => include_bytes!("../../assets/icons/terminal.svg"),
            Glyph::Spinner => include_bytes!("../../assets/icons/spinner.svg"),
            Glyph::Server => include_bytes!("../../assets/icons/server.svg"),
            Glyph::People => include_bytes!("../../assets/icons/people.svg"),
            Glyph::Clock => include_bytes!("../../assets/icons/clock.svg"),
            Glyph::Box => include_bytes!("../../assets/icons/box.svg"),
            Glyph::Database => include_bytes!("../../assets/icons/database.svg"),
            Glyph::Globe => include_bytes!("../../assets/icons/globe.svg"),
            Glyph::List => include_bytes!("../../assets/icons/list.svg"),
            Glyph::Rocket => include_bytes!("../../assets/icons/rocket.svg"),
            Glyph::Puzzle => include_bytes!("../../assets/icons/puzzle.svg"),
            Glyph::External => include_bytes!("../../assets/icons/external.svg"),
            Glyph::Keyboard => include_bytes!("../../assets/icons/keyboard.svg"),
            Glyph::Up => include_bytes!("../../assets/icons/up.svg"),
            Glyph::Warn => include_bytes!("../../assets/icons/warn.svg"),
            Glyph::Trash => include_bytes!("../../assets/icons/trash.svg"),
            Glyph::Check => include_bytes!("../../assets/icons/check.svg"),
            Glyph::Split => include_bytes!("../../assets/icons/split.svg"),
            Glyph::NoteWarn => include_bytes!("../../assets/icons/note-warn.svg"),
            Glyph::NoteBad => include_bytes!("../../assets/icons/note-bad.svg"),
            Glyph::NoteGood => include_bytes!("../../assets/icons/note-good.svg"),
            Glyph::NoteInfo => include_bytes!("../../assets/icons/note-info.svg"),
            Glyph::Tick => include_bytes!("../../assets/icons/tick.svg"),
            Glyph::Expand => include_bytes!("../../assets/icons/expand.svg"),
            Glyph::FolderSm => include_bytes!("../../assets/icons/folder-sm.svg"),
            Glyph::FileSm => include_bytes!("../../assets/icons/file-sm.svg"),
            Glyph::File => include_bytes!("../../assets/icons/file.svg"),
            Glyph::Help => include_bytes!("../../assets/icons/help.svg"),
        }
    }

    /// The glyph at `size` CSS pixels, in the text colour it is put under.
    pub fn at(self, size: f32) -> Icon {
        use gpui_kit::prelude::*;
        Icon::default().data(self.bytes()).size(rem(size))
    }

    /// The mark an extension's view names (`ExtTab::icon`); a name the window does not draw, and an
    /// extension that names none, get the puzzle piece, since an extension never sends an image.
    pub fn of_extension(name: Option<&str>) -> Glyph {
        match name {
            Some("box") => Glyph::Box,
            Some("server") => Glyph::Server,
            Some("database") => Glyph::Database,
            Some("globe") => Glyph::Globe,
            Some("list") => Glyph::List,
            Some("rocket") => Glyph::Rocket,
            Some("terminal") => Glyph::Console,
            Some("gear") => Glyph::Gear,
            _ => Glyph::Puzzle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_extension_names_one_of_eight_marks_and_gets_the_puzzle_piece_for_any_other() {
        for (name, glyph) in [
            ("box", Glyph::Box),
            ("server", Glyph::Server),
            ("database", Glyph::Database),
            ("globe", Glyph::Globe),
            ("list", Glyph::List),
            ("rocket", Glyph::Rocket),
            ("terminal", Glyph::Console),
            ("gear", Glyph::Gear),
        ] {
            assert_eq!(Glyph::of_extension(Some(name)), glyph, "{name}");
        }
        // `constructor` is a name the protocol lets through; inherited it is no icon at all.
        for other in [Some("constructor"), Some(""), Some("Box"), None] {
            assert_eq!(Glyph::of_extension(other), Glyph::Puzzle, "{other:?}");
        }
    }


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
            Glyph::External,
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
            Glyph::Info,
            Glyph::Gear,
            Glyph::Folder,
            Glyph::Terminal,
            Glyph::Spinner,
            Glyph::Server,
            Glyph::People,
            Glyph::Clock,
            Glyph::Box,
            Glyph::Database,
            Glyph::Globe,
            Glyph::List,
            Glyph::Rocket,
            Glyph::Puzzle,
            Glyph::Keyboard,
            Glyph::Up,
            Glyph::Warn,
            Glyph::Trash,
            Glyph::Check,
            Glyph::Split,
            Glyph::NoteWarn,
            Glyph::NoteBad,
            Glyph::NoteGood,
            Glyph::NoteInfo,
            Glyph::Tick,
            Glyph::Expand,
        ] {
            let text = std::str::from_utf8(glyph.bytes()).expect("utf-8");
            assert!(text.starts_with("<svg"), "{glyph:?}");
            assert!(text.contains("currentColor"), "{glyph:?} would draw in a colour of its own");
        }
    }
}
