//! The server end to end: a real listener, a page's requests over HTTP and
//! the WebSocket, and a daemon played on the other end of an in-memory
//! stream.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use argus_protocol::{
    read_msg, write_msg, Body, CheckoutId, CheckoutInfo, ClientMsg, Entry, Hello, PaneId,
    PaneInfo, PaneKind, PaneStatus, ProjectId, ProjectInfo, RepositoryId, RepositoryInfo,
    ServerMsg, Update, WorkspaceId, WorkspaceTree, WIDE_TREE,
};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{self, client::IntoClientRequest};

use crate::routes::App;
use crate::{daemon, pairing, routes, Devices};

/// A running server, the daemon end of its connection, and where it is.
struct Running {
    addr: std::net::SocketAddr,
    app: Arc<App>,
    daemon: mpsc::UnboundedReceiver<DuplexStream>,
    notices: mpsc::UnboundedReceiver<crate::push::Notice>,
    _dir: tempfile::TempDir,
}

async fn serve() -> Running {
    let dir = tempfile::tempdir().unwrap();
    let (streams, daemon) = mpsc::unbounded_channel();
    let streams = Arc::new(Mutex::new(streams));
    let connect = move || {
        let streams = streams.clone();
        async move {
            let (ours, theirs) = tokio::io::duplex(1 << 20);
            streams.lock().unwrap().send(theirs).unwrap();
            anyhow::Ok(ours)
        }
    };
    let (feeds, asks, notices, _link) = daemon::start(connect);
    let app = Arc::new(App {
        feeds,
        asks,
        devices: Devices::at(dir.path()),
        code: Mutex::new(pairing::Code::new().unwrap()),
        vapid: crate::push::Vapid::load_or_create(dir.path()).ok(),
        visible: Mutex::new(Default::default()),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = routes::router(app.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    Running {
        addr,
        app,
        daemon,
        notices,
        _dir: dir,
    }
}

/// A plain HTTP request, and the whole response as text.
async fn http(addr: std::net::SocketAddr, request: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

fn pairing_request(addr: std::net::SocketAddr, origin: &str, code: &str) -> String {
    let body = serde_json::json!({ "code": code, "name": "phone" }).to_string();
    format!(
        "POST /api/pair HTTP/1.1\r\nHost: {addr}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\n\
Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn paired_cookie(running: &Running) -> String {
    let code = running.app.code.lock().unwrap().digits().to_string();
    let origin = format!("http://{}", running.addr);
    let response = http(running.addr, &pairing_request(running.addr, &origin, &code)).await;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let line = response
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("set-cookie:"))
        .expect("pairing sets a cookie");
    assert!(line.contains("HttpOnly"), "{line}");
    assert!(line.contains("SameSite=Strict"), "{line}");
    line.split_once(':').unwrap().1.split(';').next().unwrap().trim().to_string()
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn open(addr: std::net::SocketAddr, cookie: Option<&str>, origin: &str) -> Result<Socket, tungstenite::Error> {
    let mut request = format!("ws://{addr}/ws").into_client_request().unwrap();
    request.headers_mut().insert("origin", origin.parse().unwrap());
    if let Some(cookie) = cookie {
        request.headers_mut().insert("cookie", cookie.parse().unwrap());
    }
    tokio_tungstenite::connect_async(request).await.map(|(socket, _)| socket)
}

async fn next_json(socket: &mut Socket) -> serde_json::Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("a message from the server")
            .unwrap()
            .unwrap();
        if let tungstenite::Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

async fn next_of(socket: &mut Socket, kind: &str) -> serde_json::Value {
    loop {
        let json = next_json(socket).await;
        if json["type"] == kind {
            return json;
        }
    }
}

/// The daemon's end: answers the greeting, sends a wide tree with one
/// waiting agent, and hands back the stream for the test to go on with.
async fn play_daemon(running: &mut Running) -> DuplexStream {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), running.daemon.recv())
        .await
        .unwrap()
        .unwrap();
    let ClientMsg::Hello(hello) = read_msg::<_, ClientMsg>(&mut stream).await.unwrap() else {
        panic!("argus web greets first");
    };
    assert!(hello.can(WIDE_TREE), "argus web asks for every workspace");
    write_msg(&mut stream, &ServerMsg::Hello(Hello::this_build())).await.unwrap();
    write_msg(&mut stream, &ServerMsg::WideTree(vec![tree()])).await.unwrap();
    stream
}

fn tree() -> WorkspaceTree {
    WorkspaceTree {
        id: WorkspaceId(0),
        name: "default".into(),
        open: true,
        projects: vec![ProjectInfo {
            id: ProjectId(0),
            name: "argus".into(),
            root: None,
            repositories: vec![RepositoryInfo {
                id: RepositoryId(0),
                name: "argus".into(),
                branches: Vec::new(),
                default_branch: None,
                remote_branches: Vec::new(),
                checkouts: vec![CheckoutInfo {
                    id: CheckoutId(0),
                    name: "main".into(),
                    path: "/repo".into(),
                    panes: vec![PaneInfo {
                        id: PaneId(7),
                        kind: PaneKind::Agent,
                        title: "fixing the build".into(),
                        status: PaneStatus::Waiting,
                        note: Some("allow cargo test?".into()),
                        template: Some("claude".into()),
                        children: Vec::new(),
                        telemetry: Default::default(),
                        has_transcript: true,
                        since: Some(1_790_000_000),
                        queued: Vec::new(),
                        live: false,
                    }],
                    git: None,
                    primary: true,
                }],
            }],
        }],
    }
}

#[tokio::test]
async fn every_response_carries_the_security_headers() {
    let running = serve().await;
    let page = http(
        running.addr,
        &format!("GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", running.addr),
    )
    .await
    .to_ascii_lowercase();
    assert!(page.starts_with("http/1.1 200"), "{page}");
    assert!(page.contains("content-security-policy: default-src 'none'; script-src 'self'"));
    assert!(page.contains("x-frame-options: deny"));
    assert!(page.contains("referrer-policy: no-referrer"));
    assert!(page.contains("x-content-type-options: nosniff"));
    assert!(page.contains("<script type=\"module\" src=\"/app.js\"></script>"));
}

#[tokio::test]
async fn an_unpaired_browser_is_told_so() {
    let running = serve().await;
    let me = http(
        running.addr,
        &format!("GET /api/me HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", running.addr),
    )
    .await;
    assert!(me.starts_with("HTTP/1.1 401"), "{me}");
}

#[tokio::test]
async fn a_wrong_code_or_a_foreign_page_cannot_pair() {
    let running = serve().await;
    let code = running.app.code.lock().unwrap().digits().to_string();
    let wrong = if code == "000000" { "111111" } else { "000000" };
    let origin = format!("http://{}", running.addr);

    let refused = http(running.addr, &pairing_request(running.addr, &origin, wrong)).await;
    assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");
    assert!(!refused.to_ascii_lowercase().contains("set-cookie"));

    let foreign = http(running.addr, &pairing_request(running.addr, "https://evil.example", &code)).await;
    assert!(foreign.starts_with("HTTP/1.1 403"), "{foreign}");
}

#[tokio::test]
async fn the_socket_needs_a_paired_device_and_this_servers_page() {
    let running = serve().await;
    let origin = format!("http://{}", running.addr);
    assert!(open(running.addr, None, &origin).await.is_err(), "no cookie");
    assert!(
        open(running.addr, Some("argus_device=forged"), &origin).await.is_err(),
        "a token nobody was given"
    );
    let cookie = paired_cookie(&running).await;
    assert!(
        open(running.addr, Some(&cookie), "https://evil.example").await.is_err(),
        "another site's page"
    );
    assert!(open(running.addr, Some(&cookie), &origin).await.is_ok());
}

#[tokio::test]
async fn a_paired_phone_sees_the_agents_and_follows_a_conversation() {
    let mut running = serve().await;
    let cookie = paired_cookie(&running).await;
    let mut daemon = play_daemon(&mut running).await;
    let origin = format!("http://{}", running.addr);
    let mut phone = open(running.addr, Some(&cookie), &origin).await.unwrap();

    let agents = loop {
        let json = next_of(&mut phone, "agents").await;
        if !json["workspaces"].as_array().unwrap().is_empty() {
            break json;
        }
    };
    let agent = &agents["workspaces"][0]["agents"][0];
    assert_eq!(agent["pane"], 7);
    assert_eq!(agent["status"], "waiting");
    assert_eq!(agent["note"], "allow cargo test?");

    phone
        .send(tungstenite::Message::Text(r#"{"type":"watch","pane":7}"#.into()))
        .await
        .unwrap();
    let asked = loop {
        match read_msg::<_, ClientMsg>(&mut daemon).await.unwrap() {
            ClientMsg::WatchTranscript { pane } => break pane,
            _ => continue,
        }
    };
    assert_eq!(asked, PaneId(7));

    write_msg(
        &mut daemon,
        &ServerMsg::Transcript {
            pane: PaneId(7),
            fresh: true,
            earlier: None,
            updates: vec![Update::Upsert(Entry {
                id: "0:0.0".into(),
                at: None,
                body: Body::Reply {
                    text: "**done** <script>x</script>".into(),
                },
            })],
        },
    )
    .await
    .unwrap();
    let transcript = next_of(&mut phone, "transcript").await;
    assert_eq!(transcript["fresh"], true);
    let html = transcript["updates"][0]["entry"]["html"].as_str().unwrap();
    assert!(html.contains("<strong>done</strong>"), "{html}");
    assert!(!html.contains("<script>"), "{html}");
}

#[tokio::test]
async fn a_revoked_phone_is_let_go() {
    let running = serve().await;
    let cookie = paired_cookie(&running).await;
    let origin = format!("http://{}", running.addr);
    let _phone = open(running.addr, Some(&cookie), &origin).await.unwrap();
    assert!(running.app.devices.revoke("phone").unwrap());
    assert!(open(running.addr, Some(&cookie), &origin).await.is_err());
}

#[test]
fn a_pairing_time_reads_as_its_date() {
    assert_eq!(crate::date(0), "1970-01-01");
    assert_eq!(crate::date(1_790_656_736), "2026-09-29");
    assert_eq!(crate::date(951_782_400), "2000-02-29");
}

#[tokio::test]
async fn a_phone_message_reaches_the_daemon_and_its_answer_comes_back() {
    let mut running = serve().await;
    let cookie = paired_cookie(&running).await;
    let mut daemon = play_daemon(&mut running).await;
    let origin = format!("http://{}", running.addr);
    let mut phone = open(running.addr, Some(&cookie), &origin).await.unwrap();
    let _ = next_of(&mut phone, "agents").await;

    phone
        .send(tungstenite::Message::Text(
            r#"{"type":"send","pane":7,"text":"also run clippy","now":false}"#.into(),
        ))
        .await
        .unwrap();
    let (request_id, text, now) = loop {
        match read_msg::<_, ClientMsg>(&mut daemon).await.unwrap() {
            ClientMsg::SendToAgent { request_id, text, now, .. } => break (request_id, text, now),
            _ => continue,
        }
    };
    assert_eq!((text.as_str(), now), ("also run clippy", false));
    write_msg(
        &mut daemon,
        &ServerMsg::Sent {
            request_id,
            pane: PaneId(7),
            sent: argus_protocol::Sent::Queued { id: 1 },
        },
    )
    .await
    .unwrap();
    let answer = next_of(&mut phone, "sent").await;
    assert_eq!(answer["outcome"], "queued");

    phone
        .send(tungstenite::Message::Text(r#"{"type":"stop","pane":7}"#.into()))
        .await
        .unwrap();
    let stopped = loop {
        match read_msg::<_, ClientMsg>(&mut daemon).await.unwrap() {
            ClientMsg::Interrupt { pane } => break pane,
            _ => continue,
        }
    };
    assert_eq!(stopped, PaneId(7));
}

#[tokio::test]
async fn a_phone_watches_a_screen_without_resizing_it_and_answers_with_keys() {
    let mut running = serve().await;
    let cookie = paired_cookie(&running).await;
    let mut daemon = play_daemon(&mut running).await;
    let origin = format!("http://{}", running.addr);
    let mut phone = open(running.addr, Some(&cookie), &origin).await.unwrap();
    let _ = next_of(&mut phone, "agents").await;

    phone
        .send(tungstenite::Message::Text(r#"{"type":"screen","pane":7}"#.into()))
        .await
        .unwrap();
    let mut seen = Vec::new();
    let subscribed = loop {
        let msg = read_msg::<_, ClientMsg>(&mut daemon).await.unwrap();
        let done = matches!(msg, ClientMsg::Subscribe { .. });
        seen.push(msg);
        if done {
            break seen.last().cloned().unwrap();
        }
    };
    assert!(matches!(subscribed, ClientMsg::Subscribe { pane: PaneId(7) }));

    let cells: Vec<argus_protocol::Cell> = "Allow cargo test? 1 yes 2 no"
        .chars()
        .map(|c| argus_protocol::Cell { ch: c.to_string().into(), ..Default::default() })
        .collect();
    write_msg(
        &mut daemon,
        &ServerMsg::PaneRows {
            pane: PaneId(7),
            rows: 2,
            cols: 40,
            runs: vec![argus_protocol::CellRun::encode(1, 0, &cells)],
            cursor: Default::default(),
            mouse: Default::default(),
            alternate_screen: false,
        },
    )
    .await
    .unwrap();
    let screen = next_of(&mut phone, "screen").await;
    assert_eq!(screen["fresh"], true);
    assert_eq!(screen["cols"], 40);
    assert_eq!(screen["lines"][1][1][0][0], "Allow cargo test? 1 yes 2 no");

    phone
        .send(tungstenite::Message::Text(r#"{"type":"key","pane":7,"key":"1"}"#.into()))
        .await
        .unwrap();
    phone
        .send(tungstenite::Message::Text(r#"{"type":"key","pane":7,"key":"rm -rf /"}"#.into()))
        .await
        .unwrap();
    let typed = loop {
        let msg = read_msg::<_, ClientMsg>(&mut daemon).await.unwrap();
        seen.push(msg.clone());
        if let ClientMsg::Input { bytes, .. } = msg {
            break bytes;
        }
    };
    assert_eq!(typed, b"1");
    let refused = next_of(&mut phone, "error").await;
    assert_eq!(refused["message"], "the key bar has no such key");
    assert!(
        !seen.iter().any(|m| matches!(m, ClientMsg::Resize { .. })),
        "a phone never resizes a pane: {seen:?}"
    );
}

fn with_status(status: PaneStatus, note: Option<&str>) -> WorkspaceTree {
    let mut t = tree();
    let pane = &mut t.projects[0].repositories[0].checkouts[0].panes[0];
    pane.status = status;
    pane.note = note.map(str::to_string);
    t
}

#[tokio::test]
async fn an_agent_starting_to_wait_is_worth_a_push_and_a_first_look_is_not() {
    let mut running = serve().await;
    let mut daemon = play_daemon(&mut running).await;
    // The tree play_daemon sent, with the agent already waiting, is the
    // first this connection saw: taken as it is, not announced.
    write_msg(&mut daemon, &ServerMsg::WideTree(vec![with_status(PaneStatus::Working, None)])).await.unwrap();
    write_msg(
        &mut daemon,
        &ServerMsg::WideTree(vec![with_status(PaneStatus::Waiting, Some("allow cargo test?"))]),
    )
    .await
    .unwrap();

    let notice = tokio::time::timeout(Duration::from_secs(5), running.notices.recv())
        .await
        .expect("a notice")
        .unwrap();
    assert_eq!(notice.pane, 7);
    assert_eq!(notice.title, "fixing the build");
    assert_eq!(notice.body, "claude is waiting for you: allow cargo test?");
    assert_eq!(notice.url, "/#/pane/7");
    assert!(running.notices.try_recv().is_err(), "only the change into waiting");
}

#[test]
fn a_notice_goes_to_subscribed_devices_that_are_not_looking() {
    let subscription = |n: &str| crate::push::Subscription {
        endpoint: format!("https://push.example/{n}"),
        keys: crate::push::SubscriptionKeys { p256dh: "k".into(), auth: "a".into() },
    };
    let device = |name: &str, push: bool| crate::pairing::Device {
        name: name.into(),
        hash: String::new(),
        paired: 0,
        push: push.then(|| subscription(name)),
    };
    let devices = [device("phone", true), device("tablet", true), device("laptop", false)];
    let looking = [("tablet".to_string(), 1usize)].into_iter().collect();
    let chosen: Vec<String> = crate::recipients(&devices, &looking).into_iter().map(|(n, _)| n).collect();
    assert_eq!(chosen, ["phone"]);
}

#[tokio::test]
async fn a_device_subscribes_to_pushes_only_from_this_page_and_only_to_a_push_service() {
    let running = serve().await;
    let cookie = paired_cookie(&running).await;
    let origin = format!("http://{}", running.addr);
    let post = |origin: &str, body: &str| {
        format!(
            "POST /api/push/subscribe HTTP/1.1\r\nHost: {}\r\nOrigin: {origin}\r\nCookie: {cookie}\r\n\
Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            running.addr,
            body.len()
        )
    };
    let good = r#"{"endpoint":"https://fcm.googleapis.com/fcm/send/x","expirationTime":null,"keys":{"p256dh":"k","auth":"a"}}"#;
    let plain = r#"{"endpoint":"http://169.254.169.254/latest","keys":{"p256dh":"k","auth":"a"}}"#;

    assert!(http(running.addr, &post("https://evil.example", good)).await.starts_with("HTTP/1.1 403"));
    assert!(http(running.addr, &post(&origin, plain)).await.starts_with("HTTP/1.1 400"));
    assert!(http(running.addr, &post(&origin, good)).await.starts_with("HTTP/1.1 204"));
    let device = running.app.devices.list().into_iter().find(|d| d.name == "phone").unwrap();
    assert_eq!(device.push.unwrap().endpoint, "https://fcm.googleapis.com/fcm/send/x");
}

#[tokio::test]
async fn the_installable_pages_files_are_served_as_what_they_are() {
    let running = serve().await;
    for (path, kind) in [
        ("/sw.js", "text/javascript"),
        ("/manifest.webmanifest", "application/manifest+json"),
        ("/icon-192.png", "image/png"),
        ("/icon-512.png", "image/png"),
    ] {
        let response = http(
            running.addr,
            &format!("GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", running.addr),
        )
        .await
        .to_ascii_lowercase();
        assert!(response.starts_with("http/1.1 200"), "{path}");
        assert!(response.contains(&format!("content-type: {kind}")), "{path}: {response:.200}");
    }
}
