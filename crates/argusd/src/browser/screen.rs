//! Which of a browser's pages is shown, and showing it: its pictures read
//! off as the page changes, and the person's input put on it.
//!
//! The newest page is the one shown, since an agent that opens a tab is
//! about to work in it; when it closes, the newest left is. The agent's
//! own connection is untouched: this one attaches beside it.

use argus_protocol::BrowserInput;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::cdp::{Cdp, Event};
use super::input::commands;
use super::Report;

/// The pictures asked for: the window's size at most, and a quality that
/// keeps one to a few tens of kilobytes.
const SCREENCAST: fn() -> Value =
    || json!({ "format": "jpeg", "quality": 60, "maxWidth": 1280, "maxHeight": 800, "everyNthFrame": 1 });

#[derive(Debug, Clone, PartialEq)]
struct Page {
    target: String,
    url: String,
    title: String,
}

/// The browser's pages in the order they opened.
#[derive(Debug, Default)]
struct Pages(Vec<Page>);

impl Pages {
    /// What a target event says of the pages; anything not a page, such as
    /// a worker, is not one to show.
    fn heard(&mut self, event: &Event) {
        match event.method.as_str() {
            "Target.targetCreated" | "Target.targetInfoChanged" => {
                let info = &event.params["targetInfo"];
                if info["type"] != "page" {
                    return;
                }
                let text = |key: &str| info[key].as_str().unwrap_or_default().to_string();
                let page = Page { target: text("targetId"), url: text("url"), title: text("title") };
                match self.0.iter_mut().find(|held| held.target == page.target) {
                    Some(held) => *held = page,
                    None => self.0.push(page),
                }
            }
            "Target.targetDestroyed" => {
                let target = event.params["targetId"].as_str().unwrap_or_default();
                self.0.retain(|page| page.target != target);
            }
            _ => {}
        }
    }

    fn newest(&self) -> Option<&Page> {
        self.0.last()
    }

    fn get(&self, target: &str) -> Option<&Page> {
        self.0.iter().find(|page| page.target == target)
    }
}

/// The page being shown: its target, and the session this connection
/// holds on it.
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
    // Answered with a created event for every target already open, so the
    // first page is heard like any other.
    if let Err(error) = cdp.call("Target.setDiscoverTargets", json!({ "discover": true }), None).await {
        tracing::warn!("browser: discovering its pages: {error:#}");
        return;
    }
    let mut pages = Pages::default();
    let mut shown: Option<Shown> = None;
    // Nothing shown is not news until something was.
    let mut told: Option<(String, String)> = Some(Default::default());
    loop {
        tokio::select! {
            event = events.recv() => {
                let Some(mut event) = event else { return };
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
                pages.heard(&event);
                let newest = pages.newest().map(|page| page.target.clone());
                if newest != shown.as_ref().map(|s| s.target.clone()) {
                    shown = show(&cdp, shown.take(), newest).await;
                }
                let now = shown.as_ref().and_then(|s| pages.get(&s.target));
                let page = now.map(|p| (p.url.clone(), p.title.clone())).unwrap_or_default();
                if told.as_ref() != Some(&page) {
                    let _ = reports.send(Report::Page { url: page.0.clone(), title: page.1.clone() });
                    told = Some(page);
                }
            }
            hand = input.recv() => {
                let Some(hand) = hand else { return };
                let Some(shown) = &shown else { continue };
                for (method, params) in commands(&hand) {
                    cdp.tell(method, params, Some(&shown.session));
                }
            }
        }
    }
}

/// Stops showing `old` and starts showing `target`; what is shown after,
/// which is nothing when the page could not be attached to — it may have
/// closed in the moment between.
async fn show(cdp: &Cdp, old: Option<Shown>, target: Option<String>) -> Option<Shown> {
    if let Some(old) = old {
        cdp.tell("Target.detachFromTarget", json!({ "sessionId": old.session }), None);
    }
    let target = target?;
    let attached = cdp.call("Target.attachToTarget", json!({ "targetId": target, "flatten": true }), None).await;
    let session = match attached {
        Ok(answer) => answer["sessionId"].as_str()?.to_string(),
        Err(error) => {
            tracing::debug!("browser: attaching to a page: {error:#}");
            return None;
        }
    };
    let _ = cdp.call("Page.enable", json!({}), Some(&session)).await;
    if let Err(error) = cdp.call("Page.startScreencast", SCREENCAST(), Some(&session)).await {
        tracing::debug!("browser: starting its pictures: {error:#}");
    }
    Some(Shown { target, session })
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

    #[test]
    fn shows_the_newest_page_and_falls_back_when_it_closes() {
        let mut pages = Pages::default();
        pages.heard(&target("Target.targetCreated", "a", "page", "about:blank"));
        pages.heard(&target("Target.targetCreated", "w", "service_worker", ""));
        pages.heard(&target("Target.targetCreated", "b", "page", "https://example.com"));
        assert_eq!(pages.newest().unwrap().target, "b");
        pages.heard(&target("Target.targetInfoChanged", "a", "page", "https://a.test"));
        assert_eq!(pages.newest().unwrap().target, "b", "a page that navigates is not a new one");
        pages.heard(&destroyed("b"));
        assert_eq!(pages.newest().unwrap().url, "https://a.test");
        pages.heard(&destroyed("a"));
        assert!(pages.newest().is_none());
    }
}
