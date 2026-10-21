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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRequest {
    Open,
    Close,
    /// One of the built-in commands, for an agent with no browser tool of
    /// its own.
    Act(BrowserAct),
}

/// A built-in command, done to the tab the person is watching: an agent
/// with no browser tool drives the browser with these rather than writing
/// its own DevTools client, which is how tabs pile up — a script opens one
/// for every page where navigating the one it has would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserAct {
    /// The tab goes to `url`, opening the browser if it is not.
    Go { url: String },
    Back,
    Reload,
    /// A tab of its own, at `url` or blank: only for a second page at once.
    New { url: Option<String> },
    /// The tabs, the one shown marked.
    Tabs,
    /// The page's text, as the person would read it.
    Text,
    /// A real click, at the middle of the first element `selector` matches.
    Click { selector: String },
    /// `text` typed into the first element `selector` matches.
    Type { selector: String, text: String },
    /// A script run in the page, answered with its value as JSON.
    Eval { expression: String },
    /// A picture of the tab as PNG, saved where the answer says.
    Shot,
}

impl BrowserAct {
    /// Whether it moves the page, which is refused while the person holds
    /// the browser, as the same move through the agent's own tool is.
    pub fn drives(&self) -> bool {
        matches!(
            self,
            BrowserAct::Go { .. }
                | BrowserAct::Back
                | BrowserAct::Reload
                | BrowserAct::New { .. }
                | BrowserAct::Click { .. }
                | BrowserAct::Type { .. }
        )
    }
}

/// Where a browser pane's browser is, sent as it opens and whenever a tab
/// opens, closes, changes address or title, or another is shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowserPage {
    /// The browser's own pane, in its owner's checkout: what every other
    /// browser message names.
    pub pane: PaneId,
    /// The agent pane that opened it, and whose pane API it is driven
    /// through. It ends with that pane.
    pub owner: PaneId,
    /// The shown tab's address and title, or nothing with no tab open.
    pub url: String,
    pub title: String,
    /// Every tab, in the order they opened.
    pub tabs: Vec<BrowserTab>,
    /// The tab whose pictures are sent, by its id in `tabs`.
    pub shown: Option<String>,
    /// Whether the tab shown is the one the agent works in, rather than one
    /// the person picked.
    pub following: bool,
    /// Whether the person holds the browser: the agent's input is refused
    /// until they hand it back.
    pub held: bool,
    /// Who moved the page in the last few seconds, if anyone did.
    pub driver: Option<BrowserDriver>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum BrowserDriver {
    Agent,
    Person,
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
    /// The tab to show, by its id, until it closes or the person goes back
    /// to following the agent.
    ShowTab { id: String },
    /// Show the tab the agent works in, as it moves between them.
    FollowAgent,
    /// Take the browser from the agent, or hand it back: while held, the
    /// agent's input and navigation are refused, and it is told why.
    Hold { held: bool },
    NewTab,
    CloseTab { id: String },
    /// The room the person's view of it has, in CSS pixels, and how many
    /// device pixels each is: the page is laid out at that size and its
    /// pictures sent at that resolution, so they are drawn one to one.
    Viewport { width: u32, height: u32, scale: f64 },
}

impl BrowserInput {
    /// Whether this is the person moving the page, which marks them as its
    /// driver; looking at it, sizing or picking a tab is not.
    pub fn drives(&self) -> bool {
        !matches!(
            self,
            BrowserInput::ShowTab { .. } | BrowserInput::FollowAgent | BrowserInput::Hold { .. } | BrowserInput::Viewport { .. }
        )
    }

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

    #[test]
    fn a_built_in_command_moves_the_page_only_when_it_says_so() {
        let go = BrowserRequest::Act(BrowserAct::Go { url: "https://a.test".into() });
        let json = serde_json::to_string(&go).unwrap();
        assert_eq!(json, r#"{"act":{"go":{"url":"https://a.test"}}}"#);
        assert_eq!(serde_json::from_str::<BrowserRequest>(&json).unwrap(), go);
        assert!(BrowserAct::Click { selector: "a".into() }.drives());
        assert!(!BrowserAct::Text.drives() && !BrowserAct::Shot.drives() && !BrowserAct::Tabs.drives());
    }
}
