//! `argus-hook browser`: the pane's browser opened, answered with the
//! DevTools URL a browser tool attaches to, or closed; and, for an agent
//! with no browser tool of its own, the built-in commands that drive the
//! tab the person watches. The person watches it in the window while the
//! agent drives it.

use argus_protocol::{BrowserAct, BrowserRequest};

use super::*;

/// A browser's cold start is seconds, and a page's load as many again.
const BROWSER_WAIT: Duration = Duration::from_secs(45);

pub(super) fn browser(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", browser_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

const USAGE: &str = "browser: `browser` (or `browser open`) for the DevTools URL; `browser close`; \
                     or a built-in command on the tab shown: `go <url>`, `back`, `reload`, `new [url]`, `tabs`, \
                     `text`, `click <selector>`, `type <selector> <text>`, `eval <script>`, `shot`";

/// The line that says what to do with the URL, since that is why it was
/// asked for.
const HOW: &str = "attach your browser tool to it, e.g. `agent-browser --cdp <url> open https://example.com`, \
                   or use the built-in commands (`argus-hook browser go <url>`, `text`, `click`, ...); \
                   the person watches the tab you act in, in the Argus window";

/// What the words after `browser` ask for, or the usage when they ask for
/// nothing it knows. A selector, a script and typed text keep their spaces.
fn request(rest: &[&str]) -> Option<BrowserRequest> {
    let act = |act| Some(BrowserRequest::Act(act));
    match rest {
        [] | ["open"] => Some(BrowserRequest::Open),
        ["close"] => Some(BrowserRequest::Close),
        ["go", url] => act(BrowserAct::Go { url: url.to_string() }),
        ["back"] => act(BrowserAct::Back),
        ["reload"] => act(BrowserAct::Reload),
        ["new"] => act(BrowserAct::New { url: None }),
        ["new", url] => act(BrowserAct::New { url: Some(url.to_string()) }),
        ["tabs"] => act(BrowserAct::Tabs),
        ["text"] => act(BrowserAct::Text),
        ["shot"] => act(BrowserAct::Shot),
        ["click", selector @ ..] if !selector.is_empty() => act(BrowserAct::Click { selector: selector.join(" ") }),
        ["type", selector, text @ ..] if !text.is_empty() => {
            act(BrowserAct::Type { selector: selector.to_string(), text: text.join(" ") })
        }
        ["eval", script @ ..] if !script.is_empty() => act(BrowserAct::Eval { expression: script.join(" ") }),
        _ => None,
    }
}

pub(super) fn browser_message(rest: &[&str], base_url: &str, token: &str) -> String {
    let Some(request) = request(rest) else { return USAGE.to_string() };
    match call_within::<hook::call::Browser>(base_url, token, &request, BROWSER_WAIT) {
        Ok(_) if request == BrowserRequest::Close => "browser closed".to_string(),
        Ok(url) if request == BrowserRequest::Open => format!("{url}\n{HOW}"),
        Ok(answer) => answer,
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
    fn a_built_in_command_prints_what_the_browser_answered() {
        let (address, server) = serve_once("https://a.test/ — A");
        let answer = browser_message(&["go", "https://a.test/"], &format!("http://{address}/pane/3"), "t");
        server.join().unwrap();
        assert_eq!(answer, "https://a.test/ — A");
    }

    #[test]
    fn a_selector_script_or_text_keeps_its_spaces() {
        assert_eq!(
            request(&["click", "button.primary", "span"]),
            Some(BrowserRequest::Act(BrowserAct::Click { selector: "button.primary span".into() }))
        );
        assert_eq!(
            request(&["type", "#q", "rust", "tokio"]),
            Some(BrowserRequest::Act(BrowserAct::Type { selector: "#q".into(), text: "rust tokio".into() }))
        );
        assert_eq!(
            request(&["eval", "1", "+", "1"]),
            Some(BrowserRequest::Act(BrowserAct::Eval { expression: "1 + 1".into() }))
        );
        assert_eq!(request(&["new"]), Some(BrowserRequest::Act(BrowserAct::New { url: None })));
    }

    #[test]
    fn anything_else_is_the_usage() {
        assert_eq!(browser_message(&["launch"], "http://127.0.0.1:1/pane/3", "t"), USAGE);
        assert_eq!(request(&["click"]), None, "a click needs something to click");
        assert_eq!(request(&["type", "#q"]), None, "typing needs text");
    }
}
