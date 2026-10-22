//! The browsers agents' panes run, as this window holds them: where each
//! is, its tabs and latest picture, and which of them the window draws.
//!
//! A browser is a pane of its own in the tree, which the Workspace's grid
//! tiles like any pane; this module holds only what the daemon says about
//! its page. The daemon owns the browsers. The person's hand on one, or
//! their close, is sent and changes nothing here until the daemon says it
//! did.

pub mod address;
pub mod key_input;
pub mod keys;
pub mod picture;
pub mod status;
pub mod viewport;
pub mod within;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use argus_protocol::{BrowserDriver, BrowserFrame, BrowserInput, BrowserPage, BrowserTab, ClientMsg, PaneId};
use serde::Serialize;

use crate::connection::sent::Sent;
use picture::Picture;
use viewport::Viewports;

/// The Browsers stage, whole, whenever a browser opens, moves or closes.
/// Its pictures come apart, as `BrowserShot`s, since they come many times a
/// second.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BrowsersPage {
    /// In pane order, which is the order the agents were started in.
    pub browsers: Vec<PageBrowser>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageBrowser {
    /// The browser's own pane, which the grid tiles.
    pub pane: u64,
    /// The agent's pane that owns it, and so closes it.
    pub owner: u64,
    /// The owner's title, which names the agent driving it.
    pub agent: String,
    pub url: String,
    pub title: String,
    pub tabs: Vec<BrowserTab>,
    /// The tab whose pictures come, by id.
    pub shown: Option<String>,
    /// Whether that is the tab the agent works in, not one the person
    /// picked.
    pub following: bool,
    /// Whether the person holds it, the agent's input refused.
    pub held: bool,
    pub driver: Option<BrowserDriver>,
}

/// One picture of a pane's browser, as the daemon sent it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BrowserShot {
    pub pane: u64,
    /// A JPEG, as base64.
    pub jpeg: String,
    /// The page's CSS pixels, which a click's point is sent in.
    pub width: u32,
    pub height: u32,
}

#[derive(Default)]
pub struct Browsers {
    open: BTreeMap<PaneId, BrowserPage>,
    shots: BTreeMap<PaneId, BrowserShot>,
    /// `shots` decoded for a window that draws them natively, on its first
    /// ask for each, and dropped with the shot it was made from.
    decoded: BTreeMap<PaneId, Option<Arc<Picture>>>,
    /// Shots given so far, the `serial` of a `Picture`.
    served: u64,
    /// What each cell has said of its room, and what the daemon was told.
    viewports: Viewports,
    /// The browsers whose pictures the daemon was last asked for; `None`
    /// before a connection asks for any.
    watching: Option<Vec<PaneId>>,
    sent: Sent<BrowsersPage>,
}

impl Browsers {
    /// A new connection replays every browser the daemon runs, so what
    /// this window had is dropped rather than kept stale.
    pub fn connected(&mut self, out: &mut Vec<ClientMsg>) {
        self.open.clear();
        self.shots.clear();
        self.decoded.clear();
        self.viewports.reconnected();
        self.watching = Some(Vec::new());
        out.push(ClientMsg::WatchBrowsers { showing: Vec::new() });
    }

    /// Asks for the pictures of the browsers the window draws now, when
    /// they are not the ones it asked for last: a browser no one looks at
    /// costs the connection nothing.
    pub fn watch(&mut self, showing: Vec<PaneId>, out: &mut Vec<ClientMsg>) {
        if self.watching.as_ref().is_some_and(|asked| *asked != showing) {
            self.watching = Some(showing.clone());
            out.push(ClientMsg::WatchBrowsers { showing });
        }
    }

    /// The browsers drawn: those in the Workspace's grid, when it is on
    /// screen, and on the Browsers stage every one as a tile. `placed` is
    /// every pane the grid holds, browsers among the rest.
    pub fn drawn(&self, placed: &[PaneId], stage_up: bool) -> Vec<PaneId> {
        let mut drawn: Vec<PaneId> = if stage_up { self.open.keys().copied().collect() } else { Vec::new() };
        drawn.extend(placed.iter().copied().filter(|pane| self.has(*pane)));
        drawn.sort();
        drawn.dedup();
        drawn
    }

    /// Where a browser is now; `true` when it has just opened, which a
    /// pane beside its agent's is opened for.
    pub fn page(&mut self, page: BrowserPage) -> bool {
        // No tab, no page: a page that loads is not sent a closed one's picture.
        if page.shown.is_none() {
            self.shots.remove(&page.pane);
            self.decoded.remove(&page.pane);
        }
        self.open.insert(page.pane, page).is_none()
    }

    /// The agent's pane that owns `pane`, which is a browser.
    pub fn owner(&self, pane: PaneId) -> Option<PaneId> {
        self.open.get(&pane).map(|page| page.owner)
    }

    /// A picture, kept as the one a page that loads now is sent, and handed
    /// back to send the page there is.
    pub fn shot(&mut self, frame: BrowserFrame) -> BrowserShot {
        let shot = BrowserShot { pane: frame.pane.0, jpeg: frame.jpeg, width: frame.width, height: frame.height };
        self.shots.insert(frame.pane, shot.clone());
        self.decoded.remove(&frame.pane);
        self.served += 1;
        shot
    }

    /// The latest picture of `pane`'s browser as bytes, for a window that
    /// draws it without the page's base64 path: decoded on first ask and
    /// shared after. Latest wins, and a browser with no tab, or one that
    /// closed, has none. A picture whose base64 is bad is none too.
    pub fn picture(&mut self, pane: PaneId) -> Option<Arc<Picture>> {
        let shot = self.shots.get(&pane)?;
        let served = self.served;
        self.decoded.entry(pane).or_insert_with(|| picture::decode(shot, served).map(Arc::new)).clone()
    }

    pub fn closed(&mut self, pane: PaneId) {
        self.open.remove(&pane);
        self.shots.remove(&pane);
        self.decoded.remove(&pane);
        self.viewports.closed(pane);
    }

    pub fn has(&self, pane: PaneId) -> bool {
        self.open.contains_key(&pane)
    }

    pub fn input(&self, pane: PaneId, input: BrowserInput, out: &mut Vec<ClientMsg>) {
        if self.has(pane) {
            out.push(ClientMsg::BrowserInput { pane, input });
        }
    }

    /// A cell's box is now `width` x `height` of the window's pixels at
    /// `scale`, `now` on a clock the caller reads: the daemon is told when
    /// that is news to it (`viewport`). A tile never reports.
    pub fn viewport(&mut self, pane: PaneId, width: f64, height: f64, scale: f64, now: Duration, out: &mut Vec<ClientMsg>) {
        let open = self.has(pane);
        if let Some(viewport) = self.viewports.report(pane, width, height, scale, open, now) {
            self.send_viewport(pane, viewport, out);
        }
    }

    /// Sends the sizes `viewport` held back that are due at `now`. Ask
    /// again at `next_viewport`.
    pub fn viewports_due(&mut self, now: Duration, out: &mut Vec<ClientMsg>) {
        let open = self.open.keys().copied().collect::<Vec<_>>();
        for (pane, viewport) in self.viewports.due(now, |pane| open.contains(&pane)) {
            self.send_viewport(pane, viewport, out);
        }
    }

    /// When a size held back may go, on the caller's clock; `None` when
    /// none is held.
    pub fn next_viewport(&self) -> Option<Duration> {
        self.viewports.next(|pane| self.has(pane))
    }

    fn send_viewport(&self, pane: PaneId, viewport: viewport::Viewport, out: &mut Vec<ClientMsg>) {
        let input = BrowserInput::Viewport { width: viewport.width, height: viewport.height, scale: viewport.scale };
        self.input(pane, input, out);
    }

    pub fn close(&self, pane: PaneId, out: &mut Vec<ClientMsg>) {
        if self.has(pane) {
            out.push(ClientMsg::CloseBrowser { pane });
        }
    }

    /// What to send the page, if it has not heard it. `agent` titles an
    /// owner's pane.
    /// With no browser running there is nothing to say: the page draws its
    /// empty stage from nothing heard, as from the last browser going.
    pub fn take(&mut self, agent: impl Fn(PaneId) -> Option<String>) -> Option<BrowsersPage> {
        let now = (!self.open.is_empty()).then(|| BrowsersPage {
            browsers: self
                .open
                .values()
                .map(|page| PageBrowser {
                    pane: page.pane.0,
                    owner: page.owner.0,
                    agent: agent(page.owner).unwrap_or_else(|| format!("pane {}", page.pane.0)),
                    url: page.url.clone(),
                    title: page.title.clone(),
                    tabs: page.tabs.clone(),
                    shown: page.shown.clone(),
                    following: page.following,
                    held: page.held,
                    driver: page.driver,
                })
                .collect(),
        });
        self.sent.take(now)
    }

    /// The page reloaded: the stage is sent again, and every picture.
    pub fn forget(&mut self) -> Vec<BrowserShot> {
        self.sent.forget();
        self.shots.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(pane: u64, url: &str) -> BrowserPage {
        BrowserPage { pane: PaneId(pane), owner: PaneId(pane), url: url.into(), title: String::new(), tabs: Vec::new(), shown: None, following: true, held: false, driver: None }
    }

    #[test]
    fn a_browser_is_new_once_and_names_its_owner() {
        let mut browsers = Browsers::default();
        assert!(browsers.page(page(1, "about:blank")), "a browser that opens is new");
        assert!(!browsers.page(page(1, "https://b.test")), "one that moves is not");
        assert_eq!(browsers.owner(PaneId(1)), Some(PaneId(1)));
        assert_eq!(browsers.owner(PaneId(9)), None);
        browsers.closed(PaneId(1));
        assert!(browsers.page(page(1, "about:blank")), "one that came back is new again");
    }

    #[test]
    fn the_stage_tells_each_browser_by_its_owners_title() {
        let mut browsers = Browsers::default();
        let mut open = page(5, "about:blank");
        open.owner = PaneId(2);
        browsers.page(open);
        let told = browsers.take(|pane| (pane == PaneId(2)).then(|| "claude".to_string())).unwrap();
        assert_eq!((told.browsers[0].pane, told.browsers[0].owner, told.browsers[0].agent.as_str()), (5, 2, "claude"));
    }

    #[test]
    fn nothing_is_said_until_a_browser_runs_and_once_when_the_last_goes() {
        let mut browsers = Browsers::default();
        assert_eq!(browsers.take(|_| None), None);
        browsers.page(page(1, "about:blank"));
        assert!(browsers.take(|_| None).is_some());
        browsers.closed(PaneId(1));
        assert_eq!(browsers.take(|_| None), Some(BrowsersPage::default()));
        assert_eq!(browsers.take(|_| None), None);
    }

    #[test]
    fn the_hand_goes_only_to_a_browser_that_is_there() {
        let mut browsers = Browsers::default();
        let mut out = Vec::new();
        browsers.input(PaneId(1), BrowserInput::Reload, &mut out);
        browsers.close(PaneId(1), &mut out);
        assert!(out.is_empty());
        browsers.page(page(1, "about:blank"));
        browsers.input(PaneId(1), BrowserInput::Reload, &mut out);
        assert!(matches!(out[..], [ClientMsg::BrowserInput { pane: PaneId(1), .. }]));
    }

    #[test]
    fn pictures_are_asked_for_only_of_the_browsers_drawn_and_again_only_when_that_changes() {
        let mut browsers = Browsers::default();
        let mut out = Vec::new();
        browsers.watch(vec![PaneId(1)], &mut out);
        assert!(out.is_empty(), "nothing is asked before a connection is");
        browsers.connected(&mut out);
        browsers.page(page(1, "about:blank"));
        browsers.page(page(2, "about:blank"));
        out.clear();
        assert_eq!(browsers.drawn(&[], false), [], "nothing on screen draws none");
        assert_eq!(browsers.drawn(&[PaneId(7), PaneId(2)], false), [PaneId(2)], "the grid draws its browsers, not its other panes");
        assert_eq!(browsers.drawn(&[], true), [PaneId(1), PaneId(2)], "the stage's tiles draw them all");
        assert_eq!(browsers.drawn(&[PaneId(2)], true), [PaneId(1), PaneId(2)], "one browser is asked for once");
        browsers.watch(browsers.drawn(&[], true), &mut out);
        browsers.watch(browsers.drawn(&[], true), &mut out);
        assert!(matches!(&out[..], [ClientMsg::WatchBrowsers { showing }] if showing == &[PaneId(1), PaneId(2)]), "{out:?}");
    }

    #[test]
    fn a_browser_whose_last_tab_closed_keeps_no_picture_for_a_page_that_loads() {
        let mut browsers = Browsers::default();
        let mut shown = page(1, "https://a.test");
        shown.shown = Some("t1".into());
        browsers.page(shown);
        browsers.shot(BrowserFrame { pane: PaneId(1), jpeg: "x".into(), width: 1, height: 1 });
        assert_eq!(browsers.forget().len(), 1);
        browsers.page(page(1, ""));
        assert!(browsers.forget().is_empty(), "the picture was of a tab that closed");
    }

    #[test]
    fn a_reconnect_drops_what_the_last_daemon_ran() {
        let mut browsers = Browsers::default();
        browsers.page(page(1, "about:blank"));
        browsers.shot(BrowserFrame { pane: PaneId(1), jpeg: "x".into(), width: 1, height: 1 });
        let mut out = Vec::new();
        browsers.connected(&mut out);
        assert!(matches!(&out[..], [ClientMsg::WatchBrowsers { showing }] if showing.is_empty()));
        assert!(browsers.forget().is_empty());
        assert_eq!(browsers.take(|_| None), None);
    }

    fn shot(pane: u64, jpeg: &str) -> BrowserFrame {
        BrowserFrame { pane: PaneId(pane), jpeg: jpeg.into(), width: 1280, height: 800 }
    }

    #[test]
    fn a_picture_is_decoded_once_and_the_latest_wins() {
        let mut browsers = Browsers::default();
        let mut open = page(1, "https://a.test");
        open.shown = Some("t".into());
        browsers.page(open);
        assert!(browsers.picture(PaneId(1)).is_none(), "nothing heard yet");
        browsers.shot(shot(1, "aGk="));
        let first = browsers.picture(PaneId(1)).unwrap();
        assert_eq!(&*first.bytes, b"hi");
        assert!(Arc::ptr_eq(&first, &browsers.picture(PaneId(1)).unwrap()), "shared, not decoded again");
        browsers.shot(shot(1, "eW8="));
        let second = browsers.picture(PaneId(1)).unwrap();
        assert_eq!(&*second.bytes, b"yo");
        assert!(second.serial > first.serial);
    }

    #[test]
    fn a_picture_goes_with_the_tab_the_browser_and_the_connection() {
        let mut browsers = Browsers::default();
        let mut with_tab = page(1, "https://a.test");
        with_tab.shown = Some("t".into());
        browsers.page(with_tab.clone());
        browsers.shot(shot(1, "aGk="));
        assert!(browsers.picture(PaneId(1)).is_some());
        browsers.page(page(1, ""));
        assert!(browsers.picture(PaneId(1)).is_none(), "no tab, no picture");
        browsers.page(with_tab);
        browsers.shot(shot(1, "aGk="));
        browsers.closed(PaneId(1));
        assert!(browsers.picture(PaneId(1)).is_none());
        browsers.shot(shot(1, "aGk="));
        browsers.connected(&mut Vec::new());
        assert!(browsers.picture(PaneId(1)).is_none());
    }

    #[test]
    fn a_viewport_is_told_to_a_browser_that_is_there_and_again_after_a_reconnect() {
        let mut browsers = Browsers::default();
        let (mut out, ms) = (Vec::new(), Duration::from_millis);
        browsers.viewport(PaneId(1), 600.0, 400.0, 2.0, ms(0), &mut out);
        assert!(out.is_empty(), "no such browser yet");
        browsers.page(page(1, "about:blank"));
        browsers.viewports_due(ms(1), &mut out);
        assert!(matches!(&out[..], [ClientMsg::BrowserInput { pane: PaneId(1), input: BrowserInput::Viewport { width: 600, height: 400, scale } }] if *scale == 2.0), "{out:?}");
        out.clear();
        browsers.viewport(PaneId(1), 600.0, 400.0, 2.0, ms(900), &mut out);
        assert!(out.is_empty(), "the daemon has it");
        browsers.connected(&mut out);
        out.clear();
        browsers.page(page(1, "about:blank"));
        browsers.viewports_due(ms(1000), &mut out);
        assert_eq!(out.len(), 1, "the new daemon is told without the box changing");
    }
}
