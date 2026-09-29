//! One client connection: read a message, dispatch it, write what comes
//! back.
//!
//! Dispatch is a chain of small functions, each matching the messages it
//! owns and handing the rest on. Anything that does real I/O — a git
//! subprocess, a directory walk, a diff — leaves the message loop for a
//! task of its own, because a slow answer for one client must never delay
//! a keystroke going to some other pane.

use std::sync::Arc;

use argus_protocol::{
    grid_from_runs, read_known_msg, write_frame, CellRun, ClientMsg, Hello, PaneId, ServerMsg,
    CELL_RUNS, LIVE_CHANNELS, PANE_TELEMETRY, WIDE_TREE,
};
use tokio::io::{split, AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc, Semaphore};

use crate::state::{BranchDeletion, Daemon, ViewerId};

mod dispatch;

static REVIEW_PERMIT: Semaphore = Semaphore::const_new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShutdownReason {
    Restart,
    Stop,
}

const STOP_NOTICE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub async fn handle<S>(
    stream: S,
    daemon: Arc<Daemon>,
    shutdown: broadcast::Sender<ShutdownReason>,
    mut shutdown_rx: broadcast::Receiver<ShutdownReason>,
)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (rd, wr) = split(stream);
    let (mut client_rx, reader) = spawn_client_reader(rd);
    let (out_tx, out_rx) = mpsc::unbounded_channel::<ServerMsg>();
    let (frames_tx, frames_rx) = mpsc::channel::<ServerMsg>(FRAME_QUEUE);
    let (control_flushed, mut control_flushed_rx) = broadcast::channel(1);

    tokio::spawn(writer_task(wr, out_rx, frames_rx, control_flushed));

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
    let mut telemetry_rx = daemon.subscribe_telemetry();
    let mut workspaces_rx = daemon.subscribe_workspaces();
    let mut decisions_rx = daemon.subscribe_decisions();
    let mut tasks_rx = daemon.subscribe_tasks();
    let mut diagrams_rx = daemon.subscribe_diagrams();
    let mut subs = Subscriptions::new(frames_tx);
    let mut review_task = None;
    // What the client said it can take: nothing until it greets, and
    // nothing for good from a client that never does.
    let mut peer: Option<Hello> = None;
    // Held while a client that reads live channels stays, so agents that
    // start meanwhile get theirs.
    let mut live_reader = None;

    loop {
        tokio::select! {
            // In order, so a tree taken before a telemetry report cannot
            // land after it and wind that pane's numbers back.
            biased;
            reason = shutdown_rx.recv() => {
                let stop_notice_queued = match reason {
                    Ok(ShutdownReason::Stop) => out_tx.send(ServerMsg::Stopping).is_ok(),
                    _ => false,
                };
                if stop_notice_queued {
                    let _ = tokio::time::timeout(
                        STOP_NOTICE_TIMEOUT,
                        control_flushed_rx.recv(),
                    ).await;
                }
                break;
            }
            msg = client_rx.recv() => {
                let Some(cmsg) = msg else { break };
                if let ClientMsg::Hello(hello) = &cmsg {
                    subs.runs = hello.can(CELL_RUNS);
                    if hello.can(LIVE_CHANNELS) && live_reader.is_none() {
                        live_reader = Some(daemon.live_reader());
                    }
                    peer = Some(hello.clone());
                }
                let wide_greeting = matches!(&cmsg, ClientMsg::Hello(h) if h.can(WIDE_TREE));
                let shutdown_reason = handle_client_msg(
                    cmsg,
                    &daemon,
                    &out_tx,
                    &mut subs,
                    &mut review_task,
                    viewer,
                );
                if let Some(reason) = shutdown_reason {
                    // The writer owns the other end of this signal and
                    // sends it only after the acknowledgement frame is
                    // flushed to the client.
                    let _ = control_flushed_rx.recv().await;
                    let _ = shutdown.send(reason);
                    break;
                }
                // Behind the greeting's answer, so the client knows what it
                // is reading; the tree it was sent on connecting was only
                // the open workspace's.
                if wide_greeting {
                    let _ = out_tx.send(ServerMsg::WideTree(daemon.wide_snapshot()));
                }
            }
            Ok(tree) = tree_rx.recv() => {
                let _ = out_tx.send(tree_for(&daemon, peer.as_ref(), tree));
            }
            report = telemetry_rx.recv() => match report {
                Ok((pane, telemetry)) if peer.as_ref().is_some_and(|p| p.can(PANE_TELEMETRY)) => {
                    let _ = out_tx.send(ServerMsg::PaneTelemetry { pane, telemetry });
                }
                // A client that cannot take one pane's record, or one that
                // fell behind the records, is sent them all in a tree.
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {
                    let _ = out_tx.send(tree_for(&daemon, peer.as_ref(), daemon.snapshot()));
                }
                // The daemon holds the sender for as long as it runs.
                Err(broadcast::error::RecvError::Closed) => break,
            },
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
            Ok(list) = diagrams_rx.recv() => {
                let _ = out_tx.send(ServerMsg::SequenceDiagrams(Box::new(list)));
            }
        }
    }
    daemon.release_viewer(viewer);
    drop(live_reader);
    reader.abort();
    if let Some(task) = review_task {
        task.abort();
    }
}

/// The tree as this client reads it: every workspace's to one that listed
/// `WIDE_TREE`, the open workspace's (`tree`) to any other.
fn tree_for(daemon: &Daemon, peer: Option<&Hello>, tree: Vec<argus_protocol::ProjectInfo>) -> ServerMsg {
    if peer.is_some_and(|p| p.can(WIDE_TREE)) {
        ServerMsg::WideTree(daemon.wide_snapshot())
    } else {
        ServerMsg::Tree(tree)
    }
}

/// How many client messages may wait for the message loop before the
/// reader stops taking more off the socket.
const CLIENT_QUEUE: usize = 64;

/// Reads the client's frames on a task of their own, for the message loop
/// to take from a channel.
///
/// `read_msg` is not cancellation-safe: dropped partway through a frame, it
/// loses the bytes it had read and leaves the stream misaligned. Called in
/// the message loop's `select!`, it was dropped whenever a tree or a board
/// arrived first — harmless while frames arrive whole, as small ones do on
/// a local socket, and a broken connection for a large paste or for any
/// frame on a slow link.
fn spawn_client_reader<R>(
    mut rd: R,
) -> (mpsc::Receiver<ClientMsg>, tokio::task::JoinHandle<()>)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (tx, rx) = mpsc::channel(CLIENT_QUEUE);
    let reader = tokio::spawn(async move {
        while let Ok(msg) = read_known_msg::<_, ClientMsg>(&mut rd).await {
            if tx.send(msg).await.is_err() {
                break;
            }
        }
    });
    (rx, reader)
}

/// How many pane frames may wait for a client that is slow to read.
///
/// A frame that finds the queue full is dropped rather than queued behind
/// it: the pane falls behind and catches up with one fresh snapshot once
/// there is room, so a client that cannot keep up sees the latest screen
/// late rather than every screen later and later. Deep enough to ride out
/// a burst for a client that is keeping up — about half a second of one
/// pane at full rate — without paying for a snapshot.
const FRAME_QUEUE: usize = 32;

/// The panes this connection is streaming, one forwarding task each, and
/// the bounded queue they share on the way to the writer.
///
/// More than one at a time because the client draws more than one at a
/// time: an editor in a floating window must not cost you sight of the
/// agent running behind it.
struct Subscriptions {
    tasks: std::collections::HashMap<PaneId, tokio::task::JoinHandle<()>>,
    /// The panes whose conversation this connection is watching, one
    /// forwarding task each. They send on the reply queue rather than the
    /// frame queue: an update is a record, not a frame a later one can
    /// stand in for.
    transcripts: std::collections::HashMap<PaneId, tokio::task::JoinHandle<()>>,
    /// Whether the client reads a pane's screen as runs (`CELL_RUNS`).
    /// Panes are sent in the form it reads; one that does not is sent the
    /// per-cell form, which the forwarder builds from the runs.
    runs: bool,
    /// Everything a pane sends travels this one queue — its snapshots, its
    /// damage, its copies and its exit — because the client applies them
    /// in order: damage ahead of its snapshot lands on no grid, and an exit
    /// ahead of the last frame removes the grid that frame was for.
    frames: mpsc::Sender<ServerMsg>,
}

impl Subscriptions {
    fn new(frames: mpsc::Sender<ServerMsg>) -> Self {
        Subscriptions {
            tasks: std::collections::HashMap::new(),
            transcripts: std::collections::HashMap::new(),
            runs: false,
            frames,
        }
    }

    /// Streams `pane` from `snapshot`, which `rx` continues.
    ///
    /// The snapshot is queued here, before the forwarder exists, so no
    /// frame can overtake the grid it applies to. A full queue queues
    /// nothing, and the forwarder starts out behind.
    fn add(
        &mut self,
        pane: PaneId,
        snapshot: ServerMsg,
        rx: broadcast::Receiver<ServerMsg>,
        daemon: Arc<Daemon>,
    ) {
        self.remove(pane);
        let behind = match self.frames.try_send(snapshot) {
            Ok(()) => false,
            Err(mpsc::error::TrySendError::Full(_)) => true,
            Err(mpsc::error::TrySendError::Closed(_)) => return,
        };
        let forwarder = forward_pane(pane, rx, self.frames.clone(), daemon, behind, self.runs);
        self.tasks.insert(pane, tokio::spawn(forwarder));
    }

    fn remove(&mut self, pane: PaneId) {
        if let Some(task) = self.tasks.remove(&pane) {
            task.abort();
        }
    }

    fn watch_transcript(
        &mut self,
        pane: PaneId,
        daemon: Arc<Daemon>,
        out: mpsc::UnboundedSender<ServerMsg>,
    ) {
        self.unwatch_transcript(pane);
        let task = tokio::spawn(forward_transcript(pane, daemon, out));
        self.transcripts.insert(pane, task);
    }

    fn unwatch_transcript(&mut self, pane: PaneId) {
        if let Some(task) = self.transcripts.remove(&pane) {
            task.abort();
        }
    }
}

impl Drop for Subscriptions {
    fn drop(&mut self) {
        for task in self.tasks.values().chain(self.transcripts.values()) {
            task.abort();
        }
    }
}

/// Carries one pane's conversation to the connection: its tail, then every
/// update after it, and the tail again whenever the connection falls
/// behind the updates.
///
/// The watch is joined before the tail is read, so nothing written in
/// between is missed; what arrives twice replaces itself.
async fn forward_transcript(
    pane: PaneId,
    daemon: Arc<Daemon>,
    out: mpsc::UnboundedSender<ServerMsg>,
) {
    let mut rx = daemon.watch_transcript(pane);
    let _watching = TranscriptWatch {
        daemon: daemon.clone(),
        pane,
    };
    loop {
        let reader = daemon.clone();
        let Ok(tail) = tokio::task::spawn_blocking(move || reader.transcript_tail(pane)).await
        else {
            return;
        };
        if out.send(tail).is_err() {
            return;
        }
        loop {
            match rx.recv().await {
                Ok(update) => {
                    if out.send(update).is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => break,
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }
}

/// One connection's hold on a pane's followers, let go however the task
/// holding it ends — including being aborted.
struct TranscriptWatch {
    daemon: Arc<Daemon>,
    pane: PaneId,
}

impl Drop for TranscriptWatch {
    fn drop(&mut self) {
        self.daemon.unwatch_transcript(self.pane);
    }
}

/// A pane's whole grid as the message that carries it, in runs when the
/// client reads them, and the damage stream that continues it.
fn subscribe(
    daemon: &Daemon,
    pane: PaneId,
    runs: bool,
) -> anyhow::Result<(ServerMsg, broadcast::Receiver<ServerMsg>)> {
    let (rows, cols, cells, cursor, mouse, alternate_screen, rx) = daemon.subscribe_pane(pane)?;
    let snapshot = if runs {
        ServerMsg::PaneRows {
            pane,
            rows,
            cols,
            runs: argus_protocol::grid_runs(&cells),
            cursor,
            mouse,
            alternate_screen,
        }
    } else {
        ServerMsg::PaneSnapshot {
            pane,
            rows,
            cols,
            cells,
            cursor,
            mouse,
            alternate_screen,
        }
    };
    Ok((snapshot, rx))
}

/// A pane frame in the per-cell form, for a client that does not read
/// runs. `None` for a scroll, which that form cannot say: the client falls
/// behind and catches up with a fresh grid, which is what a scroll cost it
/// before runs anyway.
fn per_cell(frame: ServerMsg) -> Option<ServerMsg> {
    match frame {
        ServerMsg::PaneRows {
            pane,
            rows,
            cols,
            runs,
            cursor,
            mouse,
            alternate_screen,
        } => Some(ServerMsg::PaneSnapshot {
            pane,
            rows,
            cols,
            cells: grid_from_runs(rows, cols, &runs),
            cursor,
            mouse,
            alternate_screen,
        }),
        ServerMsg::RowDamage {
            scroll: Some(_), ..
        } => None,
        ServerMsg::RowDamage {
            pane,
            scroll: None,
            runs,
            cursor,
            mouse,
            alternate_screen,
        } => Some(ServerMsg::Damage {
            pane,
            spans: runs.iter().map(CellRun::span).collect(),
            cursor,
            mouse,
            alternate_screen,
        }),
        other => Some(other),
    }
}

/// Carries one pane's broadcast to the connection's frame queue.
///
/// Frames are latest-wins. One that finds the queue full is dropped and
/// the pane falls behind: its frames are skipped until a slot frees, and
/// that slot carries a fresh snapshot with the stream that continues it.
/// Falling behind the broadcast itself recovers the same way. A copy and
/// an exit are one-offs no later frame can stand in for, so they wait for
/// room instead, behind or not. Frames go out in the form the client reads
/// (`runs`).
async fn forward_pane(
    pane: PaneId,
    mut rx: broadcast::Receiver<ServerMsg>,
    frames: mpsc::Sender<ServerMsg>,
    daemon: Arc<Daemon>,
    mut behind: bool,
    runs: bool,
) {
    loop {
        tokio::select! {
            biased;
            permit = frames.reserve(), if behind => {
                let Ok(permit) = permit else { return };
                let Ok((snapshot, replacement)) = subscribe(&daemon, pane, runs) else { return };
                permit.send(snapshot);
                rx = replacement;
                behind = false;
            }
            msg = rx.recv() => match msg {
                Ok(frame @ (ServerMsg::RowDamage { .. } | ServerMsg::PaneRows { .. })) => {
                    if behind {
                        continue;
                    }
                    let frame = if runs { Some(frame) } else { per_cell(frame) };
                    let Some(frame) = frame else {
                        behind = true;
                        continue;
                    };
                    match frames.try_send(frame) {
                        Ok(()) => {}
                        Err(mpsc::error::TrySendError::Full(_)) => behind = true,
                        Err(mpsc::error::TrySendError::Closed(_)) => return,
                    }
                }
                Ok(one_off) => {
                    if frames.send(one_off).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => behind = true,
                Err(broadcast::error::RecvError::Closed) => return,
            },
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
) -> Option<ShutdownReason> {
    match msg {
        ClientMsg::Restart => {
            let _ = out_tx.send(ServerMsg::Restarting);
            return Some(ShutdownReason::Restart);
        }
        ClientMsg::Stop => {
            let _ = out_tx.send(ServerMsg::Stopping);
            return Some(ShutdownReason::Stop);
        }
        // Answered whenever it comes. A client that greets reads on until
        // the answer, keeping the tree and the rest that went out first;
        // one that never greets is never sent a message it cannot read.
        ClientMsg::Hello(hello) => {
            tracing::info!(
                version = %hello.version,
                protocol = hello.protocol,
                "client greeted"
            );
            let _ = out_tx.send(ServerMsg::Hello(Hello::this_build()));
        }
        msg => {
            let result = dispatch::dispatch(msg, daemon, out_tx, subs, review_task, viewer);
            if let Err(e) = result {
                let _ = out_tx.send(ServerMsg::Error {
                    message: e.to_string(),
                });
            }
        }
    }
    None
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
/// There are two queues. Pane frames wait in a bounded one (see
/// [`FRAME_QUEUE`]); everything else — trees, replies, errors, acks — in
/// an unbounded one, and goes first, so a screen the client is slow to
/// take never holds up the answer to something it asked. Each queue is
/// written in the order it was filled: a pane's frames stay in order, as
/// do replies.
async fn writer_task<W>(
    wr: W,
    mut rx: mpsc::UnboundedReceiver<ServerMsg>,
    mut frames: mpsc::Receiver<ServerMsg>,
    control_flushed: broadcast::Sender<()>,
) where
    W: AsyncWrite + Unpin,
{
    let mut wr = tokio::io::BufWriter::new(wr);
    loop {
        let msg = tokio::select! {
            biased;
            Some(msg) = rx.recv() => msg,
            Some(frame) = frames.recv() => frame,
            else => break,
        };
        let mut contains_control = is_shutdown_ack(&msg);
        if write_frame(&mut wr, &msg).await.is_err() {
            break;
        }
        // Whatever else is already waiting rides along on this flush.
        let mut batched = 0;
        while batched < MAX_BATCHED_MESSAGES {
            let Ok(msg) = rx.try_recv().or_else(|_| frames.try_recv()) else {
                break;
            };
            contains_control |= is_shutdown_ack(&msg);
            if write_frame(&mut wr, &msg).await.is_err() {
                return;
            }
            batched += 1;
        }
        if tokio::io::AsyncWriteExt::flush(&mut wr).await.is_err() {
            break;
        }
        if contains_control {
            let _ = control_flushed.send(());
        }
    }
}

fn is_shutdown_ack(msg: &ServerMsg) -> bool {
    matches!(msg, ServerMsg::Restarting | ServerMsg::Stopping)
}

/// How much of the queue one flush may carry. A cap rather than the whole
/// backlog so a client that has been away for a while still starts seeing
/// frames while the rest is still going out.
const MAX_BATCHED_MESSAGES: usize = 256;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigFile, ProjectConfig};
    use argus_protocol::{read_msg, CheckoutId, PaneId, ReviewBase};

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

        let (_frames_tx, frames_rx) = mpsc::channel(1);
        let (control_flushed, _) = broadcast::channel(1);
        tokio::spawn(writer_task(client, rx, frames_rx, control_flushed));

        for i in 0..MAX_BATCHED_MESSAGES * 2 {
            let msg: ServerMsg = read_msg(&mut daemon).await.expect("a framed message");
            assert!(
                matches!(msg, ServerMsg::Error { ref message } if *message == i.to_string()),
                "message {i} arrived out of order: {msg:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_stop_ack_is_signaled_after_its_frame_is_flushed() {
        let (client, mut daemon) = tokio::io::duplex(1024 * 1024);
        let (tx, rx) = mpsc::unbounded_channel();
        let (_frames_tx, frames_rx) = mpsc::channel(1);
        let (control_flushed, mut flushed_rx) = broadcast::channel(1);
        tokio::spawn(writer_task(client, rx, frames_rx, control_flushed));

        tx.send(ServerMsg::Stopping).unwrap();
        assert!(matches!(
            read_msg::<_, ServerMsg>(&mut daemon).await.unwrap(),
            ServerMsg::Stopping
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
        let handler = tokio::spawn(handle(
            server,
            daemon,
            shutdown.clone(),
            shutdown.subscribe(),
        ));
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
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), shutdown_rx.recv())
            .await
            .expect("the daemon should signal shutdown after the acknowledgement")
                .expect("the shutdown sender should remain available"),
            ShutdownReason::Restart
        );
        handler.await.unwrap();
    }

    /// A connection to a fresh daemon, past its three opening messages.
    async fn connected(dir: &std::path::Path) -> tokio::io::DuplexStream {
        connect_to(Harness::new(dir).daemon).await
    }

    /// A connection to `daemon`, past its three opening messages.
    async fn connect_to(daemon: Arc<Daemon>) -> tokio::io::DuplexStream {
        let (mut client, server) = tokio::io::duplex(1024 * 1024);
        let (shutdown, _) = broadcast::channel(1);
        tokio::spawn(handle(server, daemon, shutdown.clone(), shutdown.subscribe()));
        for _ in 0..3 {
            let _: ServerMsg = read_msg(&mut client).await.unwrap();
        }
        client
    }

    #[tokio::test]
    async fn a_greeting_is_answered_with_this_builds_own() {
        let dir = tempfile::tempdir().unwrap();
        let mut client = connected(dir.path()).await;

        argus_protocol::write_msg(&mut client, &ClientMsg::Hello(Hello::this_build()))
            .await
            .unwrap();

        let answer: ServerMsg = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            read_msg(&mut client),
        )
        .await
        .expect("the greeting should be answered")
        .unwrap();
        match answer {
            ServerMsg::Hello(hello) => assert_eq!(hello, Hello::this_build()),
            other => panic!("expected a greeting, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn watching_a_conversation_sends_its_tail_then_what_is_written_after() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let daemon = Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                repos: vec![dir.path().to_string_lossy().to_string()],
                ..Default::default()
            }],
            agents: vec![crate::config::AgentConfig {
                name: "claude".to_string(),
                cmd: vec!["echo".to_string(), "hi".to_string()],
                env: Default::default(),
                harness: None,
                restart: Default::default(),
                live: false,
            }],
            harnesses: Vec::new(),
        });
        let checkout = daemon.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = daemon.spawn_agent(checkout, "claude").unwrap();
        let file = dir.path().join("s.jsonl");
        let line = |record: serde_json::Value| format!("{record}\n");
        std::fs::write(
            &file,
            line(serde_json::json!({"type": "user", "message": {"content": "hi there"}})),
        )
        .unwrap();
        daemon.report_transcript(pane, None, &file.to_string_lossy());
        let mut client = connect_to(daemon.clone()).await;

        argus_protocol::write_msg(&mut client, &ClientMsg::WatchTranscript { pane })
            .await
            .unwrap();
        let said = |updates: &[argus_protocol::Update]| -> Vec<String> {
            updates
                .iter()
                .filter_map(|u| match u {
                    argus_protocol::Update::Upsert(e) => match &e.body {
                        argus_protocol::Body::Prompt { text }
                        | argus_protocol::Body::Reply { text } => Some(text.clone()),
                        _ => None,
                    },
                    _ => None,
                })
                .collect()
        };
        let tail = loop {
            if let ServerMsg::Transcript { fresh: true, updates, .. } = next(&mut client).await {
                break updates;
            }
        };
        assert_eq!(said(&tail), ["hi there"]);

        std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap()
            .write_all(
                line(serde_json::json!({
                    "type": "assistant",
                    "message": {"content": [{"type": "text", "text": "hello"}]},
                }))
                .as_bytes(),
            )
            .unwrap();
        let more = loop {
            if let ServerMsg::Transcript { fresh: false, updates, .. } = next(&mut client).await {
                break updates;
            }
        };
        assert_eq!(said(&more), ["hello"]);

        argus_protocol::write_msg(&mut client, &ClientMsg::UnwatchTranscript { pane })
            .await
            .unwrap();
        let released = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while daemon.transcript_watched(pane) {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(released.is_ok(), "unwatching let go of the pane's reader");
        let _ = daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn a_client_that_follows_every_agent_is_sent_every_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let (daemon, pane) = daemon_with_a_pane(dir.path());
        let mut client = connect_to(daemon.clone()).await;
        argus_protocol::write_msg(
            &mut client,
            &ClientMsg::Hello(Hello::this_build().and(WIDE_TREE)),
        )
        .await
        .unwrap();
        assert!(matches!(next(&mut client).await, ServerMsg::Hello(_)));
        let ServerMsg::WideTree(workspaces) = next(&mut client).await else {
            panic!("a wide tree follows the greeting");
        };
        assert_eq!(workspaces.len(), 1);
        assert!(workspaces[0].open);

        // A change anywhere reaches it as a wide tree too, never a scoped one.
        let checkout = daemon.snapshot()[0].repositories[0].checkouts[0].id;
        let second = daemon.spawn_shell(checkout).unwrap();
        let ServerMsg::WideTree(workspaces) = next(&mut client).await else {
            panic!("changes arrive wide");
        };
        let panes = &workspaces[0].projects[0].repositories[0].checkouts[0].panes;
        assert_eq!(panes.len(), 2);
        assert!(panes[0].since.is_some());
        let _ = daemon.close_pane(pane);
        let _ = daemon.close_pane(second);
    }

    #[tokio::test]
    async fn the_terminal_client_is_never_sent_a_wide_tree() {
        assert!(!Hello::this_build().can(WIDE_TREE));
    }

    #[tokio::test]
    async fn a_client_that_reads_live_channels_turns_them_on_until_it_goes() {
        let dir = tempfile::tempdir().unwrap();
        let (daemon, pane) = daemon_with_a_pane(dir.path());
        let greet = |hello: Hello| {
            let daemon = daemon.clone();
            async move {
                let mut client = connect_to(daemon).await;
                argus_protocol::write_msg(&mut client, &ClientMsg::Hello(hello)).await.unwrap();
                assert!(matches!(next(&mut client).await, ServerMsg::Hello(_)));
                client
            }
        };

        let terminal = greet(Hello::this_build()).await;
        assert!(!daemon.live_is_read(), "the terminal client never turns them on");
        let web = greet(Hello::this_build().and(LIVE_CHANNELS)).await;
        assert!(daemon.live_is_read());

        drop(web);
        let off = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while daemon.live_is_read() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(off.is_ok(), "they go off with the client that read them");
        drop(terminal);
        let _ = daemon.close_pane(pane);
    }

    /// A daemon with one shell pane, started before any client connects so
    /// the opening tree already holds it and no later tree is on its way.
    fn daemon_with_a_pane(dir: &std::path::Path) -> (Arc<Daemon>, PaneId) {
        let daemon = Harness::new(dir).daemon;
        let checkout = daemon.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = daemon.spawn_shell(checkout).unwrap();
        (daemon, pane)
    }

    fn opus() -> argus_protocol::AgentTelemetry {
        argus_protocol::AgentTelemetry {
            model: Some("opus".into()),
            ..Default::default()
        }
    }

    async fn next(client: &mut tokio::io::DuplexStream) -> ServerMsg {
        tokio::time::timeout(std::time::Duration::from_secs(5), read_msg(client))
            .await
            .expect("a message should arrive")
            .unwrap()
    }

    #[tokio::test]
    async fn a_client_that_can_take_one_panes_telemetry_is_sent_just_that() {
        let dir = tempfile::tempdir().unwrap();
        let (daemon, pane) = daemon_with_a_pane(dir.path());
        let mut client = connect_to(daemon.clone()).await;
        argus_protocol::write_msg(&mut client, &ClientMsg::Hello(Hello::this_build()))
            .await
            .unwrap();
        assert!(matches!(next(&mut client).await, ServerMsg::Hello(_)));

        daemon.report_pane_telemetry(pane, None, opus());

        match next(&mut client).await {
            ServerMsg::PaneTelemetry { pane: id, telemetry } => {
                assert_eq!(id, pane);
                assert_eq!(telemetry.model.as_deref(), Some("opus"));
            }
            other => panic!("expected the one record, got {other:?}"),
        }
        let _ = daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn a_client_that_cannot_is_sent_the_whole_tree() {
        let dir = tempfile::tempdir().unwrap();
        let (daemon, pane) = daemon_with_a_pane(dir.path());
        let mut client = connect_to(daemon.clone()).await;

        daemon.report_pane_telemetry(pane, None, opus());

        match next(&mut client).await {
            ServerMsg::Tree(tree) => {
                let info = &tree[0].repositories[0].checkouts[0].panes[0];
                assert_eq!(info.telemetry.model.as_deref(), Some("opus"));
            }
            other => panic!("expected a tree, got {other:?}"),
        }
        let _ = daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn a_client_that_never_greets_is_never_sent_one() {
        // A client from before the handshake hangs up on a message it does
        // not know, so the answer goes only to a client that asked.
        let dir = tempfile::tempdir().unwrap();
        let mut client = connected(dir.path()).await;

        argus_protocol::write_msg(
            &mut client,
            &ClientMsg::Input {
                pane: PaneId(9999),
                bytes: b"x".to_vec(),
            },
        )
        .await
        .unwrap();

        let next: ServerMsg = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            read_msg(&mut client),
        )
        .await
        .expect("the input should be answered")
        .unwrap();
        assert!(matches!(next, ServerMsg::Error { .. }), "{next:?}");
    }

    #[tokio::test]
    async fn a_message_from_a_newer_client_is_skipped_not_fatal() {
        // A client newer than this daemon can send a request it has never
        // heard of. Dropping the connection for it cost the client
        // everything else it was doing.
        #[derive(serde::Serialize)]
        enum FromTheFuture {
            Teleport { to: u32 },
        }
        let dir = tempfile::tempdir().unwrap();
        let daemon = Harness::new(dir.path()).daemon;
        let (mut client, server) = tokio::io::duplex(1024 * 1024);
        let (shutdown, _) = broadcast::channel(1);
        tokio::spawn(handle(server, daemon, shutdown.clone(), shutdown.subscribe()));
        for _ in 0..3 {
            let _: ServerMsg = read_msg(&mut client).await.unwrap();
        }

        argus_protocol::write_msg(&mut client, &FromTheFuture::Teleport { to: 7 })
            .await
            .unwrap();
        argus_protocol::write_msg(
            &mut client,
            &ClientMsg::Input {
                pane: PaneId(9999),
                bytes: b"x".to_vec(),
            },
        )
        .await
        .unwrap();

        let answer = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                match read_msg::<_, ServerMsg>(&mut client).await {
                    Ok(ServerMsg::Error { message }) => return message,
                    Ok(_) => continue,
                    Err(error) => panic!("the connection broke: {error}"),
                }
            }
        })
        .await
        .expect("the request after the unknown one should be answered");
        assert!(answer.contains("pane"), "{answer}");
    }

    #[tokio::test]
    async fn a_frame_split_around_a_broadcast_still_arrives_whole() {
        // The message loop used to read frames inside its select, so a tree
        // landing while half a frame was in lost that half and misaligned
        // the stream. Over a slow link, half a frame is the usual case.
        let dir = tempfile::tempdir().unwrap();
        let daemon = Harness::new(dir.path()).daemon;
        let checkout = daemon.snapshot()[0].repositories[0].checkouts[0].id;
        let (mut client, server) = tokio::io::duplex(1024 * 1024);
        let (shutdown, _) = broadcast::channel(1);
        tokio::spawn(handle(
            server,
            daemon.clone(),
            shutdown.clone(),
            shutdown.subscribe(),
        ));
        for _ in 0..3 {
            let _: ServerMsg = read_msg(&mut client).await.unwrap();
        }

        let mut frame = Vec::new();
        write_frame(
            &mut frame,
            &ClientMsg::Input {
                pane: PaneId(9999),
                bytes: vec![b'x'; 4096],
            },
        )
        .await
        .unwrap();
        let (first, rest) = frame.split_at(frame.len() / 2);
        tokio::io::AsyncWriteExt::write_all(&mut client, first).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let shell = daemon.spawn_shell(checkout).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        tokio::io::AsyncWriteExt::write_all(&mut client, rest).await.unwrap();

        let answer = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                match read_msg::<_, ServerMsg>(&mut client).await {
                    Ok(ServerMsg::Error { message }) => return message,
                    Ok(_) => continue,
                    Err(error) => panic!("the connection broke: {error}"),
                }
            }
        })
        .await
        .expect("the input should be answered");
        assert!(answer.contains("pane"), "{answer}");

        let _ = daemon.close_pane(shell);
    }

    #[tokio::test]
    async fn a_stop_request_notifies_connected_clients_before_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = Harness::new(dir.path()).daemon;
        let (request_client, request_server) = tokio::io::duplex(1024 * 1024);
        let (other_client, other_server) = tokio::io::duplex(1024 * 1024);
        let (shutdown, mut shutdown_rx) = broadcast::channel(4);
        let request_handler = tokio::spawn(handle(
            request_server,
            daemon.clone(),
            shutdown.clone(),
            shutdown.subscribe(),
        ));
        let other_handler = tokio::spawn(handle(
            other_server,
            daemon,
            shutdown.clone(),
            shutdown.subscribe(),
        ));
        let mut request_client = request_client;
        let mut other_client = other_client;

        for _ in 0..3 {
            let _: ServerMsg = read_msg(&mut request_client).await.unwrap();
            let _: ServerMsg = read_msg(&mut other_client).await.unwrap();
        }
        argus_protocol::write_msg(&mut request_client, &ClientMsg::Stop)
            .await
            .unwrap();

        assert!(matches!(
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                read_msg::<_, ServerMsg>(&mut request_client),
            )
            .await
            .unwrap()
            .unwrap(),
            ServerMsg::Stopping
        ));
        assert_eq!(shutdown_rx.recv().await.unwrap(), ShutdownReason::Stop);
        assert!(matches!(
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                read_msg::<_, ServerMsg>(&mut other_client),
            )
            .await
            .unwrap()
            .unwrap(),
            ServerMsg::Stopping
        ));

        request_handler.await.unwrap();
        other_handler.await.unwrap();
    }

    struct Harness {
        daemon: Arc<Daemon>,
        tx: mpsc::UnboundedSender<ServerMsg>,
        rx: mpsc::UnboundedReceiver<ServerMsg>,
        frames: mpsc::Receiver<ServerMsg>,
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
            let (frames_tx, frames) = mpsc::channel(FRAME_QUEUE);
            let viewer = daemon.new_viewer();
            Harness {
                viewer,
                daemon,
                tx,
                rx,
                frames,
                subs: Subscriptions::new(frames_tx),
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
            .is_some()
        }

        /// Everything queued for the client: replies, then pane frames.
        fn replies(&mut self) -> Vec<ServerMsg> {
            let mut out = Vec::new();
            while let Ok(m) = self.rx.try_recv() {
                out.push(m);
            }
            while let Ok(m) = self.frames.try_recv() {
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
            request_id: 0,
        });
        let error = h.error().await;
        assert!(error.contains("no such checkout"), "{error}");
    }

    #[test]
    fn a_named_request_is_answered_with_what_it_made() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::AddProject {
            path: other.path().to_string_lossy().to_string(),
            request_id: 3,
        });

        let made = h.replies().into_iter().find_map(|reply| match reply {
            ServerMsg::Created {
                request_id: 3,
                created: Some(argus_protocol::Created::Project(id)),
            } => Some(id),
            _ => None,
        });
        let made = made.expect("the named request is answered");
        assert!(h.daemon.snapshot().iter().any(|p| p.id == made));
    }

    #[test]
    fn an_unnamed_request_is_not_answered() {
        // What a client from before named requests sends: it could not
        // read the answer, so it is never sent one.
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::AddProject {
            path: other.path().to_string_lossy().to_string(),
            request_id: 0,
        });
        assert!(!h
            .replies()
            .iter()
            .any(|reply| matches!(reply, ServerMsg::Created { .. })));
    }

    #[tokio::test]
    async fn a_refused_named_request_is_answered_with_nothing_and_still_reported() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::SpawnShell {
            checkout: CheckoutId(9999),
            request_id: 8,
        });
        let answer = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
            .await
            .expect("an answer should arrive");
        assert!(
            matches!(
                answer,
                Some(ServerMsg::Created {
                    request_id: 8,
                    created: None
                })
            ),
            "{answer:?}"
        );
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

    #[test]
    fn a_stop_message_is_acknowledged_without_dispatching_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());

        assert!(h.send(ClientMsg::Stop));
        assert!(matches!(h.replies().as_slice(), [ServerMsg::Stopping]));
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
                request_id: 0,
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

        dispatch::spawn_pane(&tx, 0, || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            Ok(PaneId(1))
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
            h.subs.tasks.contains_key(&pane),
            "damage must flow after a subscribe"
        );

        h.send(ClientMsg::Unsubscribe { pane });
        assert!(!h.subs.tasks.contains_key(&pane));

        let _ = h.daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn history_is_answered_in_the_form_the_client_reads() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        let pane = h.daemon.spawn_shell(h.checkout()).unwrap();

        h.send(ClientMsg::Scrollback {
            pane,
            offset: 1,
            top: None,
        });
        assert!(matches!(
            h.replies().first(),
            Some(ServerMsg::ScrollbackRows { .. })
        ));

        h.subs.runs = true;
        h.send(ClientMsg::Scrollback {
            pane,
            offset: 1,
            top: None,
        });
        assert!(matches!(
            h.replies().first(),
            Some(ServerMsg::ScrollbackRuns { rows, top: Some(_), .. }) if *rows > 0
        ), "and numbers its first row, for the next request to ask by");

        let _ = h.daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn subscribing_to_a_pane_that_is_gone_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::Subscribe { pane: PaneId(9999) });
        assert!(!h.error().await.is_empty());
        assert!(h.subs.tasks.is_empty());
    }

    #[tokio::test]
    async fn a_lagged_subscription_recovers_with_a_fresh_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::new(dir.path());
        let checkout = h.checkout();
        let pane = h.daemon.spawn_shell(checkout).unwrap();
        let (damage_tx, rx) = broadcast::channel(1);
        let (frames_tx, mut frames) = mpsc::channel(FRAME_QUEUE);

        for i in 0..2 {
            damage_tx.send(damage(pane, i)).unwrap();
        }

        let mut subs = Subscriptions::new(frames_tx);
        subs.add(pane, snapshot_marker(pane), rx, h.daemon.clone());

        assert!(is_snapshot_marker(&frames.recv().await.unwrap()));
        let recovered = tokio::time::timeout(std::time::Duration::from_secs(5), frames.recv())
            .await
            .expect("lag recovery should answer")
            .expect("frame queue should remain open");
        assert!(matches!(recovered, ServerMsg::PaneSnapshot { pane: id, .. } if id == pane));

        let _ = h.daemon.close_pane(pane);
    }

    /// A frame as the pump sends it, told apart from the others by its
    /// cursor row.
    fn damage(pane: PaneId, row: u16) -> ServerMsg {
        ServerMsg::RowDamage {
            pane,
            scroll: None,
            runs: Vec::new(),
            cursor: argus_protocol::Cursor {
                row,
                ..Default::default()
            },
            mouse: Default::default(),
            alternate_screen: false,
        }
    }

    fn scrolled(pane: PaneId) -> ServerMsg {
        ServerMsg::RowDamage {
            pane,
            scroll: Some(argus_protocol::Scroll {
                top: 0,
                bottom: 24,
                up: 1,
            }),
            runs: Vec::new(),
            cursor: Default::default(),
            mouse: Default::default(),
            alternate_screen: false,
        }
    }

    /// A snapshot no pane could have sent, so a real one is recognisable.
    fn snapshot_marker(pane: PaneId) -> ServerMsg {
        ServerMsg::PaneSnapshot {
            pane,
            rows: 0,
            cols: 0,
            cells: Vec::new(),
            cursor: Default::default(),
            mouse: Default::default(),
            alternate_screen: false,
        }
    }

    fn is_snapshot_marker(msg: &ServerMsg) -> bool {
        matches!(msg, ServerMsg::PaneSnapshot { rows: 0, .. })
    }

    #[tokio::test]
    async fn a_pane_that_falls_behind_a_full_queue_catches_up_with_one_fresh_snapshot() {
        // A client too slow to read has to end up on the latest screen,
        // not behind an ever-longer queue of old ones. A frame that finds
        // the queue full is dropped, a copy is not, and the first free
        // slot carries a fresh grid.
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::new(dir.path());
        let pane = h.daemon.spawn_shell(h.checkout()).unwrap();
        let (damage_tx, rx) = broadcast::channel(16);
        let (frames_tx, mut frames) = mpsc::channel(2);

        let mut subs = Subscriptions::new(frames_tx);
        subs.add(pane, snapshot_marker(pane), rx, h.daemon.clone());
        damage_tx.send(damage(pane, 1)).unwrap(); // fills the queue
        damage_tx.send(damage(pane, 2)).unwrap(); // finds it full
        damage_tx.send(damage(pane, 3)).unwrap(); // skipped while behind
        damage_tx
            .send(ServerMsg::Clipboard {
                pane,
                text: "copied".into(),
            })
            .unwrap();
        // The forwarder takes all four before the client reads a thing.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !damage_tx.is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the forwarder should drain the broadcast");

        let mut read = Vec::new();
        for _ in 0..4 {
            let next = tokio::time::timeout(std::time::Duration::from_secs(5), frames.recv())
                .await
                .expect("a frame should arrive")
                .expect("the frame queue should stay open");
            read.push(next);
        }

        assert!(is_snapshot_marker(&read[0]), "{:?}", read[0]);
        // A client that did not greet reads cells, so the runs arrive as
        // spans.
        assert!(
            matches!(read[1], ServerMsg::Damage { cursor, .. } if cursor.row == 1),
            "{:?}",
            read[1]
        );
        assert!(
            matches!(&read[2], ServerMsg::Clipboard { text, .. } if text == "copied"),
            "{:?}",
            read[2]
        );
        assert!(
            matches!(&read[3], ServerMsg::PaneSnapshot { rows, .. } if *rows > 0),
            "the pane's real grid, not frames 2 and 3: {:?}",
            read[3]
        );

        let _ = h.daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn a_reply_is_written_ahead_of_queued_frames() {
        // A screen the client is slow to take must not hold up the answer
        // to something it asked.
        let (writer_end, mut reader_end) = tokio::io::duplex(1024 * 1024);
        let (tx, rx) = mpsc::unbounded_channel();
        let (frames_tx, frames_rx) = mpsc::channel(4);
        for row in [1, 2] {
            frames_tx.send(damage(PaneId(1), row)).await.unwrap();
        }
        tx.send(ServerMsg::Error {
            message: "reply".into(),
        })
        .unwrap();

        let (control_flushed, _) = broadcast::channel(1);
        tokio::spawn(writer_task(writer_end, rx, frames_rx, control_flushed));

        let first: ServerMsg = read_msg(&mut reader_end).await.unwrap();
        assert!(matches!(first, ServerMsg::Error { .. }), "{first:?}");
        for row in [1, 2] {
            let frame: ServerMsg = read_msg(&mut reader_end).await.unwrap();
            assert!(
                matches!(frame, ServerMsg::RowDamage { cursor, .. } if cursor.row == row),
                "frames keep their order: {frame:?}"
            );
        }
    }

    /// The first `count` frames a subscription forwards.
    async fn forwarded(frames: &mut mpsc::Receiver<ServerMsg>, count: usize) -> Vec<ServerMsg> {
        let mut read = Vec::new();
        for _ in 0..count {
            let next = tokio::time::timeout(std::time::Duration::from_secs(5), frames.recv())
                .await
                .expect("a frame should arrive")
                .expect("the frame queue should stay open");
            read.push(next);
        }
        read
    }

    #[tokio::test]
    async fn a_client_that_reads_runs_is_sent_them_as_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::new(dir.path());
        let pane = h.daemon.spawn_shell(h.checkout()).unwrap();
        let (damage_tx, rx) = broadcast::channel(16);
        let (frames_tx, mut frames) = mpsc::channel(FRAME_QUEUE);

        let mut subs = Subscriptions::new(frames_tx);
        subs.runs = true;
        subs.add(pane, snapshot_marker(pane), rx, h.daemon.clone());
        damage_tx.send(scrolled(pane)).unwrap();

        let read = forwarded(&mut frames, 2).await;
        assert!(
            matches!(read[1], ServerMsg::RowDamage { scroll: Some(_), .. }),
            "{:?}",
            read[1]
        );
        let _ = h.daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn a_client_that_reads_cells_catches_up_on_a_scroll_with_a_fresh_grid() {
        // The per-cell form cannot say a scroll, so the client is sent the
        // grid the scroll left, which is what a scroll cost it before runs.
        let dir = tempfile::tempdir().unwrap();
        let h = Harness::new(dir.path());
        let pane = h.daemon.spawn_shell(h.checkout()).unwrap();
        let (damage_tx, rx) = broadcast::channel(16);
        let (frames_tx, mut frames) = mpsc::channel(FRAME_QUEUE);

        let mut subs = Subscriptions::new(frames_tx);
        subs.add(pane, snapshot_marker(pane), rx, h.daemon.clone());
        damage_tx.send(scrolled(pane)).unwrap();

        let read = forwarded(&mut frames, 2).await;
        assert!(
            matches!(&read[1], ServerMsg::PaneSnapshot { rows, .. } if *rows > 0),
            "{:?}",
            read[1]
        );
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
            request_id: 0,
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
        assert_eq!(h.subs.tasks.len(), 2);

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
        assert_eq!(h.subs.tasks.len(), 1, "only the one named is dropped");
        assert!(h.subs.tasks.contains_key(&b));

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
        assert_eq!(h.subs.tasks.len(), 1);

        let _ = h.daemon.close_pane(pane);
    }
}
