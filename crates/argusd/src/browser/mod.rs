//! A browser run for an agent's pane: started, followed while it runs, and
//! ended when its handle is dropped.
//!
//! The agent drives it over DevTools with its own tool, attached through
//! the pane API rather than to the browser, which listens on no port; what
//! the daemon reads of it is only what a person is shown, as `Report`s.

mod act;
mod cdp;
mod chrome;
mod cursor;
mod input;
pub(crate) mod mux;
mod pipe;
pub(crate) mod process;
mod screen;
mod sizing;
mod widgets;

use std::time::Duration;

use argus_protocol::{BrowserAct, BrowserContext, BrowserCursor, BrowserDialog, BrowserDriver, BrowserInput, BrowserSelect, BrowserSize, BrowserTab};
use serde_json::json;
use tokio::sync::mpsc;

pub(crate) use chrome::sweep;

/// How long a browser has to answer its first command: a cold start of
/// Chrome on a loaded machine takes a few seconds.
const STARTUP: Duration = Duration::from_secs(20);

/// What a running browser says of itself. The channel closing is its end:
/// the browser exited, or its pipe closed.
#[derive(Debug)]
pub(crate) enum Report {
    Page(Box<Seen>),
    Frame { jpeg: String, width: u32, height: u32 },
    /// The cursor under the person's pointer changed.
    Cursor(BrowserCursor),
}

/// A browser as the person is told of it: its tabs, the one shown, and who
/// has it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Seen {
    pub(crate) tabs: Vec<BrowserTab>,
    pub(crate) shown: Option<String>,
    pub(crate) following: bool,
    pub(crate) held: bool,
    pub(crate) driver: Option<BrowserDriver>,
    pub(crate) dialog: Option<BrowserDialog>,
    pub(crate) select: Option<BrowserSelect>,
    pub(crate) sized: Option<BrowserSize>,
    pub(crate) context: Option<BrowserContext>,
}

/// A running browser. Dropping it kills the browser.
pub(crate) struct Browser {
    mux: mux::Mux,
    input: mpsc::UnboundedSender<BrowserInput>,
    acts: mpsc::UnboundedSender<(BrowserAct, act::Reply)>,
    task: tokio::task::JoinHandle<()>,
}

impl Browser {
    /// Starts a browser, its reports sent to `reports` until it ends; an
    /// error when it does not start, or does not answer.
    pub(crate) async fn start(reports: mpsc::UnboundedSender<Report>) -> anyhow::Result<Browser> {
        let (chrome, pipe) = chrome::start()?;
        let (to, from) = pipe.open();
        let mux = mux::Mux::new(to, from);
        let (line, heard) = mux.join(mux::Who::Screen);
        let (cdp, events) = cdp::over(line, heard);
        // Answered with a created event for every tab already open, so the
        // first tab is heard like any other.
        let discovering = cdp.call("Target.setDiscoverTargets", json!({ "discover": true }), None);
        match tokio::time::timeout(STARTUP, discovering).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => return Err(error.context("the browser closed as it started")),
            Err(_) => anyhow::bail!("the browser did not answer within {}s", STARTUP.as_secs()),
        }
        let (input, hands) = mpsc::unbounded_channel();
        let (acts, asked) = mpsc::unbounded_channel();
        let shots = chrome::shots();
        let task = tokio::spawn(async move {
            // Held for as long as the browser is followed: it ends with it.
            let _chrome = chrome;
            screen::follow(cdp, events, hands, asked, shots, reports).await;
        });
        Ok(Browser { mux, input, acts, task })
    }

    pub(crate) fn input(&self, input: BrowserInput) {
        let _ = self.input.send(input);
    }

    /// Does a built-in command to the tab shown. The answer is awaited after
    /// whatever lock this was reached under is let go.
    pub(crate) fn act(&self, act: BrowserAct) -> tokio::sync::oneshot::Receiver<Result<String, String>> {
        let (reply, answer) = tokio::sync::oneshot::channel();
        let _ = self.acts.send((act, reply));
        answer
    }

    /// A line for an agent's browser tool, and what the browser says to it.
    pub(crate) fn attach(&self) -> (mux::Line, mpsc::UnboundedReceiver<String>) {
        self.mux.join(mux::Who::Agent)
    }
}

#[cfg(test)]
impl Browser {
    /// A browser that is not one: nothing is started and nothing is heard,
    /// so a test of what the daemon does with a browser pane needs no
    /// Chrome.
    pub(crate) fn idle() -> Browser {
        let (to, _) = mpsc::unbounded_channel();
        let (_, from) = mpsc::unbounded_channel();
        let (input, _) = mpsc::unbounded_channel();
        let (acts, _) = mpsc::unbounded_channel();
        Browser { mux: mux::Mux::new(to, from), input, acts, task: tokio::spawn(std::future::pending()) }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests;
