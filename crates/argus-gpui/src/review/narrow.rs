//! Whether the Changes tab is too narrow for its files beside the diff, as it is in a side dock or
//! a small dialog, and which of the two it shows then: the files, until one is opened, and then
//! that file's diff, until its back button.
//!
//! The width is the tab's own, measured as it was last laid out, not the window's: the same tab
//! is the whole stage in one place and a 480px dock in another.

use std::cell::Cell;
use std::rc::Rc;

use argus_desktop_core::connection::page::ReviewPage;
use gpui_kit::{canvas, prelude::*, Bounds, Pixels};

use crate::theme::scale::css;

/// Below this many CSS pixels the files and the diff take turns.
pub const NARROW: f32 = 760.0;

#[derive(Default)]
pub struct Narrow {
    /// Where the tab was last laid out.
    pub area: Rc<Cell<Bounds<Pixels>>>,
    /// The files are on screen rather than the diff.
    listing: bool,
    /// The file on screen at the last page, by its side and path.
    last: Option<(bool, String)>,
}

impl Narrow {
    pub fn new() -> Narrow {
        Narrow { listing: true, ..Narrow::default() }
    }

    /// Whether the tab's last width, in CSS pixels at `rem`, is narrow. Before it was ever laid
    /// out it is not: the first frame is drawn wide, as the stage has always been.
    pub fn narrow(&self, rem: Pixels) -> bool {
        let width = self.area.get().size.width;
        width > Pixels::ZERO && css(width, rem) < NARROW
    }

    /// Records the tab's area as it is laid out, and draws the window again when that crosses
    /// `NARROW`, so the frame after a dock's sash is let go is drawn the new way.
    pub fn measure(&self) -> impl IntoElement {
        let area = self.area.clone();
        canvas(
            move |bounds, window, _| {
                let rem = window.rem_size();
                let was = area.replace(bounds).size.width;
                if (css(was, rem) < NARROW) != (css(bounds.size.width, rem) < NARROW) {
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    /// Follows the page: a file opened, by a click, a key or the jump field, puts its diff on
    /// screen; with no file open the files are.
    pub fn follow(&mut self, page: &ReviewPage) {
        let staged = page.staging.as_ref().is_some_and(|staging| staging.staged);
        let now = page.file.and_then(|i| page.files.get(i)).map(|file| (staged, file.path.clone()));
        if now.is_none() {
            self.listing = true;
        } else if now != self.last {
            self.listing = false;
        }
        self.last = now;
    }

    /// The diff's back button: the files again, the file still open behind them.
    pub fn back(&mut self) {
        self.listing = true;
    }

    /// Whether the files are on screen, once narrow.
    pub fn listing(&self) -> bool {
        self.listing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::connection::page::ReviewFile;

    fn on(path: Option<&str>) -> ReviewPage {
        let file = |path: &str| ReviewFile { path: path.into(), dir: String::new(), name: path.into(), added: 1, removed: 0, kind: "modified" };
        let files = vec![file("a.rs"), file("b.rs")];
        let file = path.and_then(|path| files.iter().position(|f| f.path == path));
        ReviewPage { files, file, ..ReviewPage::default() }
    }

    #[test]
    fn a_file_opened_shows_its_diff_and_back_shows_the_files_with_it_still_open() {
        let mut narrow = Narrow::new();
        narrow.follow(&on(None));
        assert!(narrow.listing());
        narrow.follow(&on(Some("a.rs")));
        assert!(!narrow.listing(), "opened");
        narrow.back();
        narrow.follow(&on(Some("a.rs")));
        assert!(narrow.listing(), "the same file heard again does not leave the list");
        narrow.follow(&on(Some("b.rs")));
        assert!(!narrow.listing(), "another file opened");
        narrow.follow(&on(None));
        assert!(narrow.listing(), "nothing open, nothing to show but the files");
    }

    #[test]
    fn it_is_narrow_by_its_own_width_and_wide_before_it_was_laid_out() {
        let narrow = Narrow::new();
        let rem = gpui_kit::px(16.0);
        assert!(!narrow.narrow(rem));
        narrow.area.set(Bounds { origin: Default::default(), size: gpui_kit::size(gpui_kit::px(480.0), gpui_kit::px(900.0)) });
        assert!(narrow.narrow(rem));
        narrow.area.set(Bounds { origin: Default::default(), size: gpui_kit::size(gpui_kit::px(1200.0), gpui_kit::px(900.0)) });
        assert!(!narrow.narrow(rem));
    }
}
