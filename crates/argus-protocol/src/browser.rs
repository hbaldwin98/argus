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
    /// A picture of the whole page as PNG, past what the tab's viewport
    /// shows, saved where the answer says.
    FullShot,
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
    /// A script's dialog the shown tab waits on, until someone answers it.
    /// Defaulted on the wire, so a daemon from before it says none.
    #[serde(default)]
    pub dialog: Option<BrowserDialog>,
    /// The `<select>` the person pressed, whose list the window draws in
    /// place of the one headless Chrome draws nowhere.
    #[serde(default)]
    pub select: Option<BrowserSelect>,
    /// The size an agent laid the shown tab out at, which holds over the
    /// size the person's view has until the agent clears it or the person
    /// fits it to their view again.
    #[serde(default)]
    pub sized: Option<BrowserSize>,
}

/// A page's layout size, in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BrowserSize {
    pub width: u32,
    pub height: u32,
}

/// An `alert`, `confirm`, `prompt` or leave-this-page question the page
/// asked: the page is stopped until it is answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BrowserDialog {
    pub kind: BrowserDialogKind,
    pub message: String,
    /// The text a `prompt` offers to start with.
    pub default_prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum BrowserDialogKind {
    Alert,
    Confirm,
    Prompt,
    BeforeUnload,
}

impl BrowserDialogKind {
    /// The kind DevTools names in `Page.javascriptDialogOpening`.
    pub fn from_devtools(kind: &str) -> BrowserDialogKind {
        match kind {
            "confirm" => BrowserDialogKind::Confirm,
            "prompt" => BrowserDialogKind::Prompt,
            "beforeunload" => BrowserDialogKind::BeforeUnload,
            _ => BrowserDialogKind::Alert,
        }
    }
}

/// A `<select>`'s list, open: its options and where the select sits on the
/// page, in the page's CSS pixels, so the list opens under it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BrowserSelect {
    /// The options a person can see, hidden ones left out.
    pub options: Vec<BrowserOption>,
    /// The option chosen now, by its index among all the select's.
    pub selected: Option<u32>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BrowserOption {
    /// Its index among all the select's options, which choosing it sends.
    pub index: u32,
    pub label: String,
    /// The `<optgroup>` it is in, by its label.
    pub group: Option<String>,
    pub disabled: bool,
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
    /// A press and release at once, for a client that sends no pointer of
    /// its own; one that does sends `Press` and `Release`.
    Click { x: f64, y: f64 },
    /// The pointer over the page, `held` the button down while it moves:
    /// what hovers and drags. Only to a daemon that listed
    /// [`BROWSER_POINTER`](crate::BROWSER_POINTER), as are `Press`,
    /// `Release` and `Leave`.
    Move { x: f64, y: f64, held: Option<BrowserButton> },
    /// `count` is the click's place in a run of them: 2 selects a word.
    Press { x: f64, y: f64, button: BrowserButton, count: u32 },
    Release { x: f64, y: f64, button: BrowserButton, count: u32 },
    /// The pointer went off the page, so what it hovered no longer is.
    Leave,
    /// The person's answer to the dialog the page waits on: OK (or Leave)
    /// is `accept`, and `text` is what a `prompt` was given.
    Answer { accept: bool, text: Option<String> },
    /// The option picked from an open `<select>`'s list, by its index, or
    /// none when the list was put away.
    Choose { index: Option<u32> },
    /// Lay the shown tab out at the person's view again, in place of the
    /// size an agent gave it.
    FitView,
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
        // A pointer passing over the page is not moving it: hover alone
        // does not take the chip from the agent.
        let looking = matches!(self, BrowserInput::Move { held: None, .. } | BrowserInput::Leave | BrowserInput::Choose { index: None });
        !looking
            && !matches!(
                self,
                BrowserInput::ShowTab { .. } | BrowserInput::FollowAgent | BrowserInput::Hold { .. } | BrowserInput::Viewport { .. }
            )
    }

    /// Whether only a daemon that listed
    /// [`BROWSER_POINTER`](crate::BROWSER_POINTER) can read it.
    pub fn pointer(&self) -> bool {
        matches!(
            self,
            BrowserInput::Move { .. }
                | BrowserInput::Press { .. }
                | BrowserInput::Release { .. }
                | BrowserInput::Leave
                | BrowserInput::Answer { .. }
                | BrowserInput::Choose { .. }
                | BrowserInput::FitView
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum BrowserButton {
    Left,
    Middle,
    Right,
}

/// What the pointer looks like over a browser's page: the CSS cursor under
/// it, as one a window has. A cursor no window draws, such as `wait`, `none`
/// or one a page draws from an image, is the arrow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum BrowserCursor {
    #[default]
    Default,
    Pointer,
    Text,
    Crosshair,
    Grab,
    Grabbing,
    NotAllowed,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ColResize,
    RowResize,
}

impl BrowserCursor {
    /// The cursor a CSS `cursor` keyword names.
    pub fn from_css(keyword: &str) -> BrowserCursor {
        match keyword.trim() {
            "pointer" => BrowserCursor::Pointer,
            "text" | "vertical-text" => BrowserCursor::Text,
            "crosshair" | "cell" => BrowserCursor::Crosshair,
            "grab" | "move" | "all-scroll" => BrowserCursor::Grab,
            "grabbing" => BrowserCursor::Grabbing,
            "not-allowed" | "no-drop" => BrowserCursor::NotAllowed,
            "ew-resize" | "e-resize" | "w-resize" => BrowserCursor::EwResize,
            "ns-resize" | "n-resize" | "s-resize" => BrowserCursor::NsResize,
            "nesw-resize" | "ne-resize" | "sw-resize" => BrowserCursor::NeswResize,
            "nwse-resize" | "nw-resize" | "se-resize" => BrowserCursor::NwseResize,
            "col-resize" => BrowserCursor::ColResize,
            "row-resize" => BrowserCursor::RowResize,
            _ => BrowserCursor::Default,
        }
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
        assert!(!BrowserAct::FullShot.drives());
    }

    #[test]
    fn hovering_is_not_driving_but_pressing_and_dragging_are() {
        let left = Some(BrowserButton::Left);
        assert!(!BrowserInput::Move { x: 1.0, y: 1.0, held: None }.drives());
        assert!(!BrowserInput::Leave.drives());
        assert!(BrowserInput::Move { x: 1.0, y: 1.0, held: left }.drives());
        assert!(BrowserInput::Press { x: 1.0, y: 1.0, button: BrowserButton::Left, count: 1 }.drives());
        assert!(BrowserInput::Leave.pointer() && !BrowserInput::Click { x: 1.0, y: 1.0 }.pointer());
    }

    #[test]
    fn a_page_from_a_daemon_before_the_widgets_has_none_open() {
        #[derive(Serialize)]
        struct Older {
            pane: PaneId,
            owner: PaneId,
            url: String,
            title: String,
            tabs: Vec<BrowserTab>,
            shown: Option<String>,
            following: bool,
            held: bool,
            driver: Option<BrowserDriver>,
        }
        let older = Older { pane: PaneId(1), owner: PaneId(2), url: String::new(), title: String::new(), tabs: Vec::new(), shown: None, following: true, held: false, driver: None };
        let page: BrowserPage = rmp_serde::from_slice(&rmp_serde::to_vec_named(&older).unwrap()).unwrap();
        assert_eq!((page.dialog, page.select), (None, None));
    }

    #[test]
    fn a_dialog_is_the_kind_devtools_names_and_an_alert_otherwise() {
        assert_eq!(BrowserDialogKind::from_devtools("confirm"), BrowserDialogKind::Confirm);
        assert_eq!(BrowserDialogKind::from_devtools("beforeunload"), BrowserDialogKind::BeforeUnload);
        assert_eq!(BrowserDialogKind::from_devtools("something new"), BrowserDialogKind::Alert);
    }

    #[test]
    fn a_css_cursor_is_one_a_window_draws_or_the_arrow() {
        assert_eq!(BrowserCursor::from_css("pointer"), BrowserCursor::Pointer);
        assert_eq!(BrowserCursor::from_css(" text"), BrowserCursor::Text);
        assert_eq!(BrowserCursor::from_css("move"), BrowserCursor::Grab);
        assert_eq!(BrowserCursor::from_css("wait"), BrowserCursor::Default);
        assert_eq!(BrowserCursor::from_css("url(a.png)"), BrowserCursor::Default);
    }
}
