//! A pane's live channel, against an app-server played on a Unix socket:
//! following the thread its hooks named, and speaking back through it.

#![cfg(unix)]

use std::time::Duration;

use argus_protocol::{Body, Sent, Update};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use super::*;

type Server = tokio_tungstenite::WebSocketStream<tokio::net::UnixStream>;

async fn next(server: &mut Server) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), server.next())
            .await
            .expect("a message from Argus")
            .unwrap()
            .unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

async fn tell(server: &mut Server, value: Value) {
    server.send(Message::Text(value.to_string().into())).await.unwrap();
}

/// The conversation as a phone would be sent it, as `kind: text` lines.
fn conversation(d: &Daemon, pane: PaneId) -> Vec<String> {
    let ServerMsg::Transcript { updates, .. } = d.transcript_tail(pane) else { panic!() };
    updates
        .iter()
        .filter_map(|u| match u {
            Update::Upsert(e) => Some(match &e.body {
                Body::Reply { text } => format!("reply: {text}"),
                Body::Question { prompt, answered, .. } => format!("question: {prompt} ({answered:?})"),
                other => format!("{other:?}"),
            }),
            _ => None,
        })
        .collect()
}

async fn eventually(check: impl Fn() -> bool) -> bool {
    for _ in 0..100 {
        if check() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn a_live_pane_follows_its_thread_and_speaks_back_through_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = fake_claude_config(dir.path());
    config.agents[0].cmd = vec!["cat".to_string()];
    let d = Daemon::new(config);
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    d.set_pane_session_id(pane, "thread-1");

    let socket = dir.path().join("codex.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    d.follow_live(pane, socket.clone());
    let (stream, _) = listener.accept().await.unwrap();
    let mut server = tokio_tungstenite::accept_async(stream).await.unwrap();

    let init = next(&mut server).await;
    assert_eq!(init["method"], "initialize");
    tell(&mut server, json!({ "id": init["id"], "result": {} })).await;
    assert_eq!(next(&mut server).await["method"], "initialized");
    let resume = next(&mut server).await;
    assert_eq!(resume["method"], "thread/resume");
    assert_eq!(resume["params"]["threadId"], "thread-1", "the thread the pane's hooks named");
    tell(&mut server, json!({ "id": resume["id"], "result": {} })).await;
    assert!(eventually(|| pane_info(&d, pane).live).await, "the pane is live");

    tell(&mut server, json!({ "method": "turn/started", "params": { "threadId": "thread-1", "turn": { "id": "turn-1" } } })).await;
    tell(&mut server, json!({ "method": "item/started", "params": { "item": { "type": "agentMessage", "id": "msg-1", "text": "" } } })).await;
    for delta in ["Build", " fixed."] {
        tell(&mut server, json!({ "method": "item/agentMessage/delta", "params": { "itemId": "msg-1", "delta": delta, "threadId": "thread-1", "turnId": "turn-1" } })).await;
    }
    tell(&mut server, json!({ "id": 42, "method": "item/commandExecution/requestApproval",
        "params": { "command": "cargo test", "itemId": "i", "threadId": "thread-1", "turnId": "turn-1", "startedAtMs": 0 } })).await;
    assert!(
        eventually(|| conversation(&d, pane) == ["reply: Build fixed.", "question: Run `cargo test`? (None)"]).await,
        "{:?}",
        conversation(&d, pane)
    );

    // Mid-turn, a message steers the turn Codex said is running.
    assert_eq!(d.send_to_agent(pane, "and clippy", true), Sent::Typed);
    let steer = next(&mut server).await;
    assert_eq!(steer["method"], "turn/steer");
    assert_eq!(steer["params"]["expectedTurnId"], "turn-1");
    assert_eq!(steer["params"]["input"][0]["text"], "and clippy");

    // An answer goes back as the response to Codex's request.
    d.answer(pane, "approval:42".into(), "accept".into()).unwrap();
    assert_eq!(next(&mut server).await, json!({ "id": 42, "result": { "decision": "accept" } }));
    assert!(eventually(|| conversation(&d, pane)[1] == "question: Run `cargo test`? (Some(\"accept\"))").await);

    d.interrupt(pane).unwrap();
    let interrupt = next(&mut server).await;
    assert_eq!(interrupt["method"], "turn/interrupt");
    assert_eq!(interrupt["params"]["turnId"], "turn-1");

    // The server going away takes the live channel with it.
    drop(server);
    assert!(eventually(|| !pane_info(&d, pane).live).await);
    let _ = d.close_pane(pane);
}
