//! The browsers agents' panes run, as this window holds them: where each
//! is, its latest picture, and which one the Browsers stage has chosen —
//! none for every one at once.
//!
//! The daemon owns the browsers. The person's hand on one, or their close,
//! is sent and changes nothing here until the daemon says it did; only
//! which one is chosen is the window's own.

use std::collections::BTreeMap;

use argus_protocol::{BrowserFrame, BrowserInput, BrowserPage, ClientMsg, PaneId};
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
    sent: Sent<BrowsersPage>,
}

impl Browsers {
    /// A new connection replays every browser the daemon runs, so what
    /// this window had is dropped rather than kept stale.
    pub fn connected(&mut self, out: &mut Vec<ClientMsg>) {
        self.open.clear();
        self.shots.clear();
        out.push(ClientMsg::WatchBrowsers);
    }

    pub fn page(&mut self, page: BrowserPage) {
        self.open.insert(page.pane, page);
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
                })
                .collect(),
            chosen: self.chosen.map(|pane| pane.0),
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
        BrowserPage { pane: PaneId(pane), url: url.into(), title: String::new() }
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
    fn a_reconnect_drops_what_the_last_daemon_ran() {
        let mut browsers = Browsers::default();
        browsers.page(page(1, "about:blank"));
        browsers.shot(BrowserFrame { pane: PaneId(1), jpeg: "x".into(), width: 1, height: 1 });
        let mut out = Vec::new();
        browsers.connected(&mut out);
        assert!(matches!(out[..], [ClientMsg::WatchBrowsers]));
        assert!(browsers.forget().is_empty());
        assert_eq!(browsers.take(|_| None), None);
    }
}
