//! How each topic brings a client that fell behind level, and how joining
//! and leaving go. A lagged receiver is made by publishing past the
//! channel's depth before the forwarder starts, so the lag is certain.

use super::*;
use crate::connection::tests::{daemon_with_a_pane, daemon_with_an_agent, view_change, Harness};
use crate::state::{BOARDS_DEPTH, EXTENSIONS_DEPTH, TREE_DEPTH, VIEWS_DEPTH};
use argus_protocol::{AgentTelemetry, DecisionBoard, Hello, ProjectId, RepositoryId, TaskList};
use std::time::Duration;

/// What `topic` sends once forwarded from `rx`, read off its queue.
fn forwarded<T: Topic>(
    topic: T,
    daemon: &Arc<Daemon>,
    rx: broadcast::Receiver<T::News>,
) -> mpsc::UnboundedReceiver<ServerMsg> {
    let (out, sent) = mpsc::unbounded_channel();
    tokio::spawn(forward(topic, daemon.clone(), out, rx));
    sent
}

async fn next(sent: &mut mpsc::UnboundedReceiver<ServerMsg>) -> ServerMsg {
    tokio::time::timeout(Duration::from_secs(5), sent.recv())
        .await
        .expect("a message should arrive")
        .expect("the forwarder should still run")
}

/// A greeted client's peer, which can take one pane's record.
fn greeted() -> Peer {
    let peer = Peer::default();
    peer.set(Hello::this_build());
    peer
}

#[tokio::test]
async fn a_client_that_fell_behind_the_tree_is_sent_it_whole() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, pane) = daemon_with_a_pane(dir.path());
    let mut topic = Tree { peer: greeted() };
    let (_, rx) = topic.join(&daemon);

    let last = TREE_DEPTH as u64 + 5;
    for tokens in 1..=last {
        let report = AgentTelemetry { context_tokens: Some(tokens), ..Default::default() };
        daemon.report_pane_telemetry(pane, None, report);
    }
    let mut sent = forwarded(topic, &daemon, rx);

    // Not the records one by one, which this client could take: the tree,
    // which holds the last of them.
    match next(&mut sent).await {
        ServerMsg::Tree(tree) => {
            let info = &tree[0].repositories[0].checkouts[0].panes[0];
            assert_eq!(info.telemetry.context_tokens, Some(last));
        }
        other => panic!("expected the tree, got {other:?}"),
    }
    assert!(matches!(next(&mut sent).await, ServerMsg::Consoles(_)), "with the consoles it takes");
    let _ = daemon.close_pane(pane);
}

fn decisions(repository: u64) -> BoardNews {
    BoardNews::Decisions(Box::new(DecisionBoard {
        repositories: vec![RepositoryId(repository)],
        project: Some(ProjectId(repository)),
        ..Default::default()
    }))
}

fn tasks(repository: u64, version: usize) -> BoardNews {
    BoardNews::Tasks(Box::new(TaskList {
        project_name: format!("v{version}"),
        feature: Some("pushes".into()),
        tasks: Vec::new(),
        repositories: vec![RepositoryId(repository)],
    }))
}

#[tokio::test]
async fn a_client_that_fell_behind_the_boards_is_sent_each_ones_latest() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = Harness::new(dir.path()).daemon;
    let mut topic = Boards;
    let (joined, rx) = topic.join(&daemon);
    assert!(joined.is_empty(), "a client asks for the boards it shows");

    daemon.pushes().push_board(decisions(1));
    for version in 0..BOARDS_DEPTH + 10 {
        daemon.pushes().push_board(tasks(2, version));
    }
    let mut sent = forwarded(topic, &daemon, rx);

    match next(&mut sent).await {
        ServerMsg::Decisions(board) => assert_eq!(board.repositories, [RepositoryId(1)]),
        other => panic!("expected the first repository's board, got {other:?}"),
    }
    match next(&mut sent).await {
        ServerMsg::Tasks(list) => assert_eq!(list.project_name, format!("v{}", BOARDS_DEPTH + 9)),
        other => panic!("expected the second's last list, got {other:?}"),
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(sent.try_recv().is_err(), "and no older list after it");
}

#[tokio::test]
async fn a_client_that_fell_behind_the_views_is_replayed_them() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, pane) = daemon_with_an_agent(dir.path());
    let mut topic = Views;
    let (_, rx) = topic.join(&daemon);

    for n in 0..VIEWS_DEPTH + 5 {
        daemon.change_view_for_agent(pane, view_change(&format!("v{n}"))).unwrap();
    }
    let held: Vec<String> = daemon.watch_views().0.into_iter().map(|shown| shown.view.id).collect();
    assert!(!held.is_empty());
    let mut sent = forwarded(topic, &daemon, rx);

    let mut replayed = Vec::new();
    for _ in 0..held.len() {
        match next(&mut sent).await {
            ServerMsg::ViewShown(shown) => replayed.push(shown.view.id),
            other => panic!("expected the replay, got {other:?}"),
        }
    }
    assert_eq!(replayed, held);
    let _ = daemon.close_pane(pane);
}

#[tokio::test]
async fn a_client_that_fell_behind_is_put_its_open_dialogs_again() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = Harness::new(dir.path()).daemon;
    daemon.admit_extension("docker");
    let viewer = daemon.new_viewer();
    daemon.extension_asker(viewer, true);
    let mut topic = Extensions { viewer };
    let (_, rx) = topic.join(&daemon);

    let dialog = argus_protocol::ExtDialog {
        title: "t".into(),
        kind: argus_protocol::ExtDialogKind::Confirm { message: "m".into(), yes: None, danger: false },
        answers: None,
    };
    let id = daemon.raise_dialog("docker", dialog).unwrap();
    for n in 0..EXTENSIONS_DEPTH + 5 {
        daemon.extension_notice("docker", &n.to_string()).unwrap();
    }
    let mut sent = forwarded(topic, &daemon, rx);

    assert!(matches!(next(&mut sent).await, ServerMsg::ExtensionViews(_)));
    assert!(matches!(next(&mut sent).await, ServerMsg::ExtensionCommands(_)));
    assert!(matches!(next(&mut sent).await, ServerMsg::ExtensionDialog { id: got, .. } if got == id));
}

#[tokio::test]
async fn joining_a_topic_again_replaces_it_rather_than_doubling_it() {
    let dir = tempfile::tempdir().unwrap();
    let (daemon, pane) = daemon_with_an_agent(dir.path());
    let (out, mut sent) = mpsc::unbounded_channel();
    let mut topics = Topics::new(daemon.clone(), out);
    topics.join(Views);
    topics.join(Views);
    assert!(sent.try_recv().is_err(), "no views to replay, twice");

    daemon.change_view_for_agent(pane, view_change("a")).unwrap();

    assert!(matches!(next(&mut sent).await, ServerMsg::ViewShown(shown) if shown.view.id == "a"));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(sent.try_recv().is_err(), "sent once, not once per join");
    let _ = daemon.close_pane(pane);
}

#[tokio::test]
async fn what_joining_sends_is_queued_before_join_returns() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = Harness::new(dir.path()).daemon;
    let (out, mut sent) = mpsc::unbounded_channel();
    let mut topics = Topics::new(daemon, out);

    topics.join(Workspaces);

    assert!(matches!(sent.try_recv(), Ok(ServerMsg::Workspaces(_))));
}

#[tokio::test]
async fn a_connection_that_ends_stops_forwarding_its_topics() {
    let dir = tempfile::tempdir().unwrap();
    let daemon = Harness::new(dir.path()).daemon;
    let (out, mut sent) = mpsc::unbounded_channel();
    let mut topics = Topics::new(daemon, out);
    topics.join(Tree { peer: Peer::default() });
    topics.join(Workspaces);
    topics.join(Boards);
    topics.join(Focus::default());

    drop(topics);

    // Every forwarder held a sender, so the queue closes only once all of
    // them have stopped.
    let closed = tokio::time::timeout(Duration::from_secs(1), async {
        while sent.recv().await.is_some() {}
    })
    .await;
    assert!(closed.is_ok(), "a forwarder outlived its connection");
}
