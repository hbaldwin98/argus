//! One client connection: read a message, dispatch it, write what comes
//! back, and forward the topics it has joined.
//!
//! Anything dispatch does that is real I/O — a git subprocess, a directory
//! walk, a diff — leaves the message loop for a task of its own, because a
//! slow answer for one client must never delay a keystroke going to some
//! other pane.

use std::sync::Arc;

use argus_protocol::{
    grid_from_runs, read_known_msg, write_frame, CellRun, ClientMsg, Hello, PaneId, ServerMsg,
    CELL_RUNS, CONSOLES, EXTENSIONS, EXTENSION_DIALOGS, SAVED_VIEWS,
};
use tokio::io::{split, AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc};

use crate::review_queue::ReviewLane;
use crate::state::{BranchDeletion, Daemon, ViewerId};

mod dispatch;
mod topics;

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

    // Built first, for the topics it joins; from here on, every way out of
    // the loop passes the releases below.
    let mut conn = Connection::new(&daemon, frames_tx, out_tx.clone());
    let viewer = conn.viewer;
    conn.topics.join(topics::Tree { peer: conn.peer.clone() });
    let _ = out_tx.send(ServerMsg::Templates(daemon.template_names()));
    conn.topics.join(topics::Workspaces);
    conn.topics.join(topics::Boards);

    loop {
        tokio::select! {
            // A shutdown first, then the client; what is pushed goes on
            // tasks of its own.
            biased;
            reason = shutdown_rx.recv() => {
                // Nothing is pushed after the notice.
                conn.topics.stop();
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
                let outcome = dispatch::dispatch(cmsg, &daemon, &out_tx, &mut conn);
                if let dispatch::Outcome::Shutdown(reason) = outcome {
                    // The writer owns the other end of this signal and
                    // sends it only after the acknowledgement frame is
                    // flushed to the client.
                    let _ = control_flushed_rx.recv().await;
                    let _ = shutdown.send(reason);
                    break;
                }
            }
        }
    }
    daemon.release_viewer(viewer);
    daemon.forget_extension_viewer(viewer);
    reader.abort();
}

/// What one connection holds for itself, which dispatch reads and writes
/// as well as the loop.
struct Connection {
    /// This connection's identity for as long as it lasts: the daemon
    /// reconciles one pty size out of what every attached client asks
    /// for, so its requests have to be told apart from the other clients'.
    viewer: ViewerId,
    subs: Subscriptions,
    reviews: ReviewLane<dispatch::Generated>,
    /// What the client is pushed: the tree, the workspaces and the boards
    /// from the start, and the rest once it asks or greets for them.
    topics: topics::Topics,
    peer: Peer,
}

impl Connection {
    fn new(daemon: &Arc<Daemon>, frames: mpsc::Sender<ServerMsg>, out: mpsc::UnboundedSender<ServerMsg>) -> Self {
        Connection {
            viewer: daemon.new_viewer(),
            subs: Subscriptions::new(frames),
            reviews: daemon.review_lane(),
            topics: topics::Topics::new(daemon.clone(), out),
            peer: Peer::default(),
        }
    }

    fn can(&self, capability: &str) -> bool {
        self.peer.can(capability)
    }
}

/// What the client said it can take: nothing until it greets, and nothing
/// for good from a client that never does. Shared with the tree topic,
/// whose messages depend on it and which joins before the greeting does.
#[derive(Clone, Default)]
pub(super) struct Peer(Arc<std::sync::RwLock<Option<Hello>>>);

impl Peer {
    pub(super) fn set(&self, hello: Hello) {
        *self.0.write().unwrap() = Some(hello);
    }

    pub(super) fn can(&self, capability: &str) -> bool {
        self.0.read().unwrap().as_ref().is_some_and(|hello| hello.can(capability))
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
}

impl Drop for Subscriptions {
    fn drop(&mut self) {
        for task in self.tasks.values() {
            task.abort();
        }
    }
}

/// A pane's whole grid as the message that carries it, in runs when the
/// client reads them, and the damage stream that continues it.
fn subscribe(
    daemon: &Daemon,
    pane: PaneId,
    runs: bool,
) -> anyhow::Result<(ServerMsg, broadcast::Receiver<ServerMsg>)> {
    let (rows, cols, cells, cursor, mouse, alternate_screen, keyboard, rx) = daemon.subscribe_pane(pane)?;
    let snapshot = if runs {
        ServerMsg::PaneRows {
            pane,
            rows,
            cols,
            runs: argus_protocol::grid_runs(&cells),
            cursor,
            mouse,
            alternate_screen,
            keyboard,
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
            keyboard,
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
            keyboard,
        } => Some(ServerMsg::PaneSnapshot {
            pane,
            rows,
            cols,
            cells: grid_from_runs(rows, cols, &runs),
            cursor,
            mouse,
            alternate_screen,
            keyboard,
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
            keyboard,
        } => Some(ServerMsg::Damage {
            pane,
            spans: runs.iter().map(CellRun::span).collect(),
            cursor,
            mouse,
            alternate_screen,
            keyboard,
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
    use argus_protocol::{read_msg, CheckoutId, PaneId};

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
    async fn a_connection_opens_with_the_tree_the_templates_and_the_workspaces_in_that_order() {
        let dir = tempfile::tempdir().unwrap();
        let (mut client, server) = tokio::io::duplex(1024 * 1024);
        let (shutdown, _) = broadcast::channel(1);
        tokio::spawn(handle(server, Harness::new(dir.path()).daemon, shutdown.clone(), shutdown.subscribe()));

        assert!(matches!(next(&mut client).await, ServerMsg::Tree(_)));
        assert!(matches!(next(&mut client).await, ServerMsg::Templates(_)));
        assert!(matches!(next(&mut client).await, ServerMsg::Workspaces(_)));
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

    /// The next focus a watcher is sent, past the trees around it.
    async fn next_focus(client: &mut tokio::io::DuplexStream) -> Option<argus_protocol::Focus> {
        loop {
            if let ServerMsg::Focus(focus) = next(client).await {
                return focus.map(|focus| *focus);
            }
        }
    }

    #[tokio::test]
    async fn a_watcher_is_told_where_the_person_is_and_when_that_goes() {
        let dir = tempfile::tempdir().unwrap();
        let (daemon, pane) = daemon_with_a_pane(dir.path());
        let mut watcher = connect_to(daemon.clone()).await;
        argus_protocol::write_msg(&mut watcher, &ClientMsg::WatchFocus).await.unwrap();
        assert_eq!(next_focus(&mut watcher).await, None, "nobody has said where they are");

        let mut terminal = connect_to(daemon.clone()).await;
        let view = argus_protocol::View {
            pane: Some(pane),
            active: true,
            ..Default::default()
        };
        argus_protocol::write_msg(&mut terminal, &ClientMsg::View(view.clone())).await.unwrap();
        let focus = next_focus(&mut watcher).await.expect("the terminal's view");
        assert_eq!(focus.pane.map(|p| p.id), Some(pane));
        assert!(focus.checkout.is_some());

        // The same view again is not news.
        argus_protocol::write_msg(&mut terminal, &ClientMsg::View(view)).await.unwrap();
        drop(terminal);
        assert_eq!(next_focus(&mut watcher).await, None, "its client went");
        let _ = daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn a_client_that_does_not_watch_is_never_sent_the_focus() {
        let dir = tempfile::tempdir().unwrap();
        let (daemon, pane) = daemon_with_a_pane(dir.path());
        let mut terminal = connect_to(daemon.clone()).await;
        let view = argus_protocol::View {
            pane: Some(pane),
            active: true,
            ..Default::default()
        };
        argus_protocol::write_msg(&mut terminal, &ClientMsg::View(view)).await.unwrap();
        let taken = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while daemon.focus_in(&daemon.snapshot()).is_none() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(taken.is_ok(), "the daemon should take the view");
        // Anything at all, to have something arrive after the view.
        let checkout = daemon.snapshot()[0].repositories[0].checkouts[0].id;
        let second = daemon.spawn_shell(checkout).unwrap();
        assert!(matches!(next(&mut terminal).await, ServerMsg::Tree(_)));
        let _ = daemon.close_pane(pane);
        let _ = daemon.close_pane(second);
    }

    /// A daemon with one shell pane, started before any client connects so
    /// the opening tree already holds it and no later tree is on its way.
    pub(super) fn daemon_with_a_pane(dir: &std::path::Path) -> (Arc<Daemon>, PaneId) {
        let daemon = Harness::new(dir).daemon;
        let checkout = daemon.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = daemon.spawn_shell(checkout).unwrap();
        (daemon, pane)
    }

    /// A daemon with one agent pane that stays running.
    pub(super) fn daemon_with_an_agent(dir: &std::path::Path) -> (Arc<Daemon>, PaneId) {
        let daemon = Daemon::new(ConfigFile {
            workspaces: Vec::new(),
            projects: vec![ProjectConfig {
                name: "proj".to_string(),
                root: None,
                repos: vec![dir.to_string_lossy().to_string()],
                workspace: None,
                ..Default::default()
            }],
            agents: vec![crate::config::AgentConfig {
                name: "agent".to_string(),
                cmd: if cfg!(windows) {
                    vec!["cmd".to_string(), "/K".to_string()]
                } else {
                    vec!["cat".to_string()]
                },
                env: Default::default(),
                harness: None,
                restart: Default::default(),
            }],
            harnesses: Vec::new(),
        });
        let checkout = daemon.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = daemon.spawn_agent(checkout, "agent").unwrap();
        (daemon, pane)
    }

    /// A comment on one added line to `recipient`, named `request_id`.
    pub(super) fn review_comment(checkout: CheckoutId, recipient: PaneId, request_id: u64) -> ClientMsg {
        let anchor = argus_protocol::ReviewAnchor {
            base: argus_protocol::ReviewBase::Unstaged,
            commit: None,
            path: "a.rs".into(),
            old_path: None,
            old_start: None,
            old_end: None,
            new_start: Some(2),
            new_end: Some(2),
            text: vec!["+new".into()],
        };
        ClientMsg::ReviewComment { checkout, recipient, anchor: Box::new(anchor), body: "tidy".into(), request_id }
    }

    #[tokio::test]
    async fn a_saved_comment_is_answered_under_the_id_it_was_sent_with() {
        let dir = tempfile::tempdir().unwrap();
        let (daemon, pane) = daemon_with_an_agent(dir.path());
        let checkout = daemon.snapshot()[0].repositories[0].checkouts[0].id;
        let mut client = connect_to(daemon.clone()).await;
        // Zero is what a client from before comments were named sends.
        for request_id in [4, 0] {
            argus_protocol::write_msg(&mut client, &review_comment(checkout, pane, request_id)).await.unwrap();
            loop {
                match next(&mut client).await {
                    ServerMsg::ReviewCommentSaved { request_id: answered, .. } => {
                        assert_eq!(answered, request_id);
                        break;
                    }
                    ServerMsg::Error { message } => panic!("the comment was refused: {message}"),
                    _ => {}
                }
            }
        }
        let _ = daemon.close_pane(pane);
    }

    pub(super) fn view_change(id: &str) -> argus_protocol::ViewChange {
        argus_protocol::ViewChange::Show {
            view: argus_protocol::AgentView {
                id: id.into(),
                title: id.into(),
                ..Default::default()
            },
        }
    }

    /// The next view message, past the trees around it.
    async fn next_view(client: &mut tokio::io::DuplexStream) -> ServerMsg {
        loop {
            let msg = next(client).await;
            if matches!(msg, ServerMsg::ViewShown(_) | ServerMsg::ViewClosed { .. }) {
                return msg;
            }
        }
    }

    fn shown_id(msg: ServerMsg) -> String {
        match msg {
            ServerMsg::ViewShown(shown) => shown.view.id,
            other => panic!("expected a shown view, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_watcher_is_replayed_the_views_and_sent_each_change_after() {
        let dir = tempfile::tempdir().unwrap();
        let (daemon, pane) = daemon_with_an_agent(dir.path());
        for id in ["a", "b"] {
            daemon.change_view_for_agent(pane, view_change(id)).unwrap();
        }
        let mut watcher = connect_to(daemon.clone()).await;
        argus_protocol::write_msg(&mut watcher, &ClientMsg::WatchViews).await.unwrap();
        assert_eq!(shown_id(next_view(&mut watcher).await), "a");
        assert_eq!(shown_id(next_view(&mut watcher).await), "b");

        daemon.change_view_for_agent(pane, view_change("c")).unwrap();
        assert_eq!(shown_id(next_view(&mut watcher).await), "c");

        // The person closing one, from this client or another.
        let mut other = connect_to(daemon.clone()).await;
        let close = ClientMsg::CloseView { pane, id: "a".into() };
        argus_protocol::write_msg(&mut other, &close).await.unwrap();
        match next_view(&mut watcher).await {
            ServerMsg::ViewClosed { pane: p, id } => assert_eq!((p, id.as_str()), (pane, "a")),
            other => panic!("expected the close, got {other:?}"),
        }
        let _ = daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn a_client_that_does_not_watch_is_never_sent_a_view() {
        let dir = tempfile::tempdir().unwrap();
        let (daemon, pane) = daemon_with_an_agent(dir.path());
        let mut client = connect_to(daemon.clone()).await;
        daemon.change_view_for_agent(pane, view_change("a")).unwrap();
        // Anything at all, to have something arrive after the view.
        let checkout = daemon.snapshot()[0].repositories[0].checkouts[0].id;
        let second = daemon.spawn_shell(checkout).unwrap();
        loop {
            match next(&mut client).await {
                ServerMsg::ViewShown(_) | ServerMsg::ViewClosed { .. } => {
                    panic!("a view reached a client that never asked")
                }
                ServerMsg::Tree(tree) if tree.iter().any(|p| {
                    p.repositories.iter().flat_map(|r| &r.checkouts).flat_map(|c| &c.panes).any(|p| p.id == second)
                }) => break,
                _ => {}
            }
        }
        let _ = daemon.close_pane(pane);
        let _ = daemon.close_pane(second);
    }

    pub(super) fn opus() -> argus_protocol::AgentTelemetry {
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
        argus_protocol::write_msg(&mut client, &ClientMsg::Hello(Hello::this_build().without(EXTENSIONS)))
            .await
            .unwrap();
        assert!(matches!(next(&mut client).await, ServerMsg::Hello(_)));
        assert!(matches!(next(&mut client).await, ServerMsg::Consoles(_)));

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

    /// A client greeted as `hello`, past the consoles that follow the
    /// daemon's greeting.
    async fn greeted_as(daemon: &Arc<Daemon>, hello: Hello) -> tokio::io::DuplexStream {
        let mut client = connect_to(daemon.clone()).await;
        argus_protocol::write_msg(&mut client, &ClientMsg::Hello(hello)).await.unwrap();
        assert!(matches!(next(&mut client).await, ServerMsg::Hello(_)));
        assert!(matches!(next(&mut client).await, ServerMsg::Consoles(_)));
        client
    }

    const CONTAINERS: &str = r#"{"name": "containers", "title": "Containers", "columns": [{"title": "Name"}],
        "rows": [{"key": "web", "cells": [{"kind": "text", "text": "web"}]}]}"#;

    #[tokio::test]
    async fn a_client_that_takes_extensions_is_sent_their_views_on_greeting_and_on_each_change() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = Harness::new(dir.path()).daemon;
        daemon.admit_extension("docker");
        daemon.publish_views("docker", vec![serde_json::from_str(CONTAINERS).unwrap()]).unwrap();
        let mut client = greeted_as(&daemon, Hello::this_build()).await;
        assert!(matches!(next(&mut client).await, ServerMsg::ExtensionViews(views) if views.len() == 1));
        assert!(matches!(next(&mut client).await, ServerMsg::ExtensionCommands(commands) if commands.is_empty()));

        daemon.retire_extension("docker");

        assert!(matches!(next(&mut client).await, ServerMsg::ExtensionViews(views) if views.is_empty()));
    }

    #[tokio::test]
    async fn a_client_that_did_not_list_extensions_is_never_sent_their_messages() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = Harness::new(dir.path()).daemon;
        daemon.admit_extension("docker");
        let hello = Hello::this_build().without(EXTENSIONS).without(EXTENSION_DIALOGS);
        let mut client = greeted_as(&daemon, hello).await;
        daemon.extension_asker(daemon.new_viewer(), true);

        daemon.publish_views("docker", vec![serde_json::from_str(CONTAINERS).unwrap()]).unwrap();
        daemon.extension_notice("docker", "hello").unwrap();
        daemon.raise_dialog("docker", argus_protocol::ExtDialog {
            title: "t".into(),
            kind: argus_protocol::ExtDialogKind::Prompt { message: "m".into(), initial: String::new() },
            answers: None,
        })
        .unwrap();
        let checkout = daemon.snapshot()[0].repositories[0].checkouts[0].id;
        let pane = daemon.spawn_shell(checkout).unwrap();

        // The tree the shell sends is the first thing to arrive after all
        // of that: nothing of the extension's went before it.
        assert!(matches!(next(&mut client).await, ServerMsg::Tree(_)), "only the tree");
        let _ = daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn a_dialog_goes_only_to_the_client_it_was_put_to() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = Harness::new(dir.path()).daemon;
        daemon.admit_extension("docker");
        let mut first = greeted_as(&daemon, Hello::this_build()).await;
        let mut second = greeted_as(&daemon, Hello::this_build()).await;
        for client in [&mut first, &mut second] {
            assert!(matches!(next(client).await, ServerMsg::ExtensionViews(_)));
            assert!(matches!(next(client).await, ServerMsg::ExtensionCommands(_)));
        }
        // The second says the person is there.
        argus_protocol::write_msg(&mut second, &ClientMsg::View(argus_protocol::View { active: true, ..Default::default() }))
            .await
            .unwrap();
        for _ in 0..100 {
            if daemon.focus_in(&daemon.snapshot()).is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let id = daemon.raise_dialog("docker", argus_protocol::ExtDialog {
            title: "t".into(),
            kind: argus_protocol::ExtDialogKind::Confirm { message: "m".into(), yes: None },
            answers: None,
        })
        .unwrap();
        daemon.extension_notice("docker", "after").unwrap();

        assert!(matches!(next(&mut second).await, ServerMsg::ExtensionDialog { id: got, prompted: false, .. } if got == id));
        assert!(
            matches!(next(&mut first).await, ServerMsg::ExtensionNotice { text, .. } if text == "after"),
            "the first is told nothing of the dialog"
        );
    }

    /// The consoles in the next message, which must be them.
    async fn next_consoles(client: &mut tokio::io::DuplexStream) -> Vec<argus_protocol::PaneInfo> {
        match next(client).await {
            ServerMsg::Consoles(consoles) => consoles,
            other => panic!("expected the consoles, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_client_that_takes_consoles_is_sent_them_after_each_tree() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = Harness::new(dir.path()).daemon;
        let mut client = connect_to(daemon.clone()).await;
        argus_protocol::write_msg(&mut client, &ClientMsg::Hello(Hello::this_build().without(EXTENSIONS)))
            .await
            .unwrap();
        assert!(matches!(next(&mut client).await, ServerMsg::Hello(_)));
        assert!(next_consoles(&mut client).await.is_empty(), "none started yet");

        let console = daemon.spawn_console().unwrap();
        assert!(matches!(next(&mut client).await, ServerMsg::Tree(_)));
        let consoles = next_consoles(&mut client).await;
        assert_eq!(consoles.iter().map(|c| (c.id, c.kind)).collect::<Vec<_>>(), [(console, argus_protocol::PaneKind::Shell)]);

        daemon.close_pane(console).unwrap();
        assert!(matches!(next(&mut client).await, ServerMsg::Tree(_)));
        assert!(next_consoles(&mut client).await.is_empty(), "closing takes it out");
    }

    #[tokio::test]
    async fn a_client_that_does_not_take_consoles_is_never_sent_them() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = Harness::new(dir.path()).daemon;
        let mut client = connect_to(daemon.clone()).await;
        let older = Hello {
            capabilities: Vec::new(),
            ..Hello::this_build()
        };
        argus_protocol::write_msg(&mut client, &ClientMsg::Hello(older)).await.unwrap();
        assert!(matches!(next(&mut client).await, ServerMsg::Hello(_)));

        let console = daemon.spawn_console().unwrap();
        daemon.close_pane(console).unwrap();
        assert!(matches!(next(&mut client).await, ServerMsg::Tree(_)));
        assert!(matches!(next(&mut client).await, ServerMsg::Tree(_)), "no consoles between the trees");
    }

    #[tokio::test]
    async fn a_console_belongs_to_no_checkout_but_is_a_pane_like_any_other() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = Harness::new(dir.path()).daemon;
        let console = daemon.spawn_console().unwrap();

        let in_tree = daemon
            .snapshot()
            .iter()
            .flat_map(|p| &p.repositories)
            .flat_map(|r| &r.checkouts)
            .any(|c| c.panes.iter().any(|p| p.id == console));
        assert!(!in_tree, "a console is in no checkout");
        assert!(daemon.subscribe_pane(console).is_ok(), "its screen can be watched");
        assert!(daemon.write_pane(console, b"\r").is_ok(), "and typed into");

        daemon.close_pane(console).unwrap();
        assert!(daemon.consoles().is_empty());
        assert!(daemon.subscribe_pane(console).is_err(), "a closed console is gone");
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

    /// A connection without a socket: what `handle` holds for one client,
    /// fed messages the way its loop hands them on. Dispatch's tests send
    /// through it too.
    pub(super) struct Harness {
        pub(super) daemon: Arc<Daemon>,
        tx: mpsc::UnboundedSender<ServerMsg>,
        pub(super) rx: mpsc::UnboundedReceiver<ServerMsg>,
        frames: mpsc::Receiver<ServerMsg>,
        pub(super) conn: Connection,
    }

    impl Harness {
        pub(super) fn new(repo: &std::path::Path) -> Self {
            Self::with_harnesses(repo, Vec::new())
        }

        pub(super) fn with_harnesses(repo: &std::path::Path, harnesses: Vec<crate::config::HarnessConfig>) -> Self {
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
                harnesses,
            });
            let (tx, rx) = mpsc::unbounded_channel();
            let (frames_tx, frames) = mpsc::channel(FRAME_QUEUE);
            Harness {
                // The very sender dispatch is handed, so `replies` sees what
                // joining a topic queues.
                conn: Connection::new(&daemon, frames_tx, tx.clone()),
                daemon,
                tx,
                rx,
                frames,
            }
        }

        /// A harness whose client greeted listing `REQUEST_REFUSALS`.
        pub(super) fn refused_by_id(repo: &std::path::Path) -> Self {
            let mut h = Self::new(repo);
            h.send(ClientMsg::Hello(argus_protocol::Hello {
                capabilities: vec![argus_protocol::REQUEST_REFUSALS.into()],
                ..argus_protocol::Hello::this_build()
            }));
            h.replies();
            h
        }

        pub(super) fn checkout(&self) -> CheckoutId {
            self.daemon.snapshot()[0].repositories[0].checkouts[0].id
        }

        pub(super) fn send(&mut self, msg: ClientMsg) -> bool {
            dispatch::dispatch(msg, &self.daemon, &self.tx, &mut self.conn) != dispatch::Outcome::Continue
        }

        /// Everything queued for the client: replies, then pane frames.
        pub(super) fn replies(&mut self) -> Vec<ServerMsg> {
            let mut out = Vec::new();
            while let Ok(m) = self.rx.try_recv() {
                out.push(m);
            }
            while let Ok(m) = self.frames.try_recv() {
                out.push(m);
            }
            out
        }

        pub(super) async fn error(&mut self) -> String {
            let reply = tokio::time::timeout(std::time::Duration::from_secs(5), self.rx.recv())
                .await
                .expect("an error reply should arrive");
            match reply {
                Some(ServerMsg::Error { message }) => message,
                other => panic!("expected an error, got {other:?}"),
            }
        }

        /// The next refusal by id, waiting past anything else on the way.
        pub(super) async fn refusal(&mut self) -> argus_protocol::RequestRefusal {
            loop {
                let reply = tokio::time::timeout(std::time::Duration::from_secs(5), self.rx.recv())
                    .await
                    .expect("a refusal should arrive");
                if let Some(ServerMsg::Refused(refusal)) = reply {
                    return refusal;
                }
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
            keyboard: Default::default(),
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
            keyboard: Default::default(),
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
            keyboard: Default::default(),
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
}
