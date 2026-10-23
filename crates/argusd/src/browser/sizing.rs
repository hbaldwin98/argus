//! What size the shown tab is laid out at, and laying it out: the person's
//! view, unless an agent gave the tab a size of its own.
//!
//! The person's view reports its room whenever its layout shifts, so a size
//! an agent set (to see a page as a phone does, say) would be undone at the
//! next shift if the person's size were simply put back. Instead an agent's
//! size, seen in the mux as it passes, holds for its tab until the agent
//! clears it or the person fits the tab to their view again; meanwhile the
//! pictures are taken at the agent's size and letterboxed in the view. The
//! daemon's own session is given the same metrics the agent's was, so the
//! two never disagree whichever Chrome lets win.

use std::collections::HashMap;

use argus_protocol::{BrowserInput, BrowserSize};
use serde_json::{json, Value};

use super::cdp::{Cdp, Event};

/// The pictures asked for when no one has said how big they look: the
/// window's size the browser starts at.
const UNSIZED: (u32, u32, f64) = (1280, 800, 1.0);
/// Each picture's JPEG quality: high enough that text drawn one to one
/// reads as sharp as the page's own.
const QUALITY: u32 = 85;

/// The size an agent gave a tab: the metrics it sent, and the session it
/// sent them on, which is cleared when the person fits the tab again.
#[derive(Debug, Clone, PartialEq)]
struct AgentSize {
    session: String,
    metrics: Value,
}

impl AgentSize {
    /// The layout and density the metrics ask for, held to what the
    /// pictures can be.
    fn viewport(&self) -> (u32, u32, f64) {
        let number = |key: &str| self.metrics[key].as_f64().unwrap_or_default();
        let scale = if number("deviceScaleFactor") > 0.0 { number("deviceScaleFactor") } else { 1.0 };
        BrowserInput::held_viewport(number("width").round() as u32, number("height").round() as u32, scale)
    }
}

/// How the shown tab is to be laid out.
#[derive(Debug, Clone, PartialEq)]
enum Layout {
    Person((u32, u32, f64)),
    Agent(AgentSize),
    Unsized,
}

#[derive(Debug, Default)]
pub(super) struct Sizing {
    /// The room the person's view last reported.
    person: Option<(u32, u32, f64)>,
    /// The tabs an agent sized, by target.
    agents: HashMap<String, AgentSize>,
}

impl Sizing {
    /// What the mux said an agent did to a tab's size, or a tab that went;
    /// whether it changed how `shown` is laid out.
    pub(super) fn heard(&mut self, event: &Event, shown: Option<&str>) -> bool {
        let target = match event.method.as_str() {
            super::mux::AGENT_SIZED => event.params["targetId"].as_str().unwrap_or_default().to_string(),
            "Target.targetDestroyed" => {
                let target = event.params["targetId"].as_str().unwrap_or_default();
                self.agents.remove(target);
                return false;
            }
            _ => return false,
        };
        let metrics = &event.params["metrics"];
        // A size of 0 keeps the window's own: no size of the agent's.
        let sized = metrics["width"].as_f64().unwrap_or_default() > 0.0 && metrics["height"].as_f64().unwrap_or_default() > 0.0;
        let before = self.agents.get(&target).cloned();
        match event.params["session"].as_str().filter(|_| sized) {
            Some(session) => {
                self.agents.insert(target.clone(), AgentSize { session: session.to_string(), metrics: metrics.clone() });
            }
            None => {
                self.agents.remove(&target);
            }
        }
        shown == Some(&*target) && before != self.agents.get(&target).cloned()
    }

    /// The person's view has this much room now; whether it changes how
    /// `shown` is laid out, which it does not while an agent's size holds.
    pub(super) fn person(&mut self, viewport: (u32, u32, f64), shown: Option<&str>) -> bool {
        let held = BrowserInput::held_viewport(viewport.0, viewport.1, viewport.2);
        if self.person == Some(held) {
            return false;
        }
        self.person = Some(held);
        !shown.is_some_and(|target| self.agents.contains_key(target))
    }

    /// The person fits `shown` to their view again: the agent's size is let
    /// go, and the session it was set on told so; whether there was one.
    pub(super) fn fit(&mut self, cdp: &Cdp, shown: Option<&str>) -> bool {
        let Some(agent) = shown.and_then(|target| self.agents.remove(target)) else { return false };
        cdp.tell("Emulation.clearDeviceMetricsOverride", json!({}), Some(&agent.session));
        true
    }

    /// The agent's size of `shown`, for the person to be told.
    pub(super) fn sized(&self, shown: Option<&str>) -> Option<BrowserSize> {
        let (width, height, _) = self.agents.get(shown?)?.viewport();
        Some(BrowserSize { width, height })
    }

    fn layout(&self, target: &str) -> Layout {
        match (self.agents.get(target), self.person) {
            (Some(agent), _) => Layout::Agent(agent.clone()),
            (None, Some(person)) => Layout::Person(person),
            (None, None) => Layout::Unsized,
        }
    }

    /// Lays `target`, attached on `session`, out as it should be, and asks
    /// for its pictures at that many device pixels, so a view of the same
    /// size draws them one to one.
    pub(super) async fn apply(&self, cdp: &Cdp, session: &str, target: &str) {
        let (metrics, viewport) = match self.layout(target) {
            Layout::Agent(agent) => (Some(agent.metrics.clone()), agent.viewport()),
            Layout::Person(viewport @ (width, height, scale)) => {
                (Some(json!({ "width": width, "height": height, "deviceScaleFactor": scale, "mobile": false })), viewport)
            }
            Layout::Unsized => (None, UNSIZED),
        };
        if let Some(metrics) = metrics {
            if let Err(error) = cdp.call("Emulation.setDeviceMetricsOverride", metrics, Some(session)).await {
                tracing::debug!("browser: sizing a tab: {error:#}");
            }
        }
        let _ = cdp.call("Page.stopScreencast", json!({}), Some(session)).await;
        screencast(cdp, session, viewport).await;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sized(target: &str, session: &str, width: u32, height: u32) -> Event {
        let metrics = json!({ "width": width, "height": height, "deviceScaleFactor": 3, "mobile": true });
        Event { method: super::super::mux::AGENT_SIZED.into(), params: json!({ "targetId": target, "session": session, "metrics": metrics }), session: None }
    }

    #[test]
    fn an_agents_size_holds_over_the_persons_view_until_it_is_cleared() {
        let mut sizing = Sizing::default();
        assert!(sizing.person((1000, 800, 1.25), Some("t")), "the person's room lays out an unsized tab");
        assert!(sizing.heard(&sized("t", "agent", 390, 844), Some("t")));
        assert_eq!(sizing.sized(Some("t")), Some(BrowserSize { width: 390, height: 844 }));
        assert!(matches!(sizing.layout("t"), Layout::Agent(agent) if agent.metrics["mobile"] == true), "the agent's metrics as it sent them");
        assert!(!sizing.person((1100, 700, 1.25), Some("t")), "a shift of the person's view does not undo it");
        assert!(sizing.heard(&sized("t", "agent", 0, 0), Some("t")), "a size of 0 clears it");
        assert_eq!(sizing.layout("t"), Layout::Person((1100, 700, 1.25)), "and the person's latest room is back");
    }

    #[test]
    fn a_size_is_the_tabs_it_was_set_on_and_goes_with_it() {
        let mut sizing = Sizing::default();
        sizing.person((1000, 800, 1.0), None);
        assert!(!sizing.heard(&sized("other", "agent", 390, 844), Some("t")), "another tab's size does not move the shown one");
        assert_eq!(sizing.sized(Some("t")), None);
        assert!(matches!(sizing.layout("other"), Layout::Agent(_)));
        sizing.heard(&Event { method: "Target.targetDestroyed".into(), params: json!({ "targetId": "other" }), session: None }, Some("t"));
        assert_eq!(sizing.layout("other"), Layout::Person((1000, 800, 1.0)));
    }
}
