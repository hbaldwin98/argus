//! The proxy a live Claude pane's API traffic goes through: what reaches
//! the API, what comes back, and the draft read off the way.

use std::time::Duration;

use argus_protocol::{Draft, Update};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};

use super::*;

const SSE: &str = concat!(
    "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\n",
    "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Build \"}}\n\n",
    "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"fixed.\"}}\n\n",
    "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
);

/// What reached the fake API: the path and the headers that matter.
#[derive(Debug, Clone)]
struct Seen {
    path: String,
    authorization: Option<String>,
    accept_encoding: Option<String>,
}

/// An API that streams `SSE` back to every request, and reports each one.
async fn fake_api() -> (String, tokio::sync::mpsc::UnboundedReceiver<Seen>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let tx = tx.clone();
            tokio::spawn(async move {
                let service = hyper::service::service_fn(move |request: hyper::Request<hyper::body::Incoming>| {
                    let header = |name: &str| {
                        request.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string)
                    };
                    let _ = tx.send(Seen {
                        path: request.uri().path_and_query().map(|p| p.to_string()).unwrap_or_default(),
                        authorization: header("authorization"),
                        accept_encoding: header("accept-encoding"),
                    });
                    async move {
                        Ok::<_, std::convert::Infallible>(
                            hyper::Response::builder()
                                .header("content-type", "text/event-stream")
                                .body(Full::new(Bytes::from_static(SSE.as_bytes())))
                                .unwrap(),
                        )
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });
    (url, rx)
}

/// A daemon with its proxy up and one live Claude pane whose gateway is
/// `api`.
fn live_pane(dir: &std::path::Path, api: &str) -> (Arc<Daemon>, PaneId) {
    let mut config = fake_claude_config(dir);
    config.agents[0].cmd = if cfg!(windows) {
        vec!["cmd".to_string(), "/K".to_string()]
    } else {
        vec!["cat".to_string()]
    };
    config.agents[0].live = true;
    config.agents[0].env.insert("ANTHROPIC_BASE_URL".to_string(), api.to_string());
    let d = Daemon::new(config);
    d.start_tee().unwrap();
    let _web = d.live_reader();
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    (d, pane)
}

/// Sends a request to the proxy as Claude Code would, returning the status
/// and the whole body.
async fn ask(d: &Daemon, path: &str, body: &str) -> (u16, String) {
    let port = d.tee_port.load(std::sync::atomic::Ordering::Relaxed);
    let client = Client::builder(TokioExecutor::new()).build_http::<Full<Bytes>>();
    let request = hyper::Request::post(format!("http://127.0.0.1:{port}{path}"))
        .header("authorization", "Bearer sk-ant-oat-test")
        .header("accept-encoding", "gzip")
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap();
    let reply = client.request(request).await.unwrap();
    let status = reply.status().as_u16();
    let bytes = reply.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

const TURN: &str = r#"{"model":"claude-opus","stream":true,"tools":[{"name":"Bash"}],"messages":[]}"#;

#[tokio::test]
async fn a_live_panes_reply_comes_back_untouched_and_its_text_is_drafted() {
    let dir = tempfile::tempdir().unwrap();
    let (api, mut seen) = fake_api().await;
    let (d, pane) = live_pane(dir.path(), &api);
    let mut watching = d.watch_transcript(pane);
    let path = format!("/claude/{}/{}/v1/messages?beta=true", pane.0, d.hook_token);

    let (status, body) = ask(&d, &path, TURN).await;

    assert_eq!(status, 200);
    assert_eq!(body, SSE, "the pane gets the API's reply byte for byte");
    let seen = seen.recv().await.unwrap();
    assert_eq!(seen.path, "/v1/messages?beta=true");
    assert_eq!(seen.authorization.as_deref(), Some("Bearer sk-ant-oat-test"), "the login goes through");
    assert_eq!(seen.accept_encoding.as_deref(), Some("identity"), "asked uncompressed, to be read");

    let mut drafts = Vec::new();
    while !drafts.contains(&Draft::Done) {
        let msg = tokio::time::timeout(Duration::from_secs(5), watching.recv()).await.unwrap().unwrap();
        if let ServerMsg::Transcript { updates, .. } = msg {
            drafts.extend(updates.into_iter().filter_map(|u| match u {
                Update::Draft(d) => Some(d),
                _ => None,
            }));
        }
    }
    let text: String = drafts
        .iter()
        .filter_map(|d| match d {
            Draft::More { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(drafts.first(), Some(&Draft::Start { thinking: false }));
    assert_eq!(text, "Build fixed.");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn nothing_but_a_panes_own_path_with_the_token_goes_through() {
    let dir = tempfile::tempdir().unwrap();
    let (api, mut seen) = fake_api().await;
    let (d, pane) = live_pane(dir.path(), &api);
    let (status, _) = ask(&d, &format!("/claude/{}/wrong/v1/messages", pane.0), TURN).await;
    assert_eq!(status, 403);
    let (status, _) = ask(&d, &format!("/claude/999/{}/v1/messages", d.hook_token), TURN).await;
    assert_eq!(status, 410, "no such pane, so nowhere to send it");
    let (status, _) = ask(&d, "/v1/messages", TURN).await;
    assert_eq!(status, 404);
    assert!(seen.try_recv().is_err(), "none of those reached the API");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_small_call_of_claudes_own_passes_through_undrafted() {
    let dir = tempfile::tempdir().unwrap();
    let (api, _seen) = fake_api().await;
    let (d, pane) = live_pane(dir.path(), &api);
    let mut watching = d.watch_transcript(pane);
    let path = format!("/claude/{}/{}/v1/messages", pane.0, d.hook_token);
    let (status, body) = ask(&d, &path, r#"{"stream":true,"messages":[]}"#).await;
    assert_eq!((status, body.as_str()), (200, SSE));
    let drafted = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            if let Ok(ServerMsg::Transcript { updates, .. }) = watching.recv().await {
                if updates.iter().any(|u| matches!(u, Update::Draft(_))) {
                    return;
                }
            }
        }
    })
    .await;
    assert!(drafted.is_err(), "naming a conversation is not a reply");
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_live_pane_is_pointed_at_the_proxy_and_told_it_is_anthropics_only_when_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_running_claude(dir.path());
    d.start_tee().unwrap();
    let port = d.tee_port.load(std::sync::atomic::Ordering::Relaxed);
    let var = |env: &Vec<(String, String)>, key: &str| {
        env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    };

    let mut env = Vec::new();
    assert!(d.tee_env(PaneId(41), &mut env));
    assert_eq!(
        var(&env, "ANTHROPIC_BASE_URL"),
        Some(format!("http://127.0.0.1:{port}/claude/41/{}", d.hook_token))
    );
    assert_eq!(var(&env, "_CLAUDE_CODE_ASSUME_FIRST_PARTY_BASE_URL").as_deref(), Some("1"));
    assert_eq!(d.tee_upstreams.lock().unwrap()[&PaneId(41)], crate::state::tee::ANTHROPIC_API);

    // A pane that already names its own gateway keeps going there, and is
    // not told that gateway is Anthropic.
    let mut env = vec![("ANTHROPIC_BASE_URL".to_string(), "https://gateway.example".to_string())];
    assert!(d.tee_env(PaneId(42), &mut env));
    assert!(var(&env, "_CLAUDE_CODE_ASSUME_FIRST_PARTY_BASE_URL").is_none());
    assert_eq!(d.tee_upstreams.lock().unwrap()[&PaneId(42)], "https://gateway.example");
    assert_eq!(env.iter().filter(|(k, _)| k == "ANTHROPIC_BASE_URL").count(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn only_a_pane_that_asks_and_starts_while_the_web_server_is_connected_is_proxied() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = fake_claude_config(dir.path());
    config.agents[0].cmd = vec!["sh".into(), "-c".into(), "echo \"base=[$ANTHROPIC_BASE_URL]\"; exec cat".into()];
    config.agents[0].live = true;
    let mut plain = config.agents[0].clone();
    plain.name = "plain".into();
    plain.harness = Some("claude".into());
    plain.live = false;
    config.agents.push(plain);
    let d = Daemon::new(config);
    d.start_tee().unwrap();
    let checkout = only_checkout(&d);
    let port = d.tee_port.load(std::sync::atomic::Ordering::Relaxed);
    let base = |pane: PaneId| {
        let d = d.clone();
        async move {
            for _ in 0..250 {
                let (_, _, cells, _, _, _, _) = d.subscribe_pane(pane).unwrap();
                let shown = cells.iter().map(|r| r.iter().map(|c| c.ch.as_str()).collect::<String>()).collect::<String>();
                if let Some(at) = shown.find("base=[") {
                    if let Some(len) = shown[at..].find(']') {
                        return shown[at + 6..at + len].to_string();
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            panic!("pane {} never said its base URL", pane.0);
        }
    };

    let before = d.spawn_agent(checkout, "claude").unwrap();
    assert_eq!(base(before).await, "", "no web server, no proxy");

    let web = d.live_reader();
    let during = d.spawn_agent(checkout, "claude").unwrap();
    let not_asked = d.spawn_agent(checkout, "plain").unwrap();
    assert_eq!(base(during).await, format!("http://127.0.0.1:{port}/claude/{}/{}", during.0, d.hook_token));
    assert_eq!(base(not_asked).await, "", "a template that did not ask is never proxied");

    drop(web);
    let after = d.spawn_agent(checkout, "claude").unwrap();
    assert_eq!(base(after).await, "", "the web server went");
    for pane in [before, during, not_asked, after] {
        let _ = d.close_pane(pane);
    }
}
