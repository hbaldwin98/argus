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

/// Where a pane's browser is, sent as it opens and whenever a tab opens,
/// closes, changes address or title, or another is shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserPage {
    pub pane: PaneId,
    /// The shown tab's address and title, or nothing with no tab open.
    pub url: String,
    pub title: String,
    /// Every tab, in the order they opened.
    pub tabs: Vec<BrowserTab>,
    /// The tab whose pictures are sent, by its id in `tabs`.
    pub shown: Option<String>,
}

/// One of a browser's tabs, by the browser's own id for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BrowserTab {
    pub id: String,
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
    /// The tab to show, by its id; a tab the agent opens after is shown in
    /// its place.
    ShowTab { id: String },
    NewTab,
    CloseTab { id: String },
    /// The room the person's view of it has, in CSS pixels, and how many
    /// device pixels each is: the page is laid out at that size and its
    /// pictures sent at that resolution, so they are drawn one to one.
    Viewport { width: u32, height: u32, scale: f64 },
}

impl BrowserInput {
    /// A viewport the browser can lay a page out at: no smaller than a
    /// phone, no larger than a 4K screen's pixels, whatever a client asks.
    pub fn held_viewport(width: u32, height: u32, scale: f64) -> (u32, u32, f64) {
        let scale = if scale.is_finite() { scale.clamp(1.0, 3.0) } else { 1.0 };
        let width = width.clamp(320, 3840);
        let height = height.clamp(240, 2400);
        // The pictures, not the layout, are what cost: past 4K's pixels
        // they come at a lower scale rather than at a smaller size.
        let most = (3840.0 * 2400.0) / (width as f64 * height as f64);
        (width, height, scale.min(most.sqrt()).max(1.0))
    }
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
    fn a_viewport_is_held_to_what_a_browser_can_lay_out_and_send() {
        assert_eq!(BrowserInput::held_viewport(1200, 800, 2.0), (1200, 800, 2.0));
        assert_eq!(BrowserInput::held_viewport(0, 0, f64::NAN), (320, 240, 1.0));
        assert_eq!(BrowserInput::held_viewport(99_999, 99_999, 9.0), (3840, 2400, 1.0));
        let (w, h, scale) = BrowserInput::held_viewport(2560, 1600, 3.0);
        assert!(w as f64 * h as f64 * scale * scale <= 3840.0 * 2400.0 + 1.0);
    }

    #[test]
    fn an_agent_request_reads_as_a_word() {
        assert_eq!(serde_json::from_str::<BrowserRequest>(r#""open""#).unwrap(), BrowserRequest::Open);
        assert_eq!(serde_json::to_string(&BrowserRequest::Close).unwrap(), r#""close""#);
    }
}
