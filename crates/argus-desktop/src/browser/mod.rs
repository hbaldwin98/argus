//! The browsers agents' panes run, as this window holds them: where each
//! is, its tabs and latest picture, which one the Browsers stage has
//! chosen — none for every one at once — and which one the browser panel
//! in its dock shows.
//!
//! The daemon owns the browsers. The person's hand on one, or their close,
//! is sent and changes nothing here until the daemon says it did; only
//! which one is chosen, and which is docked, are the window's own.

use std::collections::BTreeMap;

use argus_protocol::{BrowserDriver, BrowserFrame, BrowserInput, BrowserPage, BrowserTab, ClientMsg, PaneId};
use serde::Serialize;

use crate::connection::sent::Sent;

/// The Browsers stage, whole, whenever where a browser is or which is
/// chosen changes. Its pictures come apart, as `BrowserShot`s, since they
/// come many times a second.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct BrowsersPage {
    /// In pane order, which is the order the agents were started in.
    pub browsers: Vec<PageBrowser>,
    /// The browser that fills the stage; every browser at once while none.
    pub chosen: Option<u64>,
    /// The browser the browser panel shows.
    pub docked: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PageBrowser {
    pub pane: u64,
    /// The pane's title, which names the agent driving it.
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
    chosen: Option<PaneId>,
    docked: Option<PaneId>,
    /// The pane on screen the panel last followed, so it follows the person
    /// to another pane and otherwise stays where they put it.
    followed: Option<PaneId>,
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

    /// The browsers drawn: the docked one while its panel is out, and on the
    /// Browsers stage the chosen one, or every one as tiles.
    pub fn drawn(&self, panel_out: bool, stage_up: bool) -> Vec<PaneId> {
        let mut drawn = Vec::new();
        if stage_up {
            drawn.extend(self.chosen.map_or_else(|| self.open.keys().copied().collect(), |pane| vec![pane]));
        }
        if let Some(docked) = self.docked.filter(|pane| panel_out && !drawn.contains(pane)) {
            drawn.push(docked);
        }
        drawn.sort();
        drawn
    }

    /// Where a browser is now; `true` when it has just opened, which the
    /// browser panel comes out for, on it.
    pub fn page(&mut self, page: BrowserPage) -> bool {
        let pane = page.pane;
        let opened = self.open.insert(pane, page).is_none();
        if opened {
            self.docked = Some(pane);
        }
        opened
    }

    /// The pane on screen: going to one with a browser makes it the one the
    /// panel shows, so the panel follows the person between agents. Staying
    /// there leaves the panel on whichever the person docked since.
    pub fn follow(&mut self, on_screen: Option<PaneId>) {
        if on_screen == self.followed {
            return;
        }
        self.followed = on_screen;
        if let Some(pane) = on_screen.filter(|pane| self.has(*pane)) {
            self.docked = Some(pane);
        }
    }

    /// The person put `pane`'s browser in the panel. The stage lets it go
    /// back to every browser, as one browser is drawn large in one place.
    pub fn dock(&mut self, pane: PaneId) {
        if self.has(pane) {
            self.docked = Some(pane);
            if self.chosen == Some(pane) {
                self.chosen = None;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    pub fn docked(&self) -> Option<PaneId> {
        self.docked
    }

    /// A picture, kept as the one a page that loads now is sent, and handed
    /// back to send the page there is.
    pub fn shot(&mut self, frame: BrowserFrame) -> BrowserShot {
        let shot = BrowserShot { pane: frame.pane.0, jpeg: frame.jpeg, width: frame.width, height: frame.height };
        self.shots.insert(frame.pane, shot.clone());
        shot
    }

    /// A browser went. The stage goes back to every browser when it was the
    /// chosen one, rather than to an empty frame.
    pub fn closed(&mut self, pane: PaneId) {
        self.open.remove(&pane);
        self.shots.remove(&pane);
        if self.chosen == Some(pane) {
            self.chosen = None;
        }
        if self.docked == Some(pane) {
            self.docked = self.open.keys().next_back().copied();
        }
    }

    /// Chooses one browser to fill the stage, or none for every one. A
    /// pane without a browser is not chosen: the stage shows them all.
    pub fn choose(&mut self, pane: Option<PaneId>) {
        self.chosen = pane.filter(|pane| self.open.contains_key(pane));
    }

    pub fn has(&self, pane: PaneId) -> bool {
        self.open.contains_key(&pane)
    }

    pub fn input(&self, pane: PaneId, input: BrowserInput, out: &mut Vec<ClientMsg>) {
        if self.has(pane) {
            out.push(ClientMsg::BrowserInput { pane, input });
        }
    }

    pub fn close(&self, pane: PaneId, out: &mut Vec<ClientMsg>) {
        if self.has(pane) {
            out.push(ClientMsg::CloseBrowser { pane });
        }
    }

    /// What to send the page, if it has not heard it. `agent` names a pane.
    /// With no browser running there is nothing to say: the page draws its
    /// empty stage from nothing heard, as from the last browser going.
    pub fn take(&mut self, agent: impl Fn(PaneId) -> Option<String>) -> Option<BrowsersPage> {
        let now = (!self.open.is_empty()).then(|| BrowsersPage {
            browsers: self
                .open
                .values()
                .map(|page| PageBrowser {
                    pane: page.pane.0,
                    agent: agent(page.pane).unwrap_or_else(|| format!("pane {}", page.pane.0)),
                    url: page.url.clone(),
                    title: page.title.clone(),
                    tabs: page.tabs.clone(),
                    shown: page.shown.clone(),
                    following: page.following,
                    held: page.held,
                    driver: page.driver,
                })
                .collect(),
            chosen: self.chosen.map(|pane| pane.0),
            docked: self.docked.map(|pane| pane.0),
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
        BrowserPage { pane: PaneId(pane), url: url.into(), title: String::new(), tabs: Vec::new(), shown: None, following: true, held: false, driver: None }
    }

    #[test]
    fn the_panel_shows_the_newest_browser_then_the_one_on_screen() {
        let mut browsers = Browsers::default();
        assert!(browsers.page(page(1, "about:blank")), "a browser that opens brings the panel out");
        assert!(browsers.page(page(2, "about:blank")));
        assert!(!browsers.page(page(2, "https://b.test")), "one that moves does not");
        assert_eq!(browsers.docked(), Some(PaneId(2)));
        browsers.follow(Some(PaneId(1)));
        assert_eq!(browsers.docked(), Some(PaneId(1)));
        browsers.dock(PaneId(2));
        browsers.follow(Some(PaneId(1)));
        assert_eq!(browsers.docked(), Some(PaneId(2)), "staying on a pane leaves the panel where the person put it");
        browsers.follow(Some(PaneId(9)));
        browsers.follow(Some(PaneId(1)));
        assert_eq!(browsers.docked(), Some(PaneId(1)), "going back to it follows it again");
        browsers.follow(Some(PaneId(9)));
        assert_eq!(browsers.docked(), Some(PaneId(1)), "a pane without a browser leaves the panel be");
        browsers.closed(PaneId(1));
        assert_eq!(browsers.docked(), Some(PaneId(2)));
        browsers.closed(PaneId(2));
        assert_eq!(browsers.docked(), None);
    }

    #[test]
    fn a_chosen_browser_that_closes_gives_the_stage_back_to_all_of_them() {
        let mut browsers = Browsers::default();
        browsers.page(page(1, "https://a.test"));
        browsers.page(page(2, "https://b.test"));
        browsers.choose(Some(PaneId(2)));
        assert_eq!(browsers.take(|_| None).unwrap().chosen, Some(2));
        browsers.closed(PaneId(2));
        let page = browsers.take(|_| None).unwrap();
        assert_eq!((page.chosen, page.browsers.len()), (None, 1));
    }

    #[test]
    fn a_pane_without_a_browser_is_not_chosen() {
        let mut browsers = Browsers::default();
        browsers.page(page(1, "about:blank"));
        browsers.choose(Some(PaneId(7)));
        assert_eq!(browsers.take(|_| None).unwrap().chosen, None);
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
        assert_eq!(browsers.drawn(false, false), [], "nothing on screen draws none");
        assert_eq!(browsers.drawn(true, false), [PaneId(2)], "the panel draws the docked one");
        assert_eq!(browsers.drawn(false, true), [PaneId(1), PaneId(2)], "the stage's tiles draw them all");
        browsers.choose(Some(PaneId(1)));
        assert_eq!(browsers.drawn(true, true), [PaneId(1), PaneId(2)]);
        browsers.watch(browsers.drawn(false, true), &mut out);
        browsers.watch(browsers.drawn(false, true), &mut out);
        assert!(matches!(&out[..], [ClientMsg::WatchBrowsers { showing }] if showing == &[PaneId(1)]), "{out:?}");
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
}
