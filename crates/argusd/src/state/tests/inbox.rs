//! What Argus says to an agent: through its harness's inbox while a plugin
//! holds one open, typed into its terminal otherwise.

use std::time::Duration;

use argus_protocol::InboxItem;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::*;

/// Everything on the pane's screen, row after row.
#[cfg(unix)]
fn screen(d: &Daemon, pane: PaneId) -> String {
    let (_, _, cells, _, _, _, _) = d.subscribe_pane(pane).unwrap();
    cells
        .iter()
        .map(|row| row.iter().map(|c| c.ch.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Waits up to `limit` for `text` to show on the pane's screen.
#[cfg(unix)]
async fn screen_shows(d: &Daemon, pane: PaneId, text: &str, limit: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    while tokio::time::Instant::now() < deadline {
        if screen(d, pane).contains(text) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// Opens the pane's inbox over the pane API as `session`, returning the
/// stream positioned after the response head, and the status line.
pub(super) async fn open_inbox(
    d: &Arc<Daemon>,
    pane: PaneId,
    session: &str,
) -> (String, BufReader<tokio::net::TcpStream>) {
    let port = d.hook_port.load(std::sync::atomic::Ordering::Relaxed);
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let request = format!(
        "GET {} HTTP/1.1\r\nAuthorization: Bearer {}\r\n{}: {session}\r\n\r\n",
        argus_protocol::pane_path(pane, argus_protocol::Endpoint::Inbox),
        d.hook_token,
        argus_protocol::SESSION_HEADER,
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    reader.read_line(&mut status).await.unwrap();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap() == 0 || line == "\r\n" {
            break;
        }
    }
    (status, reader)
}

/// The next item the inbox carries, skipping its heartbeats.
pub(super) async fn next_item(reader: &mut BufReader<tokio::net::TcpStream>) -> InboxItem {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            if let Some(json) = line.strip_prefix("data: ") {
                return serde_json::from_str(json.trim()).unwrap();
            }
        }
    })
    .await
    .expect("an item")
}

async fn live(d: &Daemon, pane: PaneId, want: bool) -> bool {
    for _ in 0..100 {
        if pane_info(d, pane).live == want {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

fn agent(dir: &std::path::Path) -> (Arc<Daemon>, PaneId) {
    let d = daemon_with_running_claude(dir);
    d.start_hook_server().unwrap();
    let pane = d.spawn_agent(only_checkout(&d), "claude").unwrap();
    d.set_pane_session_id(pane, "s1");
    (d, pane)
}

#[tokio::test]
async fn what_is_said_to_a_live_agent_goes_through_its_plugin_until_the_plugin_goes() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (status, mut inbox) = open_inbox(&d, pane, "s1").await;
    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    assert!(live(&d, pane, true).await, "the pane says it is live");

    // Even while it works: the harness queues it for itself.
    d.report_pane_status(pane, None, PaneStatus::Working, None, false);
    assert!(d.notify(pane, "also add tests"));
    assert_eq!(
        next_item(&mut inbox).await,
        InboxItem::Message { text: "also add tests".into() }
    );

    // The plugin goes away: the pane stops being live at once, and what is
    // said next is typed rather than handed to a connection nobody holds.
    drop(inbox);
    assert!(live(&d, pane, false).await, "the pane noticed its plugin go");
    assert!(d.notify(pane, "anyone there"));
    #[cfg(unix)]
    assert!(screen_shows(&d, pane, "anyone there", Duration::from_secs(5)).await);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn nothing_is_said_to_a_shell() {
    let dir = tempfile::tempdir().unwrap();
    let d = daemon_with_running_claude(dir.path());
    let shell = d.spawn_shell(only_checkout(&d)).unwrap();
    assert!(!d.notify(shell, "ls"), "a shell is not an agent");
    close_all(&d);
}

#[tokio::test]
async fn an_agent_started_inside_the_pane_cannot_take_its_inbox() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (status, _) = open_inbox(&d, pane, "child").await;
    assert!(status.starts_with("HTTP/1.1 409"), "{status}");
    assert!(!pane_info(&d, pane).live);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_plugin_that_reconnects_takes_over_from_its_old_stream() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (_, old) = open_inbox(&d, pane, "s1").await;
    let (_, mut new) = open_inbox(&d, pane, "s1").await;
    drop(old);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(pane_info(&d, pane).live, "the old stream closing does not close the new one");
    assert!(d.notify(pane, "still here?"));
    assert_eq!(
        next_item(&mut new).await,
        InboxItem::Message { text: "still here?".into() }
    );
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn a_review_comment_reaches_a_live_agent_through_its_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (_, mut inbox) = open_inbox(&d, pane, "s1").await;
    assert!(live(&d, pane, true).await);
    let checkout = only_checkout(&d);

    let (_, delivered) = d
        .submit_review_comment(checkout, pane, review_anchor(8), "fix this".to_string())
        .unwrap();

    assert!(delivered);
    let InboxItem::Message { text } = next_item(&mut inbox).await;
    assert!(text.contains("fix this"), "{text}");
    // The screen is only read back on Unix, where the test agent is `cat`.
    #[cfg(unix)]
    assert!(!screen(&d, pane).contains("fix this"), "nothing was typed");
    let _ = d.close_pane(pane);
}
