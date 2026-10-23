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

use argus_protocol::{BrowserAct, BrowserButton, BrowserDriver, BrowserInput, BrowserTab};
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::cdp::{Cdp, Event};
use super::input::{browser_command, commands};
use super::act::{self, Reply};
use super::cursor::{self, At};
use super::sizing::Sizing;
use super::widgets::Widgets;
use super::mux::{AGENT_ACTED, HELD, HOLD};
use super::{Report, Seen};

/// How long after the last move of the page its mover is still said to be
/// driving it.
const DRIVING: Duration = Duration::from_secs(3);


/// The browser's tabs in the order they opened, the one the agent works
/// in, the one the person picked, and the blank one it started with.
#[derive(Debug, Default)]
struct Tabs {
    open: Vec<BrowserTab>,
    working: Option<String>,
    picked: Option<String>,
    /// The tab Chrome opens itself, while nothing has taken it anywhere: a
    /// tool that opens a tab of its own leaves it behind, blank, so it is
    /// closed when the first other tab opens.
    starting: Option<String>,
}

fn blank(url: &str) -> bool {
    url.is_empty() || url == "about:blank"
}

impl Tabs {
    /// What a target event says of the tabs; anything not a page, such as
    /// a worker, is not one to show. Answers the starting tab to close when
    /// another has just opened while it is still blank.
    fn heard(&mut self, event: &Event) -> Option<String> {
        match event.method.as_str() {
            "Target.targetCreated" | "Target.targetInfoChanged" => {
                let info = &event.params["targetInfo"];
                if info["type"] != "page" {
                    return None;
                }
                let text = |key: &str| info[key].as_str().unwrap_or_default().to_string();
                let tab = BrowserTab { id: text("targetId"), url: text("url"), title: text("title") };
                if self.starting.as_deref() == Some(&*tab.id) && !blank(&tab.url) {
                    self.starting = None;
                }
                match self.open.iter_mut().find(|held| held.id == tab.id) {
                    Some(held) => *held = tab,
                    None => {
                        let first = self.open.is_empty();
                        if first && blank(&tab.url) {
                            self.starting = Some(tab.id.clone());
                        }
                        self.working = Some(tab.id.clone());
                        self.open.push(tab);
                        if !first {
                            return self.starting.take();
                        }
                    }
                }
            }
            "Target.targetDestroyed" => {
                let id = event.params["targetId"].as_str().unwrap_or_default();
                self.open.retain(|tab| tab.id != id);
                for kept in [&mut self.working, &mut self.picked, &mut self.starting] {
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
        None
    }

    /// Whether the only tab is the blank one Chrome started with, which a
    /// new tab asked for is better made of than opened beside.
    fn only_starting(&self) -> bool {
        self.open.len() == 1 && self.starting.as_deref() == Some(&*self.open[0].id)
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
    mut acts: mpsc::UnboundedReceiver<(BrowserAct, Reply)>,
    shots: std::path::PathBuf,
    reports: mpsc::UnboundedSender<Report>,
) {
    let mut tabs = Tabs::default();
    let mut shown: Option<Shown> = None;
    let mut sizing = Sizing::default();
    let mut held = false;
    let mut driver: Option<(BrowserDriver, Instant)> = None;
    let mut told: Option<Seen> = None;
    let pointer = cursor::follow(cdp.clone(), reports.clone());
    let mut widgets = Widgets::default();
    // A left press that opened a `<select>`'s list rather than reaching the
    // page, whose release is not the page's either.
    let mut pressed_select = false;
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
                if on_shown {
                    widgets.heard(&event);
                }
                if sizing.heard(&event, shown.as_ref().map(|s| &*s.target)) {
                    if let Some(shown) = &shown {
                        sizing.apply(&cdp, &shown.session, &shown.target).await;
                    }
                }
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
                if let Some(starting) = tabs.heard(&event) {
                    cdp.tell("Target.closeTarget", json!({ "targetId": starting }), None);
                }
            }
            // An agent's built-in command, done on this session to the tab
            // shown; one that waits for a page to load waits off this loop,
            // so the pictures keep coming meanwhile.
            asked = acts.recv() => {
                let Some((mut asked, reply)) = asked else { return };
                if let BrowserAct::New { url } = &asked {
                    if tabs.only_starting() {
                        asked = BrowserAct::Go { url: url.clone().unwrap_or_else(|| "about:blank".into()) };
                    }
                }
                if held && asked.drives() {
                    let _ = reply.send(Err(HELD.to_string()));
                } else if let Some(dialog) = widgets.dialog.as_ref().filter(|_| asked != BrowserAct::Tabs) {
                    // The page is stopped until the dialog is answered: a
                    // command on it would wait out its whole time.
                    let _ = reply.send(Err(format!("the page is waiting on a dialog (\"{}\"); the person answers it in the Argus window", dialog.message)));
                } else if asked == BrowserAct::Tabs {
                    let _ = reply.send(Ok(listed(&tabs, shown.as_ref())));
                } else {
                    if asked.drives() {
                        driver = Some((BrowserDriver::Agent, Instant::now()));
                        if let Some(shown) = &shown {
                            tabs.working = Some(shown.target.clone());
                        }
                    }
                    let session = shown.as_ref().map(|s| s.session.clone());
                    tokio::spawn(act::run(cdp.clone(), session, asked, shots.clone(), reply));
                }
            }
            hand = input.recv() => {
                let Some(hand) = hand else { return };
                if hand.drives() {
                    driver = Some((BrowserDriver::Person, Instant::now()));
                }
                // A press can change what is under the pointer as much as a
                // move, so both ask what the cursor is there.
                if let (Some(shown), BrowserInput::Move { x, y, .. } | BrowserInput::Press { x, y, .. } | BrowserInput::Release { x, y, .. }) = (&shown, &hand) {
                    pointer.send_replace(Some(At { session: shown.session.clone(), x: *x, y: *y }));
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
                        if sizing.person((*width, *height, *scale), shown.as_ref().map(|s| &*s.target)) {
                            if let Some(shown) = &shown {
                                sizing.apply(&cdp, &shown.session, &shown.target).await;
                            }
                        }
                    }
                    BrowserInput::FitView => {
                        if let Some(shown) = &shown {
                            if sizing.fit(&cdp, Some(&shown.target)) {
                                sizing.apply(&cdp, &shown.session, &shown.target).await;
                            }
                        }
                    }
                    BrowserInput::Press { x, y, button: BrowserButton::Left, .. } if shown.is_some() => {
                        let session = shown.as_ref().map(|s| s.session.clone()).unwrap_or_default();
                        pressed_select = widgets.press(&cdp, &session, *x, *y).await;
                        if !pressed_select {
                            for (method, params) in commands(&hand) {
                                cdp.tell(method, params, Some(&session));
                            }
                        }
                    }
                    BrowserInput::Release { button: BrowserButton::Left, .. } if pressed_select => pressed_select = false,
                    BrowserInput::Choose { index } => {
                        if let Some(shown) = &shown {
                            widgets.choose(&cdp, &shown.session, *index);
                        }
                    }
                    BrowserInput::Answer { accept, text } => {
                        if let Some(shown) = &shown {
                            widgets.answer(&cdp, &shown.session, *accept, text.as_deref());
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
            widgets.shown_changed();
            shown = show(&cdp, shown.take(), want, &sizing).await;
        }
        let now = Seen {
            tabs: tabs.open.clone(),
            shown: shown.as_ref().map(|s| s.target.clone()),
            following: tabs.picked.is_none(),
            held,
            driver: driver.map(|(who, _)| who),
            dialog: widgets.dialog.clone(),
            select: widgets.select().cloned(),
            sized: sizing.sized(shown.as_ref().map(|s| &*s.target)),
        };
        if told.as_ref() != Some(&now) {
            let _ = reports.send(Report::Page(now.clone()));
            told = Some(now);
        }
    }
}

/// The tabs as an agent reads them: one a line, the shown one marked.
fn listed(tabs: &Tabs, shown: Option<&Shown>) -> String {
    if tabs.open.is_empty() {
        return "no tabs open".into();
    }
    let line = |tab: &BrowserTab| {
        let on = if shown.is_some_and(|s| s.target == tab.id) { "* " } else { "  " };
        let title = if tab.title.is_empty() || tab.title == tab.url { String::new() } else { format!(" — {}", tab.title) };
        format!("{on}{}{title}", tab.url)
    };
    let mut lines: Vec<String> = tabs.open.iter().map(line).collect();
    lines.push("* is the tab shown, which the commands act on".into());
    lines.join("\n")
}

/// Stops showing `old` and starts showing `target`, laid out as `sizing` says;
/// what is shown after, which is nothing when the tab could not be attached
/// to — it may have closed in the moment between.
async fn show(cdp: &Cdp, old: Option<Shown>, target: Option<String>, sizing: &Sizing) -> Option<Shown> {
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
    sizing.apply(cdp, &session, &target).await;
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
    fn the_blank_tab_chrome_starts_with_is_closed_when_another_opens_unless_it_was_used() {
        let mut tabs = Tabs::default();
        assert_eq!(tabs.heard(&target("Target.targetCreated", "start", "page", "about:blank")), None);
        assert!(tabs.only_starting(), "a new tab asked for is made of it");
        assert_eq!(tabs.heard(&target("Target.targetCreated", "w", "service_worker", "")), None, "a worker is no tab");
        assert_eq!(tabs.heard(&target("Target.targetCreated", "agents", "page", "about:blank")), Some("start".into()));
        assert_eq!(tabs.heard(&target("Target.targetCreated", "third", "page", "")), None, "closed once");

        let mut used = Tabs::default();
        used.heard(&target("Target.targetCreated", "start", "page", "about:blank"));
        used.heard(&target("Target.targetInfoChanged", "start", "page", "https://a.test"));
        assert!(!used.only_starting());
        assert_eq!(used.heard(&target("Target.targetCreated", "next", "page", "")), None, "a tab that was used stays");
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
