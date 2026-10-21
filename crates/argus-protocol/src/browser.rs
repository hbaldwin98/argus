//! A browser the daemon runs for an agent's pane: what an agent asks of it,
//! what a client is shown of it, and what the person's hands do to it.
//!
//! The agent drives it over the Chrome DevTools Protocol, with whatever
//! tool it brings; Argus only starts it, shows it and hands the person the
//! controls. A client never talks to the browser itself, so a window on
//! another host sees the one on the daemon's.

use serde::{Deserialize, Serialize};

use crate::PaneId;

/// The browser to run, by path, in place of the Chrome or Edge the daemon
/// looks for where each system installs it.
pub const BROWSER_VAR: &str = "ARGUS_BROWSER";

/// What an agent asks of its pane's browser through the pane API. Opening
/// one that is open answers its endpoint again, so an agent that lost the
/// URL asks for it the same way it got it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRequest {
    Open,
    Close,
}

/// Where a pane's browser is, sent as it opens and whenever the page it
/// shows changes address or title.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserPage {
    pub pane: PaneId,
    pub url: String,
    pub title: String,
}

/// One picture of the page a pane's browser shows: a JPEG, as the base64
/// the browser gave it, so neither the daemon nor the window decodes it on
/// the way. `width` and `height` are the page's own CSS pixels, which
/// [`BrowserInput`]'s points are in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserFrame {
    pub pane: PaneId,
    pub jpeg: String,
    pub width: u32,
    pub height: u32,
}

/// The person's hand on a pane's browser. Points are in the CSS pixels of
/// the last [`BrowserFrame`] the client drew.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserInput {
    Click { x: f64, y: f64 },
    Wheel { x: f64, y: f64, dx: f64, dy: f64 },
    /// Text typed into whatever has the page's focus.
    Text { text: String },
    Key { key: BrowserKey },
    /// Only an `http` or `https` address, or `about:blank`; the daemon
    /// refuses any other, so a client cannot open the host's files in it.
    Navigate { url: String },
    Back,
    Forward,
    Reload,
}

/// The keys that do something other than type, each sent as the browser's
/// own key of that name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum BrowserKey {
    Enter,
    Backspace,
    Delete,
    Tab,
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
}

impl BrowserKey {
    /// The DOM `key`, which is also its `code`, and the Windows virtual key
    /// code Chrome needs to act on it rather than only report it.
    pub fn dom(self) -> (&'static str, i64) {
        match self {
            BrowserKey::Enter => ("Enter", 13),
            BrowserKey::Backspace => ("Backspace", 8),
            BrowserKey::Delete => ("Delete", 46),
            BrowserKey::Tab => ("Tab", 9),
            BrowserKey::Escape => ("Escape", 27),
            BrowserKey::ArrowUp => ("ArrowUp", 38),
            BrowserKey::ArrowDown => ("ArrowDown", 40),
            BrowserKey::ArrowLeft => ("ArrowLeft", 37),
            BrowserKey::ArrowRight => ("ArrowRight", 39),
            BrowserKey::Home => ("Home", 36),
            BrowserKey::End => ("End", 35),
            BrowserKey::PageUp => ("PageUp", 33),
            BrowserKey::PageDown => ("PageDown", 34),
        }
    }
}

/// Whether the person may send a browser to `url`: the web, or nothing.
pub fn navigable(url: &str) -> bool {
    url == "about:blank" || crate::is_web_url(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigates_only_to_the_web() {
        assert!(navigable("https://example.com/a"));
        assert!(navigable("http://localhost:3000"));
        assert!(navigable("about:blank"));
        assert!(!navigable("file:///etc/passwd"));
        assert!(!navigable("javascript:alert(1)"));
        assert!(!navigable("chrome://settings"));
    }

    #[test]
    fn an_agent_request_reads_as_a_word() {
        assert_eq!(serde_json::from_str::<BrowserRequest>(r#""open""#).unwrap(), BrowserRequest::Open);
        assert_eq!(serde_json::to_string(&BrowserRequest::Close).unwrap(), r#""close""#);
    }
}
