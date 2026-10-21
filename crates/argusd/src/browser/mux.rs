//! One DevTools connection to a browser, shared by the daemon's own view of
//! it and the agents' tools attached to it: each one's command ids
//! rewritten so answers find their way back, events sent to whoever can use
//! them, and what an agent may not do while the person holds the browser
//! refused.
//!
//! A browser on a pipe has one connection, so this is what lets several
//! clients use it at once. Being in the middle is also how the daemon knows
//! which tab an agent works in and whether it is driving, which it tells
//! the daemon's own view as `Argus.agentActed` events no browser sends.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::sync::mpsc;

/// Who a client is: the daemon's own view, or an agent's tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Who {
    Screen,
    Agent,
}

/// The event the daemon's view is sent when an agent acts on a tab:
/// `targetId`, and `drives`, whether what it did moves the page.
pub(crate) const AGENT_ACTED: &str = "Argus.agentActed";
/// The command the daemon's view sends to say whether the person holds the
/// browser: `held`. Answered here, never sent to the browser.
pub(crate) const HOLD: &str = "Argus.hold";

/// What an agent is refused while the person holds the browser: the input
/// and navigation that would move the page from under them. Reading it —
/// a snapshot, a script that looks — is still the agent's.
fn drives(method: &str) -> bool {
    method.starts_with("Input.")
        || matches!(
            method,
            "Page.navigate" | "Page.reload" | "Page.navigateToHistoryEntry" | "Target.createTarget" | "Target.closeTarget"
        )
}

pub(crate) const HELD: &str = "The person has taken control of this browser; wait until they hand it back.";

struct Client {
    who: Who,
    to: mpsc::UnboundedSender<String>,
}

/// A command in flight: who sent it, the id they gave it, and the tab it
/// attaches to, so the session it answers with is known as theirs.
struct Asked {
    client: u64,
    id: Value,
    attaching: Option<String>,
}

/// A session: the client it is, if known, and its tab.
struct Session {
    owner: Option<u64>,
    target: String,
}

#[derive(Default)]
struct State {
    clients: HashMap<u64, Client>,
    next_client: u64,
    next_id: u64,
    asked: HashMap<u64, Asked>,
    sessions: HashMap<String, Session>,
    held: bool,
}

struct Shared {
    state: Mutex<State>,
    browser: mpsc::UnboundedSender<String>,
}

/// The connection, to join.
#[derive(Clone)]
pub(crate) struct Mux(Arc<Shared>);

/// One client's way to the browser. Dropped, the client leaves.
pub(crate) struct Line {
    shared: Arc<Shared>,
    client: u64,
}

impl Mux {
    /// Shares the connection `browser` sends to and `heard` receives from;
    /// every line ends when it does.
    pub(crate) fn new(browser: mpsc::UnboundedSender<String>, mut heard: mpsc::UnboundedReceiver<String>) -> Mux {
        let shared = Arc::new(Shared { state: Mutex::new(State::default()), browser });
        let reading = Arc::downgrade(&shared);
        tokio::spawn(async move {
            while let Some(message) = heard.recv().await {
                let Some(shared) = reading.upgrade() else { return };
                shared.state.lock().unwrap().browser_said(&message);
            }
            // The browser is gone: every client hears its line close.
            if let Some(shared) = reading.upgrade() {
                shared.state.lock().unwrap().clients.clear();
            }
        });
        Mux(shared)
    }

    /// A new client: its line, and what the browser says to it.
    pub(crate) fn join(&self, who: Who) -> (Line, mpsc::UnboundedReceiver<String>) {
        let (to, received) = mpsc::unbounded_channel();
        let mut state = self.0.state.lock().unwrap();
        state.next_client += 1;
        let client = state.next_client;
        state.clients.insert(client, Client { who, to });
        (Line { shared: self.0.clone(), client }, received)
    }
}

impl Line {
    /// A message for the browser, from this line's client.
    pub(crate) fn send(&self, message: &str) {
        let Ok(mut message) = serde_json::from_str::<Value>(message) else { return };
        let mut state = self.shared.state.lock().unwrap();
        if let Some(sent) = state.client_said(self.client, &mut message) {
            let _ = self.shared.browser.send(sent);
        }
    }
}

impl Drop for Line {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.clients.remove(&self.client);
        state.asked.retain(|_, asked| asked.client != self.client);
        state.sessions.retain(|_, session| session.owner != Some(self.client));
    }
}

impl State {
    fn tell(&self, client: u64, message: Value) {
        if let Some(c) = self.clients.get(&client) {
            let _ = c.to.send(message.to_string());
        }
    }

    fn tell_all(&self, who: Option<Who>, message: &str) {
        for c in self.clients.values().filter(|c| who.is_none_or(|who| c.who == who)) {
            let _ = c.to.send(message.to_string());
        }
    }

    /// A client's command: answered here when it is the daemon's own or
    /// refused, else given an id of the connection's and passed on.
    fn client_said(&mut self, client: u64, message: &mut Value) -> Option<String> {
        let who = self.clients.get(&client)?.who;
        let id = message.get("id").cloned()?;
        let method = message["method"].as_str().unwrap_or_default().to_string();
        if who == Who::Screen && method == HOLD {
            self.held = message["params"]["held"].as_bool().unwrap_or(false);
            self.tell(client, json!({ "id": id, "result": {} }));
            return None;
        }
        if who == Who::Agent {
            if self.held && drives(&method) {
                self.tell(client, json!({ "id": id, "error": { "code": -32000, "message": HELD } }));
                return None;
            }
            self.agent_acted(message, &method);
        }
        self.next_id += 1;
        let global = self.next_id;
        let attaching = (method == "Target.attachToTarget")
            .then(|| message["params"]["targetId"].as_str().map(str::to_string))
            .flatten();
        self.asked.insert(global, Asked { client, id, attaching });
        message["id"] = json!(global);
        Some(message.to_string())
    }

    /// Tells the daemon's view which tab an agent's command is for, when it
    /// names one: by the session it is sent on, or the tab it activates.
    fn agent_acted(&self, message: &Value, method: &str) {
        let on_session = message["sessionId"].as_str().and_then(|s| self.sessions.get(s)).map(|s| s.target.clone());
        let activated = (method == "Target.activateTarget")
            .then(|| message["params"]["targetId"].as_str().map(str::to_string))
            .flatten();
        let Some(target) = on_session.or(activated) else { return };
        let event = json!({ "method": AGENT_ACTED, "params": { "targetId": target, "drives": drives(method) } });
        self.tell_all(Some(Who::Screen), &event.to_string());
    }

    /// What the browser said: an answer goes back to whoever asked, under
    /// the id they gave it; an event on a session to that session's client,
    /// or to every agent when it is not known whose; any other to everyone.
    fn browser_said(&mut self, text: &str) {
        let Ok(mut message) = serde_json::from_str::<Value>(text) else { return };
        if let Some(global) = message.get("id").and_then(Value::as_u64) {
            let Some(asked) = self.asked.remove(&global) else { return };
            let session = message["result"]["sessionId"].as_str().map(str::to_string);
            if let (Some(target), Some(session)) = (asked.attaching.clone(), session) {
                self.sessions.insert(session, Session { owner: Some(asked.client), target });
            }
            message["id"] = asked.id;
            self.tell(asked.client, message);
            return;
        }
        let params = &message["params"];
        match message["method"].as_str() {
            Some("Target.attachedToTarget") => {
                if let (Some(session), Some(target)) = (params["sessionId"].as_str(), params["targetInfo"]["targetId"].as_str()) {
                    // Attached on a session of a client's: that client's;
                    // attached by the browser itself: not known whose.
                    let owner = message["sessionId"].as_str().and_then(|s| self.sessions.get(s)).and_then(|s| s.owner);
                    self.sessions.entry(session.to_string()).or_insert(Session { owner, target: target.to_string() });
                }
            }
            Some("Target.detachedFromTarget") => {
                if let Some(session) = params["sessionId"].as_str() {
                    self.sessions.remove(session);
                }
            }
            _ => {}
        }
        match message["sessionId"].as_str() {
            Some(session) => match self.sessions.get(session).and_then(|s| s.owner) {
                Some(owner) => self.tell(owner, message),
                None => self.tell_all(Some(Who::Agent), text),
            },
            None => self.tell_all(None, text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mux over a browser this test plays: what the mux sends it, and a
    /// way to answer.
    fn mux() -> (Mux, mpsc::UnboundedReceiver<String>, mpsc::UnboundedSender<String>) {
        let (to_browser, browser_hears) = mpsc::unbounded_channel();
        let (browser_says, heard) = mpsc::unbounded_channel();
        (Mux::new(to_browser, heard), browser_hears, browser_says)
    }

    fn parsed(text: Option<String>) -> Value {
        serde_json::from_str(&text.expect("nothing came")).unwrap()
    }

    #[tokio::test]
    async fn each_client_hears_the_answer_to_its_own_command_by_its_own_id() {
        let (mux, mut browser, says) = mux();
        let (screen, mut screen_hears) = mux.join(Who::Screen);
        let (agent, mut agent_hears) = mux.join(Who::Agent);
        screen.send(r#"{"id":1,"method":"Target.getTargets"}"#);
        agent.send(r#"{"id":1,"method":"Browser.getVersion"}"#);
        let (first, second) = (parsed(browser.recv().await), parsed(browser.recv().await));
        assert_ne!(first["id"], second["id"], "two clients' id 1 are two ids to the browser");
        says.send(json!({ "id": second["id"], "result": { "product": "Chrome" } }).to_string()).unwrap();
        says.send(json!({ "id": first["id"], "result": { "targetInfos": [] } }).to_string()).unwrap();
        assert_eq!(parsed(agent_hears.recv().await), json!({ "id": 1, "result": { "product": "Chrome" } }));
        assert_eq!(parsed(screen_hears.recv().await), json!({ "id": 1, "result": { "targetInfos": [] } }));
    }

    #[tokio::test]
    async fn a_sessions_events_go_to_the_client_that_attached_it_and_the_screen_hears_where_the_agent_acts() {
        let (mux, mut browser, says) = mux();
        let (_screen, mut screen_hears) = mux.join(Who::Screen);
        let (agent, mut agent_hears) = mux.join(Who::Agent);
        agent.send(r#"{"id":3,"method":"Target.attachToTarget","params":{"targetId":"T","flatten":true}}"#);
        let attach = parsed(browser.recv().await);
        says.send(json!({ "id": attach["id"], "result": { "sessionId": "S" } }).to_string()).unwrap();
        assert_eq!(parsed(agent_hears.recv().await)["result"]["sessionId"], "S");

        says.send(r#"{"method":"Page.loadEventFired","sessionId":"S","params":{}}"#.into()).unwrap();
        assert_eq!(parsed(agent_hears.recv().await)["method"], "Page.loadEventFired");

        agent.send(r#"{"id":4,"sessionId":"S","method":"Input.dispatchKeyEvent","params":{}}"#);
        let acted = parsed(screen_hears.recv().await);
        assert_eq!(acted["method"], AGENT_ACTED);
        assert_eq!(acted["params"], json!({ "targetId": "T", "drives": true }));
    }

    #[tokio::test]
    async fn while_the_person_holds_the_browser_the_agent_may_look_but_not_move_it() {
        let (mux, mut browser, _says) = mux();
        let (screen, mut screen_hears) = mux.join(Who::Screen);
        let (agent, mut agent_hears) = mux.join(Who::Agent);
        screen.send(&json!({ "id": 1, "method": HOLD, "params": { "held": true } }).to_string());
        assert_eq!(parsed(screen_hears.recv().await)["id"], 1);
        agent.send(r#"{"id":5,"method":"Input.dispatchMouseEvent","params":{}}"#);
        let refused = parsed(agent_hears.recv().await);
        assert_eq!(refused["id"], 5);
        assert_eq!(refused["error"]["message"], HELD);
        agent.send(r#"{"id":6,"method":"Runtime.evaluate","params":{}}"#);
        assert_eq!(parsed(browser.recv().await)["method"], "Runtime.evaluate", "a read goes through");
        agent.send(&json!({ "id": 7, "method": HOLD, "params": { "held": false } }).to_string());
        assert_eq!(parsed(browser.recv().await)["method"], HOLD, "only the daemon's own view holds the browser");
    }
}
