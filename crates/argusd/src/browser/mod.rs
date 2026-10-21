//! A browser run for an agent's pane: started, followed while it runs, and
//! ended when its handle is dropped.
//!
//! The agent drives it over DevTools with its own tool; what the daemon
//! reads of it is only what a person is shown, as `Report`s.

mod cdp;
mod chrome;
mod input;
mod screen;

use argus_protocol::{BrowserInput, BrowserTab};
use tokio::sync::mpsc;

/// What a running browser says of itself. The channel closing is its end:
/// the browser exited, or its socket closed.
#[derive(Debug)]
pub(crate) enum Report {
    /// Its tabs, and the one shown, by id.
    Page { tabs: Vec<BrowserTab>, shown: Option<String> },
    Frame { jpeg: String, width: u32, height: u32 },
}

/// A running browser. Dropping it kills the browser.
pub(crate) struct Browser {
    /// The DevTools URL an agent attaches to.
    pub(crate) endpoint: String,
    input: mpsc::UnboundedSender<BrowserInput>,
    task: tokio::task::JoinHandle<()>,
}

impl Browser {
    /// Starts a browser, its reports sent to `reports` until it ends.
    pub(crate) async fn start(reports: mpsc::UnboundedSender<Report>) -> anyhow::Result<Browser> {
        let chrome = chrome::start().await?;
        let (cdp, events) = cdp::connect(&chrome.endpoint).await?;
        let (input, hands) = mpsc::unbounded_channel();
        let endpoint = chrome.endpoint.clone();
        let task = tokio::spawn(async move {
            // Held for as long as the browser is followed: it ends with it.
            let _chrome = chrome;
            screen::follow(cdp, events, hands, reports).await;
        });
        Ok(Browser { endpoint, input, task })
    }

    pub(crate) fn input(&self, input: BrowserInput) {
        let _ = self.input.send(input);
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
        assert!(browser.endpoint.starts_with("ws://127.0.0.1:"), "{}", browser.endpoint);

        let frame = next(&mut heard, |r| matches!(r, Report::Frame { .. })).await.expect("no picture");
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
                let page = "<title>Shown</title><p>hello";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                    page.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        browser.input(BrowserInput::Navigate { url: format!("http://{address}/") });
        let titled = next(&mut heard, |r| matches!(r, Report::Page { tabs, .. } if tabs.iter().any(|t| t.title == "Shown"))).await;
        assert!(titled.is_some(), "the page's title was never reported");

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
        let opened = next(&mut heard, |r| matches!(r, Report::Page { tabs, shown } if tabs.len() == 2 && shown.as_ref() == Some(&tabs[1].id))).await;
        let Some(Report::Page { tabs, .. }) = opened else { panic!("a new tab was never shown") };
        browser.input(BrowserInput::ShowTab { id: tabs[0].id.clone() });
        let first = tabs[0].id.clone();
        let picked = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Some(Report::Page { shown, .. }) = heard.recv().await {
                    if shown.as_ref() == Some(&first) {
                        return;
                    }
                }
            }
        });
        assert!(picked.await.is_ok(), "the tab the person picked is shown");

        drop(browser);
        let ended = tokio::time::timeout(Duration::from_secs(10), async { while heard.recv().await.is_some() {} }).await;
        assert!(ended.is_ok(), "dropping the browser ends its reports");
    }
}
