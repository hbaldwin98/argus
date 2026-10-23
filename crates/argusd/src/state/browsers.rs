//! Which browser panes run a browser, where each is and its latest
//! picture, and which go when.
//!
//! A browser is a pane of its own in its agent's checkout, so it tiles and
//! closes as any pane does, and every table here is keyed by that pane.
//! In memory only, as views are: a browser is a process this daemon
//! started, and ends with it or with its pane, or with its agent's. Every
//! change is made, and told to the watchers, under the one lock, so a
//! watcher's replay and the changes after it do not cross.
//!
//! Lock order: `browsers` before `inner`, as a lookup of the agent's place
//! is made under it. The tree is broadcast only once both are let go.

use std::collections::HashMap;
use std::sync::Arc;

use argus_protocol::{BrowserAct, BrowserCursor, BrowserFrame, BrowserInput, BrowserPage, BrowserRequest, PaneId, PaneKind, ServerMsg};
use tokio::sync::{broadcast, mpsc};

use super::*;
use crate::browser::process::BrowserProcess;
use crate::browser::{Browser, Report, Seen};

/// The title of a browser pane that has no page to name it.
const UNTITLED: &str = "Browser";

/// A browser pane's running browser, and what was last heard of it.
struct Open {
    /// The agent pane that opened it, and whose it is: one browser each.
    owner: PaneId,
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
    Page(Box<BrowserPage>),
    Frame(Arc<BrowserFrame>),
    Cursor { pane: PaneId, cursor: BrowserCursor },
    Closed { pane: PaneId },
}

impl BrowserNews {
    pub(crate) fn message(self) -> ServerMsg {
        match self {
            BrowserNews::Page(page) => ServerMsg::BrowserPage(*page),
            BrowserNews::Frame(frame) => ServerMsg::BrowserFrame(Box::new((*frame).clone())),
            BrowserNews::Cursor { pane, cursor } => ServerMsg::BrowserCursor { pane, cursor },
            BrowserNews::Closed { pane } => ServerMsg::BrowserClosed { pane },
        }
    }
}

impl Daemon {
    /// An agent's `browser` request: its browser opened, answered with the
    /// DevTools URL to attach to, or closed, answered with nothing.
    ///
    /// The URL is the pane API's, behind its token, and is the agent's own:
    /// the browser listens on no port of its own, and the agent's token and
    /// identity are what the pane API knows. Opening one already open
    /// answers it again. The browser is started outside the lock, so two
    /// requests at once may both start one; the second to finish is dropped.
    pub async fn browser_for_agent(self: &Arc<Self>, agent: PaneId, request: BrowserRequest) -> anyhow::Result<String> {
        self.agent_checkout(agent)?;
        match request {
            BrowserRequest::Close => {
                if let Some(pane) = self.browser_of(agent) {
                    self.close_browser(pane);
                }
                Ok(String::new())
            }
            BrowserRequest::Open => {
                self.agents_browser(agent).await?;
                let port = self.hook_port.load(std::sync::atomic::Ordering::Relaxed);
                Ok(argus_protocol::devtools_url(port, agent, self.hook_token()))
            }
            BrowserRequest::Act(act) => {
                // Going somewhere needs no browser open first; anything else
                // done to one that is not would only be refused by it.
                let pane = match self.browser_of(agent) {
                    Some(pane) => pane,
                    None if matches!(act, BrowserAct::Go { .. } | BrowserAct::New { .. }) => self.agents_browser(agent).await?,
                    None => anyhow::bail!("no browser is open; `argus-hook browser go <url>` opens one at the page"),
                };
                let answer = {
                    let browsers = self.browsers.lock().unwrap();
                    browsers.open.get(&pane).map(|open| open.browser.act(act))
                };
                let answer = answer.ok_or_else(|| anyhow::anyhow!("the browser closed"))?;
                answer.await.map_err(|_| anyhow::anyhow!("the browser closed before it answered"))?.map_err(anyhow::Error::msg)
            }
        }
    }

    /// The agent's browser pane, started first if it has none.
    async fn agents_browser(self: &Arc<Self>, agent: PaneId) -> anyhow::Result<PaneId> {
        if let Some(pane) = self.browser_of(agent) {
            return Ok(pane);
        }
        let (reports, heard) = mpsc::unbounded_channel();
        let browser = Browser::start(reports).await?;
        self.open_browser_pane(agent, browser, heard)
    }

    /// Puts a started browser in the tree as a pane of its own, in the
    /// checkout of the agent that opened it, and starts hearing it. One
    /// that arrives after the agent is gone, or after another has opened,
    /// is dropped, which ends it.
    pub(crate) fn open_browser_pane(
        self: &Arc<Self>,
        agent: PaneId,
        browser: Browser,
        heard: mpsc::UnboundedReceiver<Report>,
    ) -> anyhow::Result<PaneId> {
        let mut browsers = self.browsers.lock().unwrap();
        if let Some(existing) = browsers.open.iter().find(|(_, open)| open.owner == agent).map(|(pane, _)| *pane) {
            return Ok(existing);
        }
        // The agent may have gone while the browser started; looked for in
        // the tree under this lock, which `drop_browser_of` takes too, so
        // an agent retired after this is retired with its browser.
        let pane = {
            let mut inner = self.inner.lock().unwrap();
            let pane = PaneId(inner.ids.alloc());
            let Some(checkout) = checkouts_mut(&mut inner.projects)
                .find(|c| c.panes.iter().any(|p| p.id == agent && p.is_running_agent()))
            else {
                anyhow::bail!("source must be a live agent pane");
            };
            checkout.panes.push(Pane::new(pane, PaneKind::Browser, UNTITLED.to_string(), Box::new(BrowserProcess::new())));
            pane
        };
        browsers.starts += 1;
        let started = browsers.starts;
        let page = BrowserPage {
            pane,
            owner: agent,
            url: "about:blank".into(),
            title: String::new(),
            tabs: Vec::new(),
            shown: None,
            following: true,
            held: false,
            driver: None,
            dialog: None,
            select: None,
            sized: None,
        };
        browsers.open.insert(pane, Open { owner: agent, browser, started, page: page.clone(), frame: None });
        self.pushes.browser(&browsers, BrowserNews::Page(Box::new(page)));
        drop(browsers);
        self.broadcast_tree();
        tokio::spawn(heed(self.clone(), pane, started, heard));
        Ok(pane)
    }

    /// The browser pane an agent owns, if it has one open.
    pub(crate) fn browser_of(&self, agent: PaneId) -> Option<PaneId> {
        let browsers = self.browsers.lock().unwrap();
        browsers.open.iter().find(|(_, open)| open.owner == agent).map(|(pane, _)| *pane)
    }

    /// A line to the agent's browser for its browser tool, if it has one
    /// open.
    pub(crate) fn attach_browser(&self, agent: PaneId) -> Option<(crate::browser::mux::Line, mpsc::UnboundedReceiver<String>)> {
        let browsers = self.browsers.lock().unwrap();
        browsers.open.values().find(|open| open.owner == agent).map(|open| open.browser.attach())
    }

    /// The person's hand on a browser pane; one the daemon does not have is
    /// ignored, as it has gone.
    pub fn browser_input(&self, pane: PaneId, input: BrowserInput) {
        if let Some(open) = self.browsers.lock().unwrap().open.get(&pane) {
            open.browser.input(input);
        }
    }

    /// The person's close, or the agent's: the pane leaves the tree, and
    /// its browser is killed as it goes. Only a browser's pane: a client
    /// naming another kind is not closing a browser.
    pub fn close_browser(&self, pane: PaneId) {
        if !self.browsers.lock().unwrap().open.contains_key(&pane) {
            return;
        }
        // A pane the tree has already lost still has its browser to end.
        if self.close_pane(pane).is_err() {
            self.kill_browser(pane);
        }
    }

    /// Called as a pane leaves the tree: a browser pane's browser is
    /// killed, and an agent's pane takes its browser pane with it.
    ///
    /// Without `inner` held, as `retire_pane` is. The owned browser is
    /// killed, then its pane taken out plainly, not through `close_pane`:
    /// this is reached from `remove_pane` too, when an agent is replaced in
    /// place and its checkout briefly has none, and `close_pane` would then
    /// uninstall the checkout's managed hooks and stamp it as used.
    pub(super) fn drop_browser_of(&self, pane: PaneId) {
        self.kill_browser(pane);
        let owned: Vec<PaneId> = {
            let browsers = self.browsers.lock().unwrap();
            browsers.open.iter().filter(|(_, open)| open.owner == pane).map(|(id, _)| *id).collect()
        };
        if owned.is_empty() {
            return;
        }
        for browser in owned {
            self.kill_browser(browser);
            let removed = {
                let mut inner = self.inner.lock().unwrap();
                remove_pane_with_checkout(&mut inner.projects, browser).map(|(p, _)| p)
            };
            if let Some(removed) = removed {
                self.retire_pane(removed);
            }
        }
        self.broadcast_tree();
    }

    /// The browser of a browser pane ended and told of it, with its pane
    /// left alone.
    fn kill_browser(&self, pane: PaneId) {
        let mut browsers = self.browsers.lock().unwrap();
        if browsers.open.remove(&pane).is_some() {
            self.pushes.browser(&browsers, BrowserNews::Closed { pane });
        }
    }

    /// Every browser and its pane's, as the daemon stops: on Windows a job
    /// would end each anyway, but elsewhere one would outlive it. The tree
    /// is going with the daemon, so the panes are left in it.
    pub fn close_browsers(&self) {
        let panes: Vec<PaneId> = self.browsers.lock().unwrap().open.keys().copied().collect();
        for pane in panes {
            self.kill_browser(pane);
        }
    }

    /// Every browser as it stands — where it is, then its latest picture —
    /// and the receiver of what follows.
    pub fn watch_browsers(&self) -> (Vec<BrowserNews>, broadcast::Receiver<BrowserNews>) {
        let browsers = self.browsers.lock().unwrap();
        let mut now = Vec::new();
        for open in browsers.open.values() {
            now.push(BrowserNews::Page(Box::new(open.page.clone())));
            now.extend(open.frame.clone().map(BrowserNews::Frame));
        }
        (now, self.pushes.subscribe_browsers())
    }

    /// What a browser said of itself, kept and told to the watchers.
    /// `false` once it is no longer this start's browser to speak for.
    fn browser_said(&self, pane: PaneId, started: u64, report: Report) -> bool {
        let mut browsers = self.browsers.lock().unwrap();
        let Some(open) = browsers.open.get_mut(&pane).filter(|open| open.started == started) else {
            return false;
        };
        let owner = open.owner;
        let mut titled = None;
        let news = match report {
            Report::Page(seen) => {
                let on = seen.shown.as_deref().and_then(|id| seen.tabs.iter().find(|tab| tab.id == id));
                let (url, title) = on.map(|tab| (tab.url.clone(), tab.title.clone())).unwrap_or_default();
                titled = Some(if title.is_empty() { UNTITLED.to_string() } else { title.clone() });
                let Seen { tabs, shown, following, held, driver, dialog, select, sized } = seen;
                // With no tab shown there is no page: the last picture is of
                // one that closed, and a client that joins is not sent it.
                if shown.is_none() {
                    open.frame = None;
                }
                open.page = BrowserPage { pane, owner, url, title, tabs, shown, following, held, driver, dialog, select, sized };
                BrowserNews::Page(Box::new(open.page.clone()))
            }
            Report::Frame { jpeg, width, height } => {
                let frame = Arc::new(BrowserFrame { pane, jpeg, width, height });
                open.frame = Some(frame.clone());
                BrowserNews::Frame(frame)
            }
            Report::Cursor(cursor) => BrowserNews::Cursor { pane, cursor },
        };
        self.pushes.browser(&browsers, news);
        // The pane is named for the page it shows, so a tile or a row says
        // which browser it is. The tree is sent only when the name moved:
        // pages are reported as often as a tab loads.
        let renamed = titled.is_some_and(|title| {
            let mut inner = self.inner.lock().unwrap();
            match find_pane(&mut inner.projects, pane) {
                Some(p) if p.title != title => {
                    p.title = title;
                    true
                }
                _ => false,
            }
        });
        drop(browsers);
        if renamed {
            self.broadcast_tree();
        }
        true
    }

    /// The browser ended on its own: its pane goes as a close takes it.
    fn browser_ended(&self, pane: PaneId, started: u64) {
        {
            let mut browsers = self.browsers.lock().unwrap();
            if !browsers.open.get(&pane).is_some_and(|open| open.started == started) {
                return;
            }
            browsers.open.remove(&pane);
            self.pushes.browser(&browsers, BrowserNews::Closed { pane });
        }
        let _ = self.close_pane(pane);
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
