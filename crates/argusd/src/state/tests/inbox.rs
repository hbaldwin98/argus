//! The live channel a harness's plugin holds open: who may open it, what
//! goes through it instead of being typed, and letting go of it.

use std::time::Duration;

use argus_protocol::{InboxItem, Sent};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::*;

/// Opens the pane's inbox over the pane API as `session`, returning the
/// stream positioned after the response head, and the status line.
async fn open_inbox(
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
async fn next_item(reader: &mut BufReader<tokio::net::TcpStream>) -> InboxItem {
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
async fn what_is_said_to_a_live_agent_goes_through_its_plugin_not_its_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    let (status, mut inbox) = open_inbox(&d, pane, "s1").await;
    assert!(status.starts_with("HTTP/1.1 200"), "{status}");
    assert!(live(&d, pane, true).await, "the pane says it is live");

    // Even while it works: the harness queues or steers for itself.
    d.report_pane_status(pane, None, PaneStatus::Working, None);
    assert_eq!(d.send_to_agent(pane, "also add tests", false), Sent::Typed);
    assert_eq!(
        next_item(&mut inbox).await,
        InboxItem::Message { text: "also add tests".into(), steer: false }
    );
    assert!(pane_info(&d, pane).queued.is_empty(), "nothing waits in the outbox");

    assert_eq!(d.send_to_agent(pane, "stop and fix the build", true), Sent::Typed);
    assert_eq!(
        next_item(&mut inbox).await,
        InboxItem::Message { text: "stop and fix the build".into(), steer: true }
    );

    d.interrupt(pane).unwrap();
    assert_eq!(next_item(&mut inbox).await, InboxItem::Interrupt);

    d.answer(pane, "perm_1".into(), "once".into()).unwrap();
    assert_eq!(
        next_item(&mut inbox).await,
        InboxItem::Answer { question: "perm_1".into(), choice: "once".into() }
    );

    // The plugin goes away: the pane stops being live at once, and what is
    // said next waits in the outbox rather than going into a connection
    // nobody holds.
    drop(inbox);
    assert!(live(&d, pane, false).await, "the pane noticed its plugin go");
    assert!(
        matches!(d.send_to_agent(pane, "anyone?", false), Sent::Queued { .. }),
        "nothing is handed to a plugin that has gone; it waits in the outbox"
    );
    let _ = d.close_pane(pane);
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
    d.interrupt(pane).unwrap();
    assert_eq!(next_item(&mut new).await, InboxItem::Interrupt);
    let _ = d.close_pane(pane);
}

#[tokio::test]
async fn an_agent_with_no_live_channel_takes_answers_on_its_screen() {
    let dir = tempfile::tempdir().unwrap();
    let (d, pane) = agent(dir.path());
    assert!(d.answer(pane, "q".into(), "yes".into()).is_err());
    let _ = d.close_pane(pane);
}
