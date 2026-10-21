//! An agent's browser through the pane API: the devtools socket refuses a
//! request without the token or a browser, and attaches a browser tool to
//! a real browser through it.

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

use super::*;

fn agent(dir: &std::path::Path) -> (Arc<Daemon>, PaneId) {
    let d = daemon_with_running_claude(dir);
    d.start_hook_server().unwrap();
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    (d, pane)
}

/// The first line of the answer to a WebSocket upgrade of `target`.
async fn upgrade(d: &Arc<Daemon>, target: &str) -> String {
    let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let request = format!(
        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut answer = vec![0u8; 512];
    let n = stream.read(&mut answer).await.unwrap();
    String::from_utf8_lossy(&answer[..n]).lines().next().unwrap_or_default().to_string()
}

#[tokio::test]
async fn the_devtools_socket_wants_the_token_and_a_browser_open() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let path = argus_protocol::pane_path(pane, Endpoint::DevTools);

    assert!(upgrade(&d, &path).await.starts_with("HTTP/1.1 401"), "no token, no browser");
    let wrong = format!("{path}?token=not-it");
    assert!(upgrade(&d, &wrong).await.starts_with("HTTP/1.1 401"));
    let right = format!("{path}?token={}", d.hook_token);
    let answer = upgrade(&d, &right).await;
    assert!(answer.starts_with("HTTP/1.1 409"), "a pane with no browser open: {answer}");
    let _ = d.close_pane(pane);
}

/// Needs a Chrome, Chromium or Edge installed, or `ARGUS_BROWSER`:
/// `cargo test -p argusd an_agents_browser_tool -- --ignored`.
#[tokio::test]
#[ignore]
async fn an_agents_browser_tool_drives_its_panes_browser_through_the_pane_api() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let response = post_agent_hook(&d, pane, Endpoint::Browser, r#""open""#).await;
    let response = String::from_utf8_lossy(&response).to_string();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let url = response.split("\r\n\r\n").nth(1).unwrap().trim().to_string();
    assert!(url.starts_with("ws://127.0.0.1:") && url.contains("/devtools?token="), "{url}");

    let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let (mut socket, _) = tokio_tungstenite::client_async(url.as_str(), stream).await.unwrap();
    let ask = serde_json::json!({ "id": 42, "method": "Target.getTargets" }).to_string();
    socket.send(Message::text(ask)).await.unwrap();
    let answer = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while let Some(Ok(message)) = socket.next().await {
            let value: serde_json::Value = serde_json::from_str(message.to_text().unwrap_or("{}")).unwrap_or_default();
            if value["id"] == 42 {
                return value;
            }
        }
        panic!("the socket closed");
    })
    .await
    .expect("no answer");
    let pages = answer["result"]["targetInfos"].as_array().unwrap().iter().filter(|t| t["type"] == "page").count();
    assert!(pages >= 1, "{answer}");

    // Closing the pane closes its browser, and the socket with it.
    let _ = d.close_pane(pane);
    let closed = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while let Some(Ok(message)) = socket.next().await {
            if message.is_close() {
                return;
            }
        }
    });
    assert!(closed.await.is_ok(), "the socket outlived the browser");
}
