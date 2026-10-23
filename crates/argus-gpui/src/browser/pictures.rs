//! The latest decoded picture of each browser, held once for every place that draws it.
//!
//! A tile on the Browsers stage and a browser's own cell show the same picture, so it is decoded
//! once and shared as one `Arc<RenderImage>`. The newest picture wins: each one heard takes a
//! ticket, and a decode that lands with a ticket that is no longer the pane's is thrown away, as
//! the page drops a bitmap that finishes after a newer one started (SHELL-BROWSER-12). A browser
//! with no tab has no picture, and a browser that closed lets its entry go, so a decode still
//! running for either lands on nothing.
//!
//! An image a window drew is not dropped by the atlas on its own; once replaced it is retired for a
//! view that has a window to hand back (`take_retired`), or every picture of a live page would stay
//! in the atlas for good. One no view drew (the Workspace asks for a picture of every browser it
//! shows, with nothing painting it yet) is not in any atlas, so it is simply dropped: retiring it
//! would keep every frame of a screencast until a view came to take them.

use std::collections::BTreeMap;
use std::sync::Arc;

use argus_desktop_core::browser::BrowsersPage;
use gpui_kit::RenderImage;

/// One browser's picture and the page's own size it was taken at, which a click's point is sent
/// in.
#[derive(Clone)]
pub struct Picture {
    pub image: Arc<RenderImage>,
    pub width: u32,
    pub height: u32,
}

#[derive(Default)]
pub struct Pictures {
    held: BTreeMap<u64, Held>,
    /// Counted across panes, so a ticket of a pane that closed is never a new pane's.
    tickets: u64,
    retired: Vec<Arc<RenderImage>>,
}

#[derive(Default)]
struct Held {
    ticket: u64,
    picture: Option<Picture>,
    /// Whether a view has been handed the current picture, so a window's atlas may hold it.
    drawn: bool,
}

impl Held {
    /// Takes the picture away; it is owed to an atlas only if it was drawn.
    fn take(&mut self) -> Option<Picture> {
        let old = self.picture.take();
        self.drawn = false;
        old
    }
}

impl Pictures {
    /// A picture of `pane` was heard and is about to be decoded: the ticket that decode lands with.
    pub fn start(&mut self, pane: u64) -> u64 {
        self.tickets += 1;
        self.held.entry(pane).or_default().ticket = self.tickets;
        self.tickets
    }

    /// A decode landed. Whether it was kept: not when a newer picture started since, or the
    /// browser lost its tab or closed.
    pub fn land(&mut self, pane: u64, ticket: u64, picture: Picture) -> bool {
        let Some(held) = self.held.get_mut(&pane).filter(|held| held.ticket == ticket) else {
            return false;
        };
        if let Some(old) = held.picture.replace(picture) {
            if std::mem::take(&mut held.drawn) {
                self.retired.push(old.image);
            }
        }
        true
    }

    /// The browsers the stage now lists: one that is gone lets its picture go, and one with no tab
    /// shows nothing, not the last of a tab that closed. The panes whose picture went.
    pub fn page(&mut self, page: &BrowsersPage) -> Vec<u64> {
        let mut gone = Vec::new();
        let open: Vec<u64> = page.browsers.iter().map(|browser| browser.pane).collect();
        for pane in self.held.keys().copied().collect::<Vec<_>>() {
            if !open.contains(&pane) {
                self.forget(pane);
                gone.push(pane);
            }
        }
        for browser in page.browsers.iter().filter(|browser| browser.shown.is_none()) {
            if self.blank(browser.pane) {
                gone.push(browser.pane);
            }
        }
        gone
    }

    /// Takes `pane`'s picture away and has a decode still running for it land on nothing; whether
    /// there was a picture.
    fn blank(&mut self, pane: u64) -> bool {
        let Some(held) = self.held.get_mut(&pane) else {
            return false;
        };
        self.tickets += 1;
        held.ticket = self.tickets;
        match (held.drawn, held.take()) {
            (_, None) => false,
            (drawn, Some(old)) => {
                if drawn {
                    self.retired.push(old.image);
                }
                true
            }
        }
    }

    fn forget(&mut self, pane: u64) {
        if let Some(mut held) = self.held.remove(&pane) {
            let drawn = held.drawn;
            if let Some(old) = held.take().filter(|_| drawn) {
                self.retired.push(old.image);
            }
        }
    }

    /// How many pictures have been set decoding, which a test reads to see that a batch of pictures
    /// of one pane started one.
    #[cfg(test)]
    pub fn started(&self) -> u64 {
        self.tickets
    }

    pub fn of(&self, pane: u64) -> Option<&Picture> {
        self.held.get(&pane)?.picture.as_ref()
    }

    /// A view takes `pane`'s picture to draw it, which makes the atlas a holder of it.
    pub fn draw(&mut self, pane: u64) -> Option<Picture> {
        let held = self.held.get_mut(&pane)?;
        held.drawn |= held.picture.is_some();
        held.picture.clone()
    }

    /// The images replaced or dropped since the last call, for the atlas to let go of.
    pub fn take_retired(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.retired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::browser::PageBrowser;

    fn picture() -> Picture {
        Picture { image: Arc::new(RenderImage::new(Vec::<image::Frame>::new())), width: 1280, height: 800 }
    }

    fn page(browsers: &[(u64, bool)]) -> BrowsersPage {
        BrowsersPage {
            browsers: browsers
                .iter()
                .map(|&(pane, tab)| PageBrowser {
                    pane,
                    owner: pane,
                    agent: "claude".into(),
                    url: String::new(),
                    title: String::new(),
                    tabs: Vec::new(),
                    shown: tab.then(|| "t".into()),
                    following: true,
                    held: false,
                    driver: None,
                    cursor: argus_protocol::BrowserCursor::Default,
                    dialog: None,
                    select: None,
                    sized: None,
                    context: None,
                })
                .collect(),
        }
    }

    #[test]
    fn the_newest_picture_wins_whichever_decode_finishes_last() {
        let mut pictures = Pictures::default();
        let (older, newer) = (pictures.start(1), pictures.start(1));
        assert!(pictures.land(1, newer, picture()));
        assert!(!pictures.land(1, older, picture()), "an older decode that lands after is dropped");
        assert_eq!(pictures.take_retired().len(), 0, "the dropped one was never held, so the atlas never had it");
        assert!(pictures.of(1).is_some());
    }

    #[test]
    fn a_picture_replaced_is_retired_for_the_atlas() {
        let mut pictures = Pictures::default();
        let first = pictures.start(1);
        pictures.land(1, first, picture());
        pictures.draw(1);
        let second = pictures.start(1);
        pictures.land(1, second, picture());
        assert_eq!(pictures.take_retired().len(), 1);
        assert!(pictures.take_retired().is_empty(), "each is handed back once");
    }

    #[test]
    fn pictures_no_view_drew_are_dropped_not_kept_for_the_atlas() {
        let mut pictures = Pictures::default();
        for _ in 0..500 {
            let ticket = pictures.start(1);
            pictures.land(1, ticket, picture());
        }
        assert!(pictures.take_retired().is_empty(), "a Workspace with no view drawing a picture holds no frame back");
        pictures.draw(1);
        pictures.page(&page(&[]));
        assert_eq!(pictures.take_retired().len(), 1, "a drawn one is handed back when its browser closes");
    }

    #[test]
    fn a_browser_with_no_tab_has_no_picture_and_one_still_decoding_lands_on_nothing() {
        let mut pictures = Pictures::default();
        let shown = pictures.start(1);
        pictures.land(1, shown, picture());
        let coming = pictures.start(1);
        assert_eq!(pictures.page(&page(&[(1, false)])), [1]);
        assert!(pictures.of(1).is_none());
        assert!(!pictures.land(1, coming, picture()), "that picture was of a tab that closed");
        assert_eq!(pictures.page(&page(&[(1, false)])), Vec::<u64>::new(), "nothing more to blank the second time");
    }

    #[test]
    fn a_browser_that_closed_lets_its_picture_go_and_a_new_one_of_that_pane_is_not_taken_for_it() {
        let mut pictures = Pictures::default();
        let old = pictures.start(1);
        pictures.land(1, old, picture());
        let running = pictures.start(1);
        assert_eq!(pictures.page(&page(&[(2, true)])), [1]);
        assert!(pictures.of(1).is_none());
        let fresh = pictures.start(1);
        assert!(!pictures.land(1, running, picture()), "the closed browser's decode is not the new one's");
        assert!(pictures.land(1, fresh, picture()));
    }

    #[test]
    fn a_browser_with_a_tab_keeps_its_picture_when_the_stage_is_told_again() {
        let mut pictures = Pictures::default();
        let ticket = pictures.start(1);
        pictures.land(1, ticket, picture());
        assert!(pictures.page(&page(&[(1, true)])).is_empty());
        assert!(pictures.of(1).is_some());
    }
}
