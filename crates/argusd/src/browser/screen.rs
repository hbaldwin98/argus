//! Which of a browser's tabs is shown, and showing it: its pictures read
//! off as the page changes, laid out at the size the person sees it, and
//! the person's input put on it.
//!
//! The tab shown is the one the agent works in — the last it acted on, or
//! failing that the last opened, since an agent that opens one is about to
//! work in it — until the person picks another, which is shown until it
//! closes or they go back to following the agent. The agent's own line is
//! untouched: this one attaches beside it, and hears from the `Mux` which
//! tab the agent acts on.

use std::time::Duration;

use argus_protocol::{BrowserDriver, BrowserInput, BrowserTab};
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::cdp::{Cdp, Event};
use super::input::{browser_command, commands};
use super::mux::{AGENT_ACTED, HOLD};
use super::{Report, Seen};

/// How long after the last move of the page its mover is still said to be
/// driving it.
const DRIVING: Duration = Duration::from_secs(3);

/// The pictures asked for when no one has said how big they look: the
/// window's size the browser starts at.
const UNSIZED: (u32, u32, f64) = (1280, 800, 1.0);
/// Each picture's JPEG quality: high enough that text drawn one to one
/// reads as sharp as the page's own.
const QUALITY: u32 = 85;

/// The browser's tabs in the order they opened, the one the agent works
/// in, and the one the person picked.
#[derive(Debug, Default)]
struct Tabs {
    open: Vec<BrowserTab>,
    working: Option<String>,
    picked: Option<String>,
}

impl Tabs {
    /// What a target event says of the tabs; anything not a page, such as
    /// a worker, is not one to show.
    fn heard(&mut self, event: &Event) {
        match event.method.as_str() {
            "Target.targetCreated" | "Target.targetInfoChanged" => {
                let info = &event.params["targetInfo"];
                if info["type"] != "page" {
                    return;
                }
                let text = |key: &str| info[key].as_str().unwrap_or_default().to_string();
                let tab = BrowserTab { id: text("targetId"), url: text("url"), title: text("title") };
                match self.open.iter_mut().find(|held| held.id == tab.id) {
                    Some(held) => *held = tab,
                    None => {
                        self.working = Some(tab.id.clone());
                        self.open.push(tab);
                    }
                }
            }
            "Target.targetDestroyed" => {
                let id = event.params["targetId"].as_str().unwrap_or_default();
                self.open.retain(|tab| tab.id != id);
                for kept in [&mut self.working, &mut self.picked] {
                    if kept.as_deref() == Some(id) {
                        *kept = None;
                    }
                }
            }
            AGENT_ACTED => {
                let id = event.params["targetId"].as_str().unwrap_or_default();
                if self.get(id).is_some() {
                    self.working = Some(id.to_string());
                }
            }
            _ => {}
        }
    }

    /// Picks a tab the browser has; any other is not.
    fn pick(&mut self, id: &str) {
        if self.get(id).is_some() {
            self.picked = Some(id.to_string());
        }
    }

    fn follow_agent(&mut self) {
        self.picked = None;
    }

    fn to_show(&self) -> Option<&BrowserTab> {
        let kept = |id: &Option<String>| id.as_deref().and_then(|id| self.get(id));
        kept(&self.picked).or(kept(&self.working)).or(self.open.last())
    }

    fn get(&self, id: &str) -> Option<&BrowserTab> {
        self.open.iter().find(|tab| tab.id == id)
    }
}

/// The tab being shown: its target, and the session this connection holds
/// on it.
struct Shown {
    target: String,
    session: String,
}

/// Follows the browser until its socket closes or `input` does.
pub(super) async fn follow(
    cdp: Cdp,
    mut events: mpsc::UnboundedReceiver<Event>,
    mut input: mpsc::UnboundedReceiver<BrowserInput>,
    reports: mpsc::UnboundedSender<Report>,
) {
    let mut tabs = Tabs::default();
    let mut shown: Option<Shown> = None;
    let mut viewport: Option<(u32, u32, f64)> = None;
    let mut held = false;
    let mut driver: Option<(BrowserDriver, Instant)> = None;
    let mut told: Option<Seen> = None;
    loop {
        let quiet = driver.map(|(_, at)| at + DRIVING);
        tokio::select! {
            _ = tokio::time::sleep_until(quiet.unwrap_or_else(Instant::now)), if quiet.is_some() => driver = None,
            event = events.recv() => {
                let Some(mut event) = event else { return };
                if event.method == AGENT_ACTED && event.params["drives"].as_bool() == Some(true) {
                    driver = Some((BrowserDriver::Agent, Instant::now()));
                }
                let on_shown = shown.as_ref().is_some_and(|s| event.session.as_deref() == Some(&*s.session));
                match event.method.as_str() {
                    "Page.screencastFrame" if on_shown => {
                        frame(&cdp, &event, &reports);
                        continue;
                    }
                    // The browser reports a page's address as it commits, but
                    // not the title its document sets, so the page is read
                    // again once it has loaded.
                    "Page.loadEventFired" | "Page.navigatedWithinDocument" if on_shown => {
                        let target = shown.as_ref().map(|s| s.target.clone());
                        match cdp.call("Target.getTargetInfo", json!({ "targetId": target }), None).await {
                            Ok(info) => event = Event { method: "Target.targetInfoChanged".into(), params: info, session: None },
                            Err(_) => continue,
                        }
                    }
                    _ => {}
                }
                tabs.heard(&event);
            }
            hand = input.recv() => {
                let Some(hand) = hand else { return };
                if hand.drives() {
                    driver = Some((BrowserDriver::Person, Instant::now()));
                }
                match &hand {
                    BrowserInput::ShowTab { id } => tabs.pick(id),
                    BrowserInput::FollowAgent => tabs.follow_agent(),
                    BrowserInput::Hold { held: now } => {
                        if cdp.call(HOLD, json!({ "held": now }), None).await.is_ok() {
                            held = *now;
                        }
                    }
                    BrowserInput::Viewport { width, height, scale } => {
                        let held = BrowserInput::held_viewport(*width, *height, *scale);
                        if viewport != Some(held) {
                            viewport = Some(held);
                            if let Some(shown) = &shown {
                                size(&cdp, &shown.session, held).await;
                            }
                        }
                    }
                    _ => {
                        if let Some((method, params)) = browser_command(&hand) {
                            cdp.tell(method, params, None);
                        } else if let Some(shown) = &shown {
                            for (method, params) in commands(&hand) {
                                cdp.tell(method, params, Some(&shown.session));
                            }
                        }
                    }
                }
            }
        }
        let want = tabs.to_show().map(|tab| tab.id.clone());
        if want != shown.as_ref().map(|s| s.target.clone()) {
            shown = show(&cdp, shown.take(), want, viewport).await;
        }
        let now = Seen {
            tabs: tabs.open.clone(),
            shown: shown.as_ref().map(|s| s.target.clone()),
            following: tabs.picked.is_none(),
            held,
            driver: driver.map(|(who, _)| who),
        };
        if told.as_ref() != Some(&now) {
            let _ = reports.send(Report::Page(now.clone()));
            told = Some(now);
        }
    }
}

/// Stops showing `old` and starts showing `target`, laid out at `viewport`;
/// what is shown after, which is nothing when the tab could not be attached
/// to — it may have closed in the moment between.
async fn show(cdp: &Cdp, old: Option<Shown>, target: Option<String>, viewport: Option<(u32, u32, f64)>) -> Option<Shown> {
    if let Some(old) = old {
        cdp.tell("Page.stopScreencast", json!({}), Some(&old.session));
        cdp.tell("Target.detachFromTarget", json!({ "sessionId": old.session }), None);
    }
    let target = target?;
    let attached = cdp.call("Target.attachToTarget", json!({ "targetId": target, "flatten": true }), None).await;
    let session = match attached {
        Ok(answer) => answer["sessionId"].as_str()?.to_string(),
        Err(error) => {
            tracing::debug!("browser: attaching to a tab: {error:#}");
            return None;
        }
    };
    let _ = cdp.call("Page.enable", json!({}), Some(&session)).await;
    match viewport {
        Some(viewport) => size(cdp, &session, viewport).await,
        None => screencast(cdp, &session, UNSIZED).await,
    }
    Some(Shown { target, session })
}

/// Lays the shown tab out at `viewport` and asks for its pictures at that
/// many device pixels, so the person's view draws them one to one.
async fn size(cdp: &Cdp, session: &str, viewport: (u32, u32, f64)) {
    let (width, height, scale) = viewport;
    let metrics = json!({ "width": width, "height": height, "deviceScaleFactor": scale, "mobile": false });
    if let Err(error) = cdp.call("Emulation.setDeviceMetricsOverride", metrics, Some(session)).await {
        tracing::debug!("browser: sizing a tab: {error:#}");
    }
    let _ = cdp.call("Page.stopScreencast", json!({}), Some(session)).await;
    screencast(cdp, session, viewport).await;
}

async fn screencast(cdp: &Cdp, session: &str, (width, height, scale): (u32, u32, f64)) {
    let pixels = |css: u32| (css as f64 * scale).round() as u32;
    let asked = json!({
        "format": "jpeg",
        "quality": QUALITY,
        "maxWidth": pixels(width),
        "maxHeight": pixels(height),
        "everyNthFrame": 1,
    });
    if let Err(error) = cdp.call("Page.startScreencast", asked, Some(session)).await {
        tracing::debug!("browser: starting its pictures: {error:#}");
    }
}

/// Passes one picture on, and acknowledges it: the browser sends the next
/// only once the last is, which is what keeps a slow reader from being
/// buried.
fn frame(cdp: &Cdp, event: &Event, reports: &mpsc::UnboundedSender<Report>) {
    let params = &event.params;
    cdp.tell("Page.screencastFrameAck", json!({ "sessionId": params["sessionId"] }), event.session.as_deref());
    let Some(jpeg) = params["data"].as_str() else { return };
    let size = |key: &str| params["metadata"][key].as_f64().unwrap_or_default().round() as u32;
    let _ = reports.send(Report::Frame {
        jpeg: jpeg.to_string(),
        width: size("deviceWidth"),
        height: size("deviceHeight"),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(method: &str, id: &str, kind: &str, url: &str) -> Event {
        Event {
            method: method.to_string(),
            params: json!({ "targetInfo": { "targetId": id, "type": kind, "url": url, "title": "" } }),
            session: None,
        }
    }

    fn destroyed(id: &str) -> Event {
        Event { method: "Target.targetDestroyed".into(), params: json!({ "targetId": id }), session: None }
    }

    fn shown(tabs: &Tabs) -> Option<&str> {
        tabs.to_show().map(|tab| tab.id.as_str())
    }

    #[test]
    fn shows_the_newest_tab_and_falls_back_when_it_closes() {
        let mut tabs = Tabs::default();
        tabs.heard(&target("Target.targetCreated", "a", "page", "about:blank"));
        tabs.heard(&target("Target.targetCreated", "w", "service_worker", ""));
        tabs.heard(&target("Target.targetCreated", "b", "page", "https://example.com"));
        assert_eq!(shown(&tabs), Some("b"));
        assert_eq!(tabs.open.len(), 2, "a worker is not a tab");
        tabs.heard(&target("Target.targetInfoChanged", "a", "page", "https://a.test"));
        assert_eq!(shown(&tabs), Some("b"), "a tab that navigates is not a new one");
        tabs.heard(&destroyed("b"));
        assert_eq!(tabs.to_show().unwrap().url, "https://a.test");
        tabs.heard(&destroyed("a"));
        assert_eq!(shown(&tabs), None);
    }

    fn acted(id: &str) -> Event {
        Event { method: AGENT_ACTED.into(), params: json!({ "targetId": id, "drives": true }), session: None }
    }

    #[test]
    fn the_tab_shown_is_the_one_the_agent_last_acted_on() {
        let mut tabs = Tabs::default();
        tabs.heard(&target("Target.targetCreated", "a", "page", ""));
        tabs.heard(&target("Target.targetCreated", "b", "page", ""));
        assert_eq!(shown(&tabs), Some("b"), "a tab that opens is where the agent is about to work");
        tabs.heard(&acted("a"));
        assert_eq!(shown(&tabs), Some("a"), "and the tab it then acts on is where it works");
        tabs.heard(&acted("gone"));
        assert_eq!(shown(&tabs), Some("a"), "a tab the browser lacks is not followed");
        tabs.heard(&destroyed("a"));
        assert_eq!(shown(&tabs), Some("b"));
    }

    #[test]
    fn a_picked_tab_is_shown_until_it_closes_or_the_person_follows_the_agent_again() {
        let mut tabs = Tabs::default();
        tabs.heard(&target("Target.targetCreated", "a", "page", ""));
        tabs.heard(&target("Target.targetCreated", "b", "page", ""));
        tabs.pick("a");
        assert_eq!(shown(&tabs), Some("a"));
        tabs.pick("gone");
        assert_eq!(shown(&tabs), Some("a"), "a tab the browser lacks is not picked");
        tabs.heard(&target("Target.targetCreated", "c", "page", ""));
        tabs.heard(&acted("b"));
        assert_eq!(shown(&tabs), Some("a"), "the agent moving on does not take the person's tab from them");
        tabs.follow_agent();
        assert_eq!(shown(&tabs), Some("b"));
        tabs.pick("c");
        tabs.heard(&destroyed("c"));
        assert_eq!(shown(&tabs), Some("b"));
    }
}
