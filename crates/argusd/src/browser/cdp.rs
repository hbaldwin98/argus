//! The daemon's own DevTools client of a browser, on a line of its `Mux`:
//! commands sent and their answers matched back by id, and everything else
//! read as an event.
//!
//! Sessions are flat — each command and event names the tab's session it
//! is for — so the tab shown and the browser itself are spoken to over the
//! same line.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use super::mux::Line;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// What the browser said unasked.
#[derive(Debug)]
pub(crate) struct Event {
    pub(crate) method: String,
    pub(crate) params: Value,
    pub(crate) session: Option<String>,
}

/// The sending half. Dropping every copy leaves the mux.
#[derive(Clone)]
pub(crate) struct Cdp {
    line: Arc<Line>,
    pending: Pending,
    next: Arc<AtomicU64>,
}

/// A client on `line`, which hears `heard`. The event receiver ends when
/// the browser does, which is how its end is noticed.
pub(crate) fn over(line: Line, mut heard: mpsc::UnboundedReceiver<String>) -> (Cdp, mpsc::UnboundedReceiver<Event>) {
    let (events, received) = mpsc::unbounded_channel();
    let pending: Pending = Arc::default();
    let answers = pending.clone();
    tokio::spawn(async move {
        while let Some(text) = heard.recv().await {
            let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
            if let Some(id) = value.get("id").and_then(Value::as_u64) {
                // A command sent with `tell` has no one waiting.
                if let Some(waiting) = answers.lock().unwrap().remove(&id) {
                    let answer = match value.get("error") {
                        Some(error) => Err(error.to_string()),
                        None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
                    };
                    let _ = waiting.send(answer);
                }
                continue;
            }
            let Some(method) = value.get("method").and_then(Value::as_str) else { continue };
            let event = Event {
                method: method.to_string(),
                params: value.get("params").cloned().unwrap_or(Value::Null),
                session: value.get("sessionId").and_then(Value::as_str).map(str::to_string),
            };
            if events.send(event).is_err() {
                break;
            }
        }
        // Whoever is still waiting hears the browser went rather than
        // waiting for ever.
        answers.lock().unwrap().clear();
    });
    (Cdp { line: Arc::new(line), pending, next: Arc::new(AtomicU64::new(1)) }, received)
}

impl Cdp {
    fn send(&self, id: u64, method: &str, params: Value, session: Option<&str>) {
        let mut command = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            command["sessionId"] = json!(session);
        }
        self.line.send(&command.to_string());
    }

    /// Sends a command and waits for its answer.
    pub(crate) async fn call(&self, method: &str, params: Value, session: Option<&str>) -> anyhow::Result<Value> {
        let (tx, rx) = oneshot::channel();
        // Registered before the send, so a quick answer finds it.
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.pending.lock().unwrap().insert(id, tx);
        self.send(id, method, params, session);
        rx.await
            .map_err(|_| anyhow!("the browser closed before answering {method}"))?
            .map_err(|error| anyhow!("{method}: {error}"))
    }

    /// Sends a command no one waits on: an acknowledgement, or the
    /// person's input, whose effect shows in the next picture or not at all.
    pub(crate) fn tell(&self, method: &str, params: Value, session: Option<&str>) {
        self.send(self.next.fetch_add(1, Ordering::Relaxed), method, params, session);
    }
}
