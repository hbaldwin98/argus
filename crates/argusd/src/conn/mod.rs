//! One client connection: read a message, dispatch it, write what comes
//! back.
//!
//! Dispatch is a chain of small functions, each matching the messages it
//! owns and handing the rest on. Anything that does real I/O — a git
//! subprocess, a directory walk, a diff — leaves the message loop for a
//! task of its own, because a slow answer for one client must never delay
//! a keystroke going to some other pane.

use std::sync::Arc;

use argus_protocol::{read_msg, write_frame, ClientMsg, PaneId, ServerMsg};
use tokio::io::{split, AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc, Semaphore};

use crate::state::{BranchDeletion, Daemon, ViewerId};

mod dispatch;

static REVIEW_PERMIT: Semaphore = Semaphore::const_new(1);

pub async fn handle<S>(stream: S, daemon: Arc<Daemon>, shutdown: broadcast::Sender<()>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut rd, wr) = split(stream);
    let (out_tx, out_rx) = mpsc::unbounded_channel::<ServerMsg>();
    let (restart_flushed, mut restart_flushed_rx) = broadcast::channel(1);

    tokio::spawn(writer_task(wr, out_rx, restart_flushed));

    if out_tx.send(ServerMsg::Tree(daemon.snapshot())).is_err() {
        return;
    }
    if out_tx
        .send(ServerMsg::Templates(daemon.template_names()))
        .is_err()
    {
        return;
    }
    if out_tx
        .send(ServerMsg::Workspaces(daemon.workspaces()))
        .is_err()
    {
        return;
    }

    // This connection's identity for as long as it lasts: the daemon
    // reconciles one pty size out of what every attached client asks for,
    // so its requests have to be told apart from the other clients'.
    let viewer = daemon.new_viewer();

    let mut tree_rx = daemon.subscribe_tree();
    let mut workspaces_rx = daemon.subscribe_workspaces();
    let mut decisions_rx = daemon.subscribe_decisions();
    let mut tasks_rx = daemon.subscribe_tasks();
    let mut subs = Subscriptions::default();
    let mut review_task = None;

    loop {
        tokio::select! {
            msg = read_msg::<_, ClientMsg>(&mut rd) => {
                match msg {
                    Ok(cmsg) => {
                        let restart = handle_client_msg(
                            cmsg,
                            &daemon,
                            &out_tx,
                            &mut subs,
                            &mut review_task,
                            viewer,
                        );
                        if restart {
                            // The writer owns the other end of this signal and
                            // sends it only after the acknowledgement frame is
                            // flushed to the client.
                            let _ = restart_flushed_rx.recv().await;
                            let _ = shutdown.send(());
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            Ok(tree) = tree_rx.recv() => {
                let _ = out_tx.send(ServerMsg::Tree(tree));
            }
            Ok(ws) = workspaces_rx.recv() => {
                let _ = out_tx.send(ServerMsg::Workspaces(ws));
            }
            // Every client is told, and one with another project open
            // drops it by name. Filtering here would mean the daemon
            // tracking what each client is looking at, which is the one
            // thing about a view it deliberately does not know.
            Ok(board) = decisions_rx.recv() => {
                let _ = out_tx.send(ServerMsg::Decisions(Box::new(board)));
            }
            // Same reasoning as the board above: pushed at everyone, and a
            // client holding another feature's list open drops it.
            Ok(list) = tasks_rx.recv() => {
                let _ = out_tx.send(ServerMsg::Tasks(Box::new(list)));
            }
        }
    }
    daemon.release_viewer(viewer);
    if let Some(task) = review_task {
        task.abort();
    }
}

/// The panes this connection is streaming, one forwarding task each.
///
/// More than one at a time because the client draws more than one at a
/// time: an editor in a floating window must not cost you sight of the
/// agent running behind it.
#[derive(Default)]
struct Subscriptions(std::collections::HashMap<PaneId, tokio::task::JoinHandle<()>>);

impl Subscriptions {
    fn add(
        &mut self,
        pane: PaneId,
        mut rx: broadcast::Receiver<ServerMsg>,
        out_tx: mpsc::UnboundedSender<ServerMsg>,
        daemon: Arc<Daemon>,
    ) {
        self.remove(pane);
        self.0.insert(
            pane,
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(msg) => {
                            if out_tx.send(msg).is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            let Ok((
                                rows,
                                cols,
                                cells,
                                cursor,
                                mouse,
                                alternate_screen,
                                replacement,
                            )) = daemon.subscribe_pane(pane)
                            else {
                                break;
                            };
                            if out_tx
                                .send(ServerMsg::PaneSnapshot {
                                    pane,
                                    rows,
                                    cols,
                                    cells,
                                    cursor,
                                    mouse,
                                    alternate_screen,
                                })
                                .is_err()
                            {
                                break;
                            }
                            rx = replacement;
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }),
        );
    }

    fn remove(&mut self, pane: PaneId) {
        if let Some(task) = self.0.remove(&pane) {
            task.abort();
        }
    }
}

impl Drop for Subscriptions {
    fn drop(&mut self) {
        for task in self.0.values() {
            task.abort();
        }
    }
}

fn handle_client_msg(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    subs: &mut Subscriptions,
    review_task: &mut Option<tokio::task::JoinHandle<()>>,
    viewer: ViewerId,
) -> bool {
    if matches!(&msg, ClientMsg::Restart) {
        let _ = out_tx.send(ServerMsg::Restarting);
        return true;
    }

    let result = dispatch::dispatch(msg, daemon, out_tx, subs, review_task, viewer);
    if let Err(e) = result {
        let _ = out_tx.send(ServerMsg::Error {
            message: e.to_string(),
        });
    }
    false
}

/// Writes what the connection has queued, in batches.
///
/// A message used to cost two writes and a flush of its own. With several
/// agents running that is a few hundred flushes a second on a socket
/// nobody is reading between them, and the queue behind it is unbounded —
/// so a client that fell behind stayed behind, and the backlog was paid for
/// in latency on every pane rather than just the noisy one. Draining
/// everything already queued into one buffer and flushing once collapses a
/// burst into a single write, which is what keeps the queue from being the
/// thing that makes the next frame late.
///
/// Ordering is exactly what it was: the batch is written in the order it
/// was queued, and the flush is the only thing that moved.
async fn writer_task<W>(
    wr: W,
    mut rx: mpsc::UnboundedReceiver<ServerMsg>,
    restart_flushed: broadcast::Sender<()>,
) where
    W: AsyncWrite + Unpin,
{
    let mut wr = tokio::io::BufWriter::new(wr);
    while let Some(msg) = rx.recv().await {
        let mut contains_restart = matches!(&msg, ServerMsg::Restarting);
        if write_frame(&mut wr, &msg).await.is_err() {
            break;
        }
        // Whatever else is already waiting rides along on this flush.
        let mut batched = 0;
        while batched < MAX_BATCHED_MESSAGES {
            let Ok(msg) = rx.try_recv() else { break };
            contains_restart |= matches!(&msg, ServerMsg::Restarting);
            if write_frame(&mut wr, &msg).await.is_err() {
                return;
            }
            batched += 1;
        }
        if tokio::io::AsyncWriteExt::flush(&mut wr).await.is_err() {
            break;
        }
        if contains_restart {
            let _ = restart_flushed.send(());
        }
    }
}

/// How much of the queue one flush may carry. A cap rather than the whole
/// backlog so a client that has been away for a while still starts seeing
/// frames while the rest is still going out.
const MAX_BATCHED_MESSAGES: usize = 256;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigFile, ProjectConfig};
    use argus_protocol::{CheckoutId, PaneId, ReviewBase};

    #[tokio::test]
    async fn a_burst_is_written_in_order_and_flushed_once() {
        // The batching exists to stop a few hundred flushes a second on a
        // socket nobody is reading between them. What it must not change is
        // the order, since a pane's snapshot and the damage that continues
        // it travel this queue together.
        let (client, mut daemon) = tokio::io::duplex(1024 * 1024);
        let (tx, rx) = mpsc::unbounded_channel();
        for i in 0..MAX_BATCHED_MESSAGES * 2 {
            tx.send(ServerMsg::Error {
                message: i.to_string(),
            })
            .unwrap();
        }
        drop(tx);

        let (restart_flushed, _) = broadcast::channel(1);
        tokio::spawn(writer_task(client, rx, restart_flushed));

        for i in 0..MAX_BATCHED_MESSAGES * 2 {
            let msg: ServerMsg = read_msg(&mut daemon).await.expect("a framed message");
            assert!(
                matches!(msg, ServerMsg::Error { ref message } if *message == i.to_string()),
                "message {i} arrived out of order: {msg:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_restart_ack_is_signaled_after_its_frame_is_flushed() {
        let (client, mut daemon) = tokio::io::duplex(1024 * 1024);
        let (tx, rx) = mpsc::unbounded_channel();
        let (restart_flushed, mut flushed_rx) = broadcast::channel(1);
        tokio::spawn(writer_task(client, rx, restart_flushed));

        tx.send(ServerMsg::Restarting).unwrap();
        assert!(matches!(
            read_msg::<_, ServerMsg>(&mut daemon).await.unwrap(),
            ServerMsg::Restarting
        ));
        tokio::time::timeout(std::time::Duration::from_secs(1), flushed_rx.recv())
            .await
            .expect("the writer should signal the flushed frame")
            .expect("the writer should remain alive");
    }

    #[tokio::test]
    async fn a_restart_request_flushes_its_ack_before_signaling_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = Harness::new(dir.path()).daemon;
        let (client, server) = tokio::io::duplex(1024 * 1024);
        let (shutdown, mut shutdown_rx) = broadcast::channel(1);
        let handler = tokio::spawn(handle(server, daemon, shutdown));
        let mut client = client;

        for _ in 0..3 {
            let _: ServerMsg = read_msg(&mut client).await.unwrap();
        }
        argus_protocol::write_msg(&mut client, &ClientMsg::Restart)
            .await
            .unwrap();
        assert!(matches!(
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                read_msg::<_, ServerMsg>(&mut client),
            )
            .await
            .unwrap()
            .unwrap(),
            ServerMsg::Restarting
        ));
        tokio::time::timeout(std::time::Duration::from_secs(1), shutdown_rx.recv())
            .await
            .expect("the daemon should signal shutdown after the acknowledgement")
            .expect("the shutdown sender should remain available");
        handler.await.unwrap();
    }

    struct Harness {
        daemon: Arc<Daemon>,
        tx: mpsc::UnboundedSender<ServerMsg>,
        rx: mpsc::UnboundedReceiver<ServerMsg>,
        subs: Subscriptions,
        review_task: Option<tokio::task::JoinHandle<()>>,
        viewer: ViewerId,
    }

    impl Harness {
        fn new(repo: &std::path::Path) -> Self {
            let daemon = Daemon::new(ConfigFile {
                workspaces: Vec::new(),
                projects: vec![ProjectConfig {
                    name: "proj".to_string(),
                    root: None,
                    repos: vec![repo.to_string_lossy().to_string()],
                    workspace: None,
                    ..Default::default()
                }],
                agents: Vec::new(),
                harnesses: Vec::new(),
            });
            let (tx, rx) = mpsc::unbounded_channel();
            let viewer = daemon.new_viewer();
            Harness {
                viewer,
                daemon,
                tx,
                rx,
                subs: Subscriptions::default(),
                review_task: None,
            }
        }

        fn checkout(&self) -> CheckoutId {
            self.daemon.snapshot()[0].repositories[0].checkouts[0].id
        }

        fn send(&mut self, msg: ClientMsg) -> bool {
            handle_client_msg(
                msg,
                &self.daemon,
                &self.tx,
                &mut self.subs,
                &mut self.review_task,
                self.viewer,
            )
        }

        fn replies(&mut self) -> Vec<ServerMsg> {
            let mut out = Vec::new();
            while let Ok(m) = self.rx.try_recv() {
                out.push(m);
            }
            out
        }

        async fn error(&mut self) -> String {
            let reply = tokio::time::timeout(std::time::Duration::from_secs(5), self.rx.recv())
                .await
                .expect("an error reply should arrive");
            match reply {
                Some(ServerMsg::Error { message }) => message,
                other => panic!("expected an error, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn a_failed_message_reaches_the_client_as_an_error() {
        // Every arm funnels its `Err` here; a silent failure would leave the
        // user pressing a key that does nothing.
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::SpawnShell {
            checkout: CheckoutId(9999),
        });
        let error = h.error().await;
        assert!(error.contains("no such checkout"), "{error}");
    }

    #[test]
    fn a_restart_message_is_acknowledged_without_dispatching_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());

        assert!(h.send(ClientMsg::Restart));
        assert!(matches!(h.replies().as_slice(), [ServerMsg::Restarting]));
    }

    #[tokio::test]
    async fn every_git_mutation_reports_its_refusal() {
        // These seven arms share one helper, so this is the test that says
        // the helper is wired to all of them: a bogus checkout has to come
        // back as a message rather than as a keypress that did nothing.
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        let gone = CheckoutId(9999);
        let branch = || "nope".to_string();

        let sent = [
            ClientMsg::SwitchBranch {
                checkout: gone,
                branch: branch(),
            },
            ClientMsg::CreateBranch {
                checkout: gone,
                branch: branch(),
            },
            ClientMsg::DeleteBranch {
                checkout: gone,
                branch: branch(),
                force: false,
            },
            ClientMsg::Fetch { checkout: gone },
            ClientMsg::Pull { checkout: gone },
            ClientMsg::CreateWorktree {
                checkout: gone,
                branch: branch(),
            },
            ClientMsg::RemoveCheckout { checkout: gone },
        ];
        let expected = sent.len();
        for msg in sent {
            h.send(msg);
        }

        for _ in 0..expected {
            let error = h.error().await;
            assert!(error.contains("no such checkout"), "{error}");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_blocked_pane_launch_does_not_block_the_connection_task() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let started = std::time::Instant::now();

        dispatch::spawn_pane(&tx, || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            Ok(())
        })
        .unwrap();

        assert!(
            started.elapsed() < std::time::Duration::from_millis(100),
            "pane launch held the connection task for {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn subscribing_sends_a_snapshot_and_arms_the_damage_stream() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        let checkout = h.checkout();
        let pane = h.daemon.spawn_shell(checkout).unwrap();

        h.send(ClientMsg::Subscribe { pane });

        assert!(matches!(
            h.replies().first(),
            Some(ServerMsg::PaneSnapshot { .. })
        ));
        assert!(
            h.subs.0.contains_key(&pane),
            "damage must flow after a subscribe"
        );

        h.send(ClientMsg::Unsubscribe { pane });
        assert!(!h.subs.0.contains_key(&pane));

        let _ = h.daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn subscribing_to_a_pane_that_is_gone_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::Subscribe { pane: PaneId(9999) });
        assert!(!h.error().await.is_empty());
        assert!(h.subs.0.is_empty());
    }

    #[tokio::test]
    async fn a_lagged_subscription_recovers_with_a_fresh_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::new(dir.path());
        let checkout = h.checkout();
        let pane = h.daemon.spawn_shell(checkout).unwrap();
        let (damage_tx, rx) = broadcast::channel(1);
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();

        for message in ["one", "two"] {
            damage_tx
                .send(ServerMsg::Error {
                    message: message.to_string(),
                })
                .unwrap();
        }

        let mut subs = Subscriptions::default();
        subs.add(pane, rx, out_tx, h.daemon.clone());

        let recovered = tokio::time::timeout(std::time::Duration::from_secs(5), out_rx.recv())
            .await
            .expect("lag recovery should answer")
            .expect("output channel should remain open");
        assert!(matches!(recovered, ServerMsg::PaneSnapshot { pane: id, .. } if id == pane));

        let _ = h.daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn a_review_request_answers_with_that_checkouts_diff() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        drop(repo);

        let mut h = Harness::new(dir.path());
        let checkout = h.checkout();
        h.send(ClientMsg::Review {
            request_id: 42,
            checkout,
            base: ReviewBase::Unstaged,
            commit: None,
        });

        // The diff runs on a blocking thread, so the reply is not immediate.
        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
            .await
            .expect("the diff should arrive")
            .expect("channel open");
        match reply {
            ServerMsg::Review(r) => {
                assert_eq!(r.checkout, checkout);
                assert_eq!(r.request_id, 42);
                assert_eq!(r.base, ReviewBase::Unstaged);
                assert_eq!(r.files[0].path, "a.txt");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_new_review_replaces_an_older_queued_review() {
        let permit = REVIEW_PERMIT.acquire().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        let mut h = Harness::new(dir.path());
        let checkout = h.checkout();

        for request_id in [1, 2] {
            h.send(ClientMsg::Review {
                request_id,
                checkout,
                base: ReviewBase::Unstaged,
                commit: None,
            });
        }
        drop(permit);

        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
            .await
            .expect("the newest diff should arrive")
            .expect("channel open");
        assert!(matches!(
            reply,
            ServerMsg::Review(argus_protocol::Review { request_id: 2, .. })
        ));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), h.rx.recv())
                .await
                .is_err(),
            "the replaced review should not run"
        );
    }

    #[tokio::test]
    async fn a_review_of_a_checkout_that_is_gone_errors_without_spawning_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::Review {
            request_id: 1,
            checkout: CheckoutId(9999),
            base: ReviewBase::Unstaged,
            commit: None,
        });
        assert!(h.error().await.contains("no such checkout"));
    }

    #[tokio::test]
    async fn input_and_resize_for_a_dead_pane_do_not_take_the_connection_down() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::Input {
            pane: PaneId(9999),
            bytes: b"hello".to_vec(),
        });
        h.send(ClientMsg::Resize {
            pane: PaneId(9999),
            rows: 10,
            cols: 40,
        });
        assert_eq!(h.replies().len(), 2, "one error each, and still running");
    }

    #[tokio::test]
    async fn opening_an_editor_on_a_path_outside_the_checkout_is_refused_here_too() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        let checkout = h.checkout();
        h.send(ClientMsg::OpenInEditor {
            checkout,
            path: "../escape.rs".to_string(),
            line: None,
            external: false,
            command: None,
        });
        let error = h.error().await;
        assert!(error.contains("inside the checkout"), "{error}");
    }

    #[tokio::test]
    async fn switching_to_a_workspace_that_does_not_exist_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::OpenWorkspace {
            workspace: argus_protocol::WorkspaceId(9999),
        });
        assert!(!h.error().await.is_empty());
    }
    #[tokio::test]
    async fn two_panes_stream_at_once() {
        // A floating editor must not cost the client sight of the agent
        // behind it, so one connection carries more than one subscription.
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        let checkout = h.checkout();
        let a = h.daemon.spawn_shell(checkout).unwrap();
        let b = h.daemon.spawn_shell(checkout).unwrap();

        h.send(ClientMsg::Subscribe { pane: a });
        h.send(ClientMsg::Subscribe { pane: b });
        assert_eq!(h.subs.0.len(), 2);

        // Both snapshots arrive, and neither subscription displaced the
        // other.
        let panes: Vec<PaneId> = h
            .replies()
            .into_iter()
            .filter_map(|m| match m {
                ServerMsg::PaneSnapshot { pane, .. } => Some(pane),
                _ => None,
            })
            .collect();
        assert!(panes.contains(&a) && panes.contains(&b), "{panes:?}");

        h.send(ClientMsg::Unsubscribe { pane: a });
        assert_eq!(h.subs.0.len(), 1, "only the one named is dropped");
        assert!(h.subs.0.contains_key(&b));

        let _ = h.daemon.close_pane(a);
        let _ = h.daemon.close_pane(b);
    }

    #[tokio::test]
    async fn subscribing_twice_to_one_pane_does_not_double_up() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        let checkout = h.checkout();
        let pane = h.daemon.spawn_shell(checkout).unwrap();

        h.send(ClientMsg::Subscribe { pane });
        h.send(ClientMsg::Subscribe { pane });
        assert_eq!(h.subs.0.len(), 1);

        let _ = h.daemon.close_pane(pane);
    }
}
