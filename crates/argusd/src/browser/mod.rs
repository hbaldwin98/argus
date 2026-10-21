//! A browser run for an agent's pane: started, followed while it runs, and
//! ended when its handle is dropped.
//!
//! The agent drives it over DevTools with its own tool, attached through
//! the pane API rather than to the browser, which listens on no port; what
//! the daemon reads of it is only what a person is shown, as `Report`s.

mod act;
mod cdp;
mod chrome;
mod input;
pub(crate) mod mux;
mod pipe;
pub(crate) mod process;
mod screen;

use std::time::Duration;

use argus_protocol::{BrowserAct, BrowserDriver, BrowserInput, BrowserTab};
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
    Page(Seen),
    Frame { jpeg: String, width: u32, height: u32 },
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
mod tests {
    use super::*;
    use std::time::Duration;

    /// A base64 JPEG's size in pixels, from its start-of-frame marker.
    fn jpeg_size(base64: &str) -> Option<(u32, u32)> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bytes = Vec::new();
        let (mut bits, mut held) = (0u32, 0);
        for c in base64.bytes().filter(|&c| c != b'=') {
            bits = bits << 6 | ALPHABET.iter().position(|&a| a == c)? as u32;
            held += 6;
            if held >= 8 {
                held -= 8;
                bytes.push((bits >> held) as u8);
            }
        }
        let mut at = 2;
        while at + 9 < bytes.len() {
            let (marker, length) = (bytes[at + 1], u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize);
            if matches!(marker, 0xC0..=0xC2) {
                let height = u16::from_be_bytes([bytes[at + 5], bytes[at + 6]]) as u32;
                let width = u16::from_be_bytes([bytes[at + 7], bytes[at + 8]]) as u32;
                return Some((width, height));
            }
            at += 2 + length;
        }
        None
    }

    /// The answer to the agent's command `id`, as the agent sent it.
    async fn answer(told: &mut mpsc::UnboundedReceiver<String>, id: u64) -> serde_json::Value {
        let wait = async {
            loop {
                let text = told.recv().await.expect("the line closed");
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value["id"] == id {
                    return value;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(20), wait).await.expect("no answer")
    }

    /// Whether the browser is said to be as `want` has it within a while.
    async fn seen_until(heard: &mut mpsc::UnboundedReceiver<Report>, want: impl Fn(&Seen) -> bool) -> bool {
        let wait = async {
            loop {
                if let Some(Report::Page(seen)) = heard.recv().await {
                    if want(&seen) {
                        return;
                    }
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(20), wait).await.is_ok()
    }

    async fn next(heard: &mut mpsc::UnboundedReceiver<Report>, want: fn(&Report) -> bool) -> Option<Report> {
        let wait = async {
            loop {
                let report = heard.recv().await.expect("the browser ended");
                if want(&report) {
                    return report;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(20), wait).await.ok()
    }

    /// Needs a Chrome, Chromium or Edge installed, or `ARGUS_BROWSER`:
    /// `cargo test -p argusd shows_a_real_browser -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn shows_a_real_browser_and_takes_the_persons_hand() {
        let (reports, mut heard) = mpsc::unbounded_channel();
        let browser = Browser::start(reports).await.unwrap();

        let frame = next(&mut heard, |r| matches!(r, Report::Frame { .. })).await.expect("no picture");
        // A browser with a debugging port writes it into its profile; one on
        // a pipe has none for another process to reach.
        let profiles = std::env::temp_dir().join("argus-browsers-default");
        let listening = std::fs::read_dir(&profiles).unwrap().flatten().any(|p| p.path().join("DevToolsActivePort").exists());
        assert!(!listening, "the browser opened a debugging port");
        let Report::Frame { jpeg, width, height } = frame else { unreachable!() };
        assert!(jpeg.starts_with("/9j/"), "a JPEG, as base64");
        assert!(width > 0 && height > 0);

        // A server of its own, so the test needs no network.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            while let Ok((mut stream, _)) = listener.accept().await {
                // Read before answering: a socket closed with the request
                // unread is reset, and the browser shows an error page.
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request).await;
                let page = "<title>Shown</title><p>hello</p><input id=q>\
                            <button id=b onclick=\"document.title = document.getElementById('q').value\">go</button>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                    page.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        browser.input(BrowserInput::Navigate { url: format!("http://{address}/") });
        let titled = next(&mut heard, |r| matches!(r, Report::Page(seen) if seen.tabs.iter().any(|t| t.title == "Shown"))).await;
        assert!(titled.is_some(), "the page's title was never reported");

        // The built-in commands, on the tab shown.
        let act = |act: BrowserAct| {
            let answer = browser.act(act);
            async move { tokio::time::timeout(Duration::from_secs(30), answer).await.expect("no answer").expect("dropped") }
        };
        let page = format!("http://{address}/");
        assert!(act(BrowserAct::Go { url: page.clone() }).await.unwrap().ends_with("— Shown"));
        assert!(act(BrowserAct::Text).await.unwrap().contains("hello"));
        assert_eq!(act(BrowserAct::Type { selector: "#q".into(), text: "typed".into() }).await, Ok("typed into #q".into()));
        assert_eq!(act(BrowserAct::Click { selector: "#b".into() }).await, Ok("clicked #b".into()));
        assert_eq!(act(BrowserAct::Eval { expression: "document.title".into() }).await, Ok("\"typed\"".into()), "a real click ran the page's handler");
        assert!(act(BrowserAct::Click { selector: "#nothing".into() }).await.unwrap_err().contains("nothing on the page matches"));
        assert!(act(BrowserAct::Eval { expression: "throw new Error('no')".into() }).await.unwrap_err().contains("no"));
        let shot = act(BrowserAct::Shot).await.unwrap();
        assert!(std::fs::read(&shot).unwrap().starts_with(b"\x89PNG"), "{shot} is a PNG");
        assert!(act(BrowserAct::Tabs).await.unwrap().lines().next().unwrap().starts_with("* "), "the shown tab is marked");
        browser.input(BrowserInput::Hold { held: true });
        assert!(seen_until(&mut heard, |s| s.held).await);
        assert!(act(BrowserAct::Reload).await.unwrap_err().contains("taken control"), "held, the page is the person's");
        assert!(act(BrowserAct::Text).await.is_ok(), "reading it is still the agent's");
        browser.input(BrowserInput::Hold { held: false });
        assert!(seen_until(&mut heard, |s| !s.held).await);

        browser.input(BrowserInput::Viewport { width: 900, height: 600, scale: 2.0 });
        let sized = next(&mut heard, |r| matches!(r, Report::Frame { width: 900, height: 600, .. })).await;
        assert!(sized.is_some(), "the page is laid out at the size the person sees it");
        // The picture taken as the size changes may be part-way there; a
        // reload's are drawn at it.
        let mut sharp = false;
        for _ in 0..5 {
            browser.input(BrowserInput::Reload);
            if let Some(Report::Frame { jpeg, .. }) = next(&mut heard, |r| matches!(r, Report::Frame { .. })).await {
                sharp |= jpeg_size(&jpeg) == Some((1800, 1200));
            }
        }
        assert!(sharp, "its picture has a pixel for each of the screen's");

        browser.input(BrowserInput::NewTab);
        let opened = next(&mut heard, |r| matches!(r, Report::Page(s) if s.tabs.len() == 2 && s.shown.as_ref() == Some(&s.tabs[1].id))).await;
        let Some(Report::Page(seen)) = opened else { panic!("a new tab was never shown") };
        let (first, second) = (seen.tabs[0].id.clone(), seen.tabs[1].id.clone());
        browser.input(BrowserInput::ShowTab { id: first.clone() });
        assert!(seen_until(&mut heard, |s| s.shown.as_ref() == Some(&first) && !s.following).await, "the tab the person picked is shown");

        // An agent's tool, on a line of its own: its ids are its own, and
        // the tab it acts on is the one followed once the person lets it be.
        let (line, mut told) = browser.attach();
        browser.input(BrowserInput::FollowAgent);
        line.send(&serde_json::json!({ "id": 7, "method": "Target.attachToTarget", "params": { "targetId": second, "flatten": true } }).to_string());
        let attached = answer(&mut told, 7).await;
        let session = attached["result"]["sessionId"].as_str().expect("a session for the agent").to_string();
        line.send(&serde_json::json!({ "id": 8, "sessionId": session, "method": "Runtime.evaluate", "params": { "expression": "1 + 1" } }).to_string());
        assert_eq!(answer(&mut told, 8).await["result"]["result"]["value"], 2);
        assert!(seen_until(&mut heard, |s| s.shown.as_ref() == Some(&second)).await, "the tab the agent acts on is followed");

        // Held by the person, the page is theirs: the agent may look, not move.
        browser.input(BrowserInput::Hold { held: true });
        assert!(seen_until(&mut heard, |s| s.held).await);
        let click = serde_json::json!({ "type": "mouseMoved", "x": 1, "y": 1 });
        line.send(&serde_json::json!({ "id": 9, "sessionId": session, "method": "Input.dispatchMouseEvent", "params": click }).to_string());
        let refused = answer(&mut told, 9).await;
        assert!(refused["error"]["message"].as_str().unwrap_or_default().contains("taken control"), "{refused}");
        line.send(&serde_json::json!({ "id": 10, "sessionId": session, "method": "Runtime.evaluate", "params": { "expression": "2" } }).to_string());
        assert_eq!(answer(&mut told, 10).await["result"]["result"]["value"], 2);
        browser.input(BrowserInput::Hold { held: false });
        assert!(seen_until(&mut heard, |s| !s.held).await);
        line.send(&serde_json::json!({ "id": 11, "sessionId": session, "method": "Input.dispatchMouseEvent", "params": click }).to_string());
        assert!(answer(&mut told, 11).await.get("error").is_none());
        assert!(seen_until(&mut heard, |s| s.driver == Some(BrowserDriver::Agent)).await, "an agent moving the page is said to drive it");

        drop(browser);
        let ended = tokio::time::timeout(Duration::from_secs(10), async { while heard.recv().await.is_some() {} }).await;
        assert!(ended.is_ok(), "dropping the browser ends its reports");
    }
}
