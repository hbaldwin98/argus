//! A browser run for an agent's pane: started, followed while it runs, and
//! ended when its handle is dropped.
//!
//! The agent drives it over DevTools with its own tool; what the daemon
//! reads of it is only what a person is shown, as `Report`s.

mod cdp;
mod chrome;
mod input;
mod screen;

use argus_protocol::BrowserInput;
use tokio::sync::mpsc;

/// What a running browser says of itself. The channel closing is its end:
/// the browser exited, or its socket closed.
#[derive(Debug)]
pub(crate) enum Report {
    Page { url: String, title: String },
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
        let titled = next(&mut heard, |r| matches!(r, Report::Page { title, .. } if title == "Shown")).await;
        assert!(titled.is_some(), "the page's title was never reported");

        drop(browser);
        let ended = tokio::time::timeout(Duration::from_secs(10), async { while heard.recv().await.is_some() {} }).await;
        assert!(ended.is_ok(), "dropping the browser ends its reports");
    }
}
