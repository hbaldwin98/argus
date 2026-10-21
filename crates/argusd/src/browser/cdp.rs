//! One DevTools Protocol connection to a browser: commands sent and their
//! answers matched back by id, and everything else read as an event.
//!
//! Sessions are flat — one socket, each command and event naming the
//! target's session it is for — so the page shown and the browser itself
//! are spoken to over the same connection.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// What the browser said unasked.
#[derive(Debug)]
pub(crate) struct Event {
    pub(crate) method: String,
    pub(crate) params: Value,
    pub(crate) session: Option<String>,
}

/// The sending half. Dropping every copy and the event receiver closes the
/// socket.
#[derive(Clone)]
pub(crate) struct Cdp {
    out: mpsc::UnboundedSender<Message>,
    pending: Pending,
    next: Arc<AtomicU64>,
}

/// Connects to `url`, a `ws://` DevTools endpoint. The receiver ends when
/// the browser closes the socket, which is how its end is noticed.
pub(crate) async fn connect(url: &str) -> anyhow::Result<(Cdp, mpsc::UnboundedReceiver<Event>)> {
    let (socket, _) = tokio_tungstenite::connect_async(url).await.context("connecting to the browser")?;
    let (mut sink, mut stream) = socket.split();
    let (out, mut outgoing) = mpsc::unbounded_channel::<Message>();
    let (events, received) = mpsc::unbounded_channel();
    let pending: Pending = Arc::default();

    tokio::spawn(async move {
        while let Some(message) = outgoing.recv().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });
    let answers = pending.clone();
    tokio::spawn(async move {
        while let Some(Ok(message)) = stream.next().await {
            let Ok(text) = message.to_text() else { continue };
            let Ok(value) = serde_json::from_str::<Value>(text) else { continue };
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
        // Whoever is still waiting hears the socket went rather than
        // waiting for ever.
        answers.lock().unwrap().clear();
    });
    Ok((Cdp { out, pending, next: Arc::new(AtomicU64::new(1)) }, received))
}

impl Cdp {
    fn send(&self, id: u64, method: &str, params: Value, session: Option<&str>) {
        let mut command = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            command["sessionId"] = json!(session);
        }
        let _ = self.out.send(Message::text(command.to_string()));
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
