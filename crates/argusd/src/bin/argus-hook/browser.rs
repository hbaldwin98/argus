//! `argus-hook browser`: the pane's browser opened, answered with the
//! DevTools URL a browser tool attaches to, or closed. The person watches
//! it in the window while the agent drives it.

use argus_protocol::BrowserRequest;

use super::*;

/// A browser's cold start is seconds, not a report's two.
const OPEN_WAIT: Duration = Duration::from_secs(30);

pub(super) fn browser(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", browser_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

const USAGE: &str = "browser: use `browser` (or `browser open`) for the DevTools URL, or `browser close`";

/// The line that says what to do with the URL, since that is why it was
/// asked for.
const HOW: &str = "attach your browser tool to it, e.g. `agent-browser --cdp <url> open https://example.com`; \
                   the person sees the newest tab in the Argus window";

pub(super) fn browser_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let request = match rest {
        [] | ["open"] => BrowserRequest::Open,
        ["close"] => BrowserRequest::Close,
        _ => return USAGE.to_string(),
    };
    match call_within::<hook::call::Browser>(base_url, token, &request, OPEN_WAIT) {
        Ok(_) if request == BrowserRequest::Close => "browser closed".to_string(),
        Ok(url) => format!("{url}\n{HOW}"),
        Err(refusal) => refusal.said("browser"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::tests::serve_once;

    #[test]
    fn opening_prints_the_url_first_then_how_to_use_it() {
        let (address, server) = serve_once("ws://127.0.0.1:9222/devtools/browser/x");
        let answer = browser_message(&[], &format!("http://{address}/pane/3"), "t");
        let head = server.join().unwrap();
        assert!(head.starts_with("POST /pane/3/browser "), "{head}");
        assert_eq!(answer.lines().next(), Some("ws://127.0.0.1:9222/devtools/browser/x"));
    }

    #[test]
    fn anything_else_is_the_usage() {
        assert_eq!(browser_message(&["launch"], "http://127.0.0.1:1/pane/3", "t"), USAGE);
    }
}
