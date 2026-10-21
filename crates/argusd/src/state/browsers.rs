//! Which panes run a browser, where each is and its latest picture, and
//! which go when.
//!
//! In memory only, as views are: a browser is a process this daemon
//! started, and ends with it or with its pane. Every change is made, and
//! told to the watchers, under the one lock, so a watcher's replay and the
//! changes after it do not cross.

use std::collections::HashMap;
use std::sync::Arc;

use argus_protocol::{BrowserFrame, BrowserInput, BrowserPage, BrowserRequest, PaneId, ServerMsg};
use tokio::sync::{broadcast, mpsc};

use super::*;
use crate::browser::{Browser, Report, Seen};

/// A pane's running browser, and what was last heard of it.
struct Open {
    browser: Browser,
    /// Which start this is, so a browser that ends after its pane has
    /// opened another does not take the new one with it.
    started: u64,
    page: BrowserPage,
    frame: Option<Arc<BrowserFrame>>,
}

#[derive(Default)]
pub(super) struct Browsers {
    open: HashMap<PaneId, Open>,
    starts: u64,
}

/// What changed among the browsers, for the connections watching them.
#[derive(Debug, Clone)]
pub enum BrowserNews {
    Page(BrowserPage),
    Frame(Arc<BrowserFrame>),
    Closed { pane: PaneId },
}

impl BrowserNews {
    pub(crate) fn message(self) -> ServerMsg {
        match self {
            BrowserNews::Page(page) => ServerMsg::BrowserPage(page),
            BrowserNews::Frame(frame) => ServerMsg::BrowserFrame(Box::new((*frame).clone())),
            BrowserNews::Closed { pane } => ServerMsg::BrowserClosed { pane },
        }
    }
}

impl Daemon {
    /// An agent's `browser` request: its pane's browser opened, answered
    /// with the DevTools URL to attach to, or closed, answered with nothing.
    ///
    /// The URL is the pane API's, behind its token: the browser listens on
    /// no port of its own. Opening one already open answers it again. The
    /// browser is started outside the lock, so two requests at once may
    /// both start one; the second to finish is dropped.
    pub async fn browser_for_agent(self: &Arc<Self>, pane: PaneId, request: BrowserRequest) -> anyhow::Result<String> {
        self.agent_checkout(pane)?;
        if request == BrowserRequest::Close {
            self.close_browser(pane);
            return Ok(String::new());
        }
        let port = self.hook_port.load(std::sync::atomic::Ordering::Relaxed);
        let endpoint = argus_protocol::devtools_url(port, pane, self.hook_token());
        if self.browsers.lock().unwrap().open.contains_key(&pane) {
            return Ok(endpoint);
        }
        let (reports, heard) = mpsc::unbounded_channel();
        let browser = Browser::start(reports).await?;
        let mut browsers = self.browsers.lock().unwrap();
        // The pane may have gone while the browser started; looked for
        // under this lock, which `drop_browser_of` takes too.
        self.agent_checkout(pane)?;
        if browsers.open.contains_key(&pane) {
            return Ok(endpoint);
        }
        browsers.starts += 1;
        let started = browsers.starts;
        let page = BrowserPage {
            pane,
            url: "about:blank".into(),
            title: String::new(),
            tabs: Vec::new(),
            shown: None,
            following: true,
            held: false,
            driver: None,
        };
        browsers.open.insert(pane, Open { browser, started, page: page.clone(), frame: None });
        self.pushes.browser(&browsers, BrowserNews::Page(page));
        drop(browsers);
        tokio::spawn(heed(self.clone(), pane, started, heard));
        Ok(endpoint)
    }

    /// A line to the pane's browser for an agent's browser tool, if it has
    /// one open.
    pub(crate) fn attach_browser(&self, pane: PaneId) -> Option<(crate::browser::mux::Line, mpsc::UnboundedReceiver<String>)> {
        self.browsers.lock().unwrap().open.get(&pane).map(|open| open.browser.attach())
    }

    /// The person's hand on a pane's browser; one the pane does not have
    /// is ignored, as it has gone.
    pub fn browser_input(&self, pane: PaneId, input: BrowserInput) {
        if let Some(open) = self.browsers.lock().unwrap().open.get(&pane) {
            open.browser.input(input);
        }
    }

    /// The person's close, or the agent's: the browser is killed.
    pub fn close_browser(&self, pane: PaneId) {
        let mut browsers = self.browsers.lock().unwrap();
        if browsers.open.remove(&pane).is_some() {
            self.pushes.browser(&browsers, BrowserNews::Closed { pane });
        }
    }

    /// Called as a pane leaves the tree: its browser goes with it.
    pub(super) fn drop_browser_of(&self, pane: PaneId) {
        self.close_browser(pane);
    }

    /// Every browser as it stands — where it is, then its latest picture —
    /// and the receiver of what follows.
    pub fn watch_browsers(&self) -> (Vec<BrowserNews>, broadcast::Receiver<BrowserNews>) {
        let browsers = self.browsers.lock().unwrap();
        let mut now = Vec::new();
        for open in browsers.open.values() {
            now.push(BrowserNews::Page(open.page.clone()));
            now.extend(open.frame.clone().map(BrowserNews::Frame));
        }
        (now, self.pushes.subscribe_browsers())
    }

    /// Every browser killed, as the daemon stops: on Windows a job would
    /// end each anyway, but elsewhere one would outlive it.
    pub fn close_browsers(&self) {
        let panes: Vec<PaneId> = self.browsers.lock().unwrap().open.keys().copied().collect();
        for pane in panes {
            self.close_browser(pane);
        }
    }

    /// What a browser said of itself, kept and told to the watchers.
    /// `false` once it is no longer this start's browser to speak for.
    fn browser_said(&self, pane: PaneId, started: u64, report: Report) -> bool {
        let mut browsers = self.browsers.lock().unwrap();
        let Some(open) = browsers.open.get_mut(&pane).filter(|open| open.started == started) else {
            return false;
        };
        let news = match report {
            Report::Page(seen) => {
                let on = seen.shown.as_deref().and_then(|id| seen.tabs.iter().find(|tab| tab.id == id));
                let (url, title) = on.map(|tab| (tab.url.clone(), tab.title.clone())).unwrap_or_default();
                let Seen { tabs, shown, following, held, driver } = seen;
                open.page = BrowserPage { pane, url, title, tabs, shown, following, held, driver };
                BrowserNews::Page(open.page.clone())
            }
            Report::Frame { jpeg, width, height } => {
                let frame = Arc::new(BrowserFrame { pane, jpeg, width, height });
                open.frame = Some(frame.clone());
                BrowserNews::Frame(frame)
            }
        };
        self.pushes.browser(&browsers, news);
        true
    }

    /// The browser ended on its own: it is taken off as a close is.
    fn browser_ended(&self, pane: PaneId, started: u64) {
        let mut browsers = self.browsers.lock().unwrap();
        if browsers.open.get(&pane).is_some_and(|open| open.started == started) {
            browsers.open.remove(&pane);
            self.pushes.browser(&browsers, BrowserNews::Closed { pane });
        }
    }
}

/// Reads what one browser reports until it ends.
async fn heed(daemon: Arc<Daemon>, pane: PaneId, started: u64, mut heard: mpsc::UnboundedReceiver<Report>) {
    while let Some(report) = heard.recv().await {
        if !daemon.browser_said(pane, started, report) {
            return;
        }
    }
    daemon.browser_ended(pane, started);
}
