//! An agent's browser: the pane it is in the tree, owned by and closed with
//! its agent, and the pane API's devtools socket, which refuses a request
//! without the token or a browser and attaches a browser tool to a real
//! browser.

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

    assert_eq!(browser_panes(&d).len(), 1, "the real browser is a pane in the tree");

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

// What follows needs no Chrome: a browser that is not one is opened into
// the tree through `open_browser_pane`, the seam `browser_for_agent` ends in.

use crate::browser::{Browser, Report, Seen};
use argus_protocol::BrowserRequest;
use tokio::sync::{broadcast, mpsc};

fn browser_panes(d: &Daemon) -> Vec<argus_protocol::PaneInfo> {
    let tree = d.snapshot();
    let panes = tree.iter().flat_map(|p| &p.repositories).flat_map(|r| &r.checkouts).flat_map(|c| &c.panes);
    panes.filter(|p| p.kind == argus_protocol::PaneKind::Browser).cloned().collect()
}

/// An agent with an idle browser open, and what that browser would report
/// through.
fn with_browser(dir: &std::path::Path) -> (Arc<Daemon>, PaneId, PaneId, mpsc::UnboundedSender<Report>) {
    let (d, agent) = agent(dir);
    let (reports, heard) = mpsc::unbounded_channel();
    let pane = d.open_browser_pane(agent, Browser::idle(), heard).unwrap();
    (d, agent, pane, reports)
}

/// What the watchers have been told since `rx` began.
fn told(rx: &mut broadcast::Receiver<BrowserNews>) -> Vec<BrowserNews> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

#[tokio::test]
async fn an_opened_browser_is_a_pane_in_its_agents_checkout_that_the_agent_owns() {
    let dir = tempfile::tempdir().unwrap();
    let (d, agent) = agent(dir.path());
    let (_, mut rx) = d.watch_browsers();
    let (reports, heard) = mpsc::unbounded_channel();
    let pane = d.open_browser_pane(agent, Browser::idle(), heard).unwrap();
    let _keep = reports;

    let tree = d.snapshot();
    let checkout = &tree[0].repositories[0].checkouts[0];
    let browser = checkout.panes.iter().find(|p| p.id == pane).expect("in the agent's checkout");
    assert_eq!((browser.kind, browser.title.as_str()), (argus_protocol::PaneKind::Browser, "Browser"));
    assert_ne!(pane, agent);
    assert!(checkout.listed_panes().all(|p| p.id != pane), "listed only in the Browsers section");
    let page = told(&mut rx)
        .into_iter()
        .find_map(|news| match news {
            BrowserNews::Page(page) => Some(page),
            _ => None,
        })
        .expect("the browser's page is told");
    assert_eq!((page.pane, page.owner), (pane, agent));
    assert_eq!(d.browser_of(agent), Some(pane));
}

#[tokio::test]
async fn a_second_open_answers_the_same_url_and_makes_no_second_pane() {
    let dir = tempfile::tempdir().unwrap();
    let (d, agent, pane, _reports) = with_browser(dir.path());
    let first = d.browser_for_agent(agent, BrowserRequest::Open).await.unwrap();
    let again = d.browser_for_agent(agent, BrowserRequest::Open).await.unwrap();
    assert_eq!(first, again);
    assert!(first.contains(&format!("/pane/{}/devtools?token=", agent.0)), "the agent's own: {first}");
    assert_eq!(browser_panes(&d).iter().map(|p| p.id).collect::<Vec<_>>(), [pane]);

    let (_, heard) = mpsc::unbounded_channel();
    assert_eq!(d.open_browser_pane(agent, Browser::idle(), heard).unwrap(), pane, "a late second start is dropped");
    assert_eq!(browser_panes(&d).len(), 1);
}

#[tokio::test]
async fn closing_the_browser_pane_closes_the_browser() {
    let dir = tempfile::tempdir().unwrap();
    let (d, agent, pane, _reports) = with_browser(dir.path());
    let (_, mut rx) = d.watch_browsers();
    d.close_pane(pane).unwrap();

    assert!(told(&mut rx).iter().any(|news| matches!(news, BrowserNews::Closed { pane: closed } if *closed == pane)));
    assert!(browser_panes(&d).is_empty());
    assert_eq!(d.browser_of(agent), None);
    assert!(d.attach_browser(agent).is_none());
    assert!(d.snapshot()[0].repositories[0].checkouts[0].panes.iter().any(|p| p.id == agent), "the agent stays");
}

#[tokio::test]
async fn a_client_closing_a_browser_removes_its_pane_and_only_a_browsers() {
    let dir = tempfile::tempdir().unwrap();
    let (d, agent, pane, _reports) = with_browser(dir.path());
    d.close_browser(agent);
    assert_eq!(browser_panes(&d).len(), 1, "an agent's pane is not a browser to close");
    assert!(d.snapshot()[0].repositories[0].checkouts[0].panes.iter().any(|p| p.id == agent));

    d.close_browser(pane);
    assert!(browser_panes(&d).is_empty());
}

#[tokio::test]
async fn an_agents_close_request_removes_its_browser_pane() {
    let dir = tempfile::tempdir().unwrap();
    let (d, agent, _, _reports) = with_browser(dir.path());
    assert_eq!(d.browser_for_agent(agent, BrowserRequest::Close).await.unwrap(), "");
    assert!(browser_panes(&d).is_empty());
    assert_eq!(d.browser_of(agent), None);
}

#[tokio::test]
async fn closing_the_agent_pane_closes_its_browser_pane() {
    let dir = tempfile::tempdir().unwrap();
    let (d, agent, pane, _reports) = with_browser(dir.path());
    let (_, mut rx) = d.watch_browsers();
    d.close_pane(agent).unwrap();

    assert!(browser_panes(&d).is_empty(), "the pane goes with its agent");
    let closed = told(&mut rx).into_iter().filter(|news| matches!(news, BrowserNews::Closed { pane: p } if *p == pane)).count();
    assert_eq!(closed, 1, "told once, however many ways it was taken out");
    assert_eq!(d.browser_of(agent), None);
}

#[tokio::test]
async fn a_browser_that_ends_on_its_own_leaves_the_tree() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, pane, reports) = with_browser(dir.path());
    let (_, mut rx) = d.watch_browsers();
    drop(reports);
    let gone = async {
        while !browser_panes(&d).is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), gone).await.expect("the pane stayed after its browser ended");
    assert!(told(&mut rx).iter().any(|news| matches!(news, BrowserNews::Closed { pane: closed } if *closed == pane)));
}

#[tokio::test]
async fn a_browser_pane_is_named_for_the_page_it_shows() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, _, reports) = with_browser(dir.path());
    let tab = |title: &str| argus_protocol::BrowserTab { id: "t1".into(), url: "http://x.test/".into(), title: title.into() };
    let said = |tabs: Vec<argus_protocol::BrowserTab>| {
        Report::Page(Seen { shown: tabs.first().map(|t| t.id.clone()), tabs, following: true, held: false, driver: None, dialog: None, select: None, sized: None })
    };
    let title_is = |want: &str| {
        let want = want.to_string();
        let d = d.clone();
        async move {
            let seen = async {
                while browser_panes(&d).first().map(|p| p.title.clone()) != Some(want.clone()) {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            };
            tokio::time::timeout(std::time::Duration::from_secs(5), seen).await.is_ok()
        }
    };
    reports.send(said(vec![tab("Docs")])).unwrap();
    assert!(title_is("Docs").await, "the shown tab names the pane");
    reports.send(said(Vec::new())).unwrap();
    assert!(title_is("Browser").await, "with no tab it is just a browser");
}

#[tokio::test]
async fn a_browser_whose_last_tab_closed_shows_a_joining_client_no_picture() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, pane, reports) = with_browser(dir.path());
    let tab = argus_protocol::BrowserTab { id: "t1".into(), url: "http://x.test/".into(), title: "X".into() };
    let page = |tabs: Vec<argus_protocol::BrowserTab>| {
        Report::Page(Seen { shown: tabs.first().map(|t| t.id.clone()), tabs, following: true, held: false, driver: None, dialog: None, select: None, sized: None })
    };
    let pictures = |d: &Daemon| d.watch_browsers().0.into_iter().filter(|n| matches!(n, BrowserNews::Frame(_))).count();
    let settle = || tokio::time::sleep(std::time::Duration::from_millis(50));
    reports.send(page(vec![tab])).unwrap();
    reports.send(Report::Frame { jpeg: "x".into(), width: 1, height: 1 }).unwrap();
    settle().await;
    assert_eq!(pictures(&d), 1);
    reports.send(page(Vec::new())).unwrap();
    settle().await;
    assert_eq!(pictures(&d), 0, "the picture was of a page that closed");
    assert!(browser_panes(&d).iter().any(|p| p.id == pane), "the browser itself stays open");
}

#[tokio::test]
async fn browser_panes_are_not_recorded_for_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (d, _, _, _reports) = with_browser(dir.path());
    let recorded = d.session_panes();
    assert!(recorded.iter().any(|p| p.kind == argus_protocol::PaneKind::Agent));
    assert!(recorded.iter().all(|p| p.kind != argus_protocol::PaneKind::Browser));
}

#[tokio::test]
async fn replacing_an_agent_in_place_takes_its_browser_pane_and_leaves_the_checkouts_hooks() {
    // `remove_pane` is the in-place replacement of an agent that restarts:
    // for a moment the checkout has no agent, and closing the browser the
    // way a person does would pull the hooks the new agent is about to use.
    let dir = tempfile::tempdir().unwrap();
    let (d, agent, _pane, _reports) = with_browser(dir.path());
    assert!(settings_of(dir.path()).exists(), "spawning installs hooks");

    d.remove_pane(agent);

    assert!(browser_panes(&d).is_empty(), "the browser pane goes with its agent");
    assert_eq!(d.browser_of(agent), None);
    assert!(settings_of(dir.path()).exists(), "the checkout's hooks stay for the replacement");
}
