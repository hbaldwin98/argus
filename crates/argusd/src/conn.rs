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

static REVIEW_PERMIT: Semaphore = Semaphore::const_new(1);

pub async fn handle<S>(stream: S, daemon: Arc<Daemon>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut rd, wr) = split(stream);
    let (out_tx, out_rx) = mpsc::unbounded_channel::<ServerMsg>();

    tokio::spawn(writer_task(wr, out_rx));

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
    let mut subs = Subscriptions::default();
    let mut review_task = None;

    loop {
        tokio::select! {
            msg = read_msg::<_, ClientMsg>(&mut rd) => {
                match msg {
                    Ok(cmsg) => handle_client_msg(
                        cmsg,
                        &daemon,
                        &out_tx,
                        &mut subs,
                        &mut review_task,
                        viewer,
                    ),
                    Err(_) => break,
                }
            }
            Ok(tree) = tree_rx.recv() => {
                let _ = out_tx.send(ServerMsg::Tree(tree));
            }
            Ok(ws) = workspaces_rx.recv() => {
                let _ = out_tx.send(ServerMsg::Workspaces(ws));
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
) {
    let result = dispatch_pane(msg, daemon, out_tx, subs, viewer)
        .or_else(|msg| dispatch_notes(msg, daemon, out_tx))
        .or_else(|msg| dispatch_workspace(msg, daemon, out_tx))
        .or_else(|msg| dispatch_branch_or_editor(msg, daemon, out_tx))
        .or_else(|msg| dispatch_review(msg, daemon, out_tx, review_task))
        .unwrap_or_else(|_| unreachable!("every client message is dispatched"));
    if let Err(e) = result {
        let _ = out_tx.send(ServerMsg::Error {
            message: e.to_string(),
        });
    }
}

type DispatchResult = Result<anyhow::Result<()>, ClientMsg>;

fn dispatch_pane(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    subs: &mut Subscriptions,
    viewer: ViewerId,
) -> DispatchResult {
    let result = match msg {
        ClientMsg::Subscribe { pane } => daemon.subscribe_pane(pane).map(
            |(rows, cols, cells, cursor, mouse, alternate_screen, rx)| {
                subs.add(pane, rx, out_tx.clone(), daemon.clone());
                let _ = out_tx.send(ServerMsg::PaneSnapshot {
                    pane,
                    rows,
                    cols,
                    cells,
                    cursor,
                    mouse,
                    alternate_screen,
                });
            },
        ),
        ClientMsg::Unsubscribe { pane } => {
            subs.remove(pane);
            daemon.release_pane_size(viewer, pane);
            Ok(())
        }
        ClientMsg::Input { pane, bytes } => daemon.write_pane(pane, &bytes),
        ClientMsg::Paste { pane, text } => daemon.paste_pane(pane, &text),
        ClientMsg::Resize { pane, rows, cols } => daemon.resize_pane(viewer, pane, rows, cols),
        ClientMsg::Scrollback { pane, offset } => {
            daemon
                .pane_scrollback(pane, offset as usize)
                .map(|(cells, offset, depth)| {
                    let _ = out_tx.send(ServerMsg::ScrollbackRows {
                        pane,
                        offset: offset as u32,
                        depth: depth as u32,
                        cells,
                    });
                })
        }
        ClientMsg::SpawnShell { checkout } => {
            let daemon = daemon.clone();
            spawn_pane(out_tx, move || daemon.spawn_shell(checkout).map(|_| ()))
        }
        ClientMsg::SpawnAgent { checkout, template } => {
            let daemon = daemon.clone();
            spawn_pane(out_tx, move || {
                daemon.spawn_agent(checkout, &template).map(|_| ())
            })
        }
        ClientMsg::Kill { pane } => daemon.close_pane(pane),
        msg => return Err(msg),
    };
    Ok(result)
}

/// A write answers with the stored note rather than an acknowledgement, so
/// the client's editor shows what the daemon holds instead of what it
/// guessed its own write would produce.
fn dispatch_notes(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
) -> DispatchResult {
    let result = match msg {
        ClientMsg::GetNote { target } => daemon.note(target).map(|note| {
            let _ = out_tx.send(ServerMsg::Note(Box::new(note)));
        }),
        ClientMsg::SetNote { target, body } => {
            answer_note(out_tx, target, daemon.set_note(target, body))
        }
        ClientMsg::SetTodo {
            target,
            line,
            state,
        } => answer_note(out_tx, target, daemon.set_todo(target, line, state)),
        msg => return Err(msg),
    };
    Ok(result)
}

/// Never an `Err`: a refusal goes back as `NoteFailed`, which names the
/// note it was about, rather than as a bare `Error`.
fn answer_note(
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    target: argus_protocol::NoteTarget,
    written: anyhow::Result<argus_protocol::Note>,
) -> anyhow::Result<()> {
    let msg = match written {
        Ok(note) => ServerMsg::Note(Box::new(note)),
        Err(e) => ServerMsg::NoteFailed {
            target,
            message: e.to_string(),
        },
    };
    let _ = out_tx.send(msg);
    Ok(())
}

/// Runs a daemon call on its own task, reporting a refusal to the client
/// that asked for it.
///
/// Every git mutation has this shape: real subprocess I/O that must not
/// stall this connection's message loop, nothing to answer with when it
/// works, and a message worth showing when it does not.
fn spawn_reporting<F, Fut>(
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    work: F,
) -> anyhow::Result<()>
where
    F: FnOnce(Arc<Daemon>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = anyhow::Result<()>> + Send,
{
    let daemon = daemon.clone();
    let out_tx = out_tx.clone();
    tokio::spawn(async move {
        if let Err(error) = work(daemon).await {
            let _ = out_tx.send(ServerMsg::Error {
                message: error.to_string(),
            });
        }
    });
    Ok(())
}

fn spawn_pane(
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    spawn: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
) -> anyhow::Result<()> {
    let out_tx = out_tx.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(error) = spawn() {
            let _ = out_tx.send(ServerMsg::Error {
                message: error.to_string(),
            });
        }
    });
    Ok(())
}

fn dispatch_workspace(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
) -> DispatchResult {
    dispatch_workspace_query(msg, daemon, out_tx)
        .or_else(|msg| dispatch_worktree_change(msg, daemon, out_tx))
}

fn dispatch_workspace_query(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
) -> DispatchResult {
    let result = match msg {
        ClientMsg::AddProject { path } => daemon.add_project(&path),
        ClientMsg::AddRepository { project, path } => daemon.add_repository(project, &path),
        // Removal only rewrites config and the tree — no subprocess, no
        // directory walk — so it stays on the message loop like AddProject.
        ClientMsg::RemoveProject { project } => daemon.remove_project(project),
        ClientMsg::RemoveRepository { repository } => daemon.remove_repository(repository),
        ClientMsg::OpenWorkspace { workspace } => daemon.open_workspace(workspace),
        ClientMsg::CreateWorkspace { name } => daemon.create_workspace(&name),
        // Listing walks a working tree, so it goes off the message loop.
        ClientMsg::ListBranches { checkout } => {
            reply_with(daemon, out_tx, checkout, move |path| ServerMsg::Branches {
                checkout,
                branches: crate::browse::branches(&path),
            })
        }
        // Not tied to a checkout — the browser roams the whole filesystem —
        // so it cannot go through `reply_with`. Off the message loop all
        // the same: a directory on a cold network drive takes its time.
        ClientMsg::ListDirectories { request_id, path } => {
            let out_tx = out_tx.clone();
            tokio::task::spawn_blocking(move || {
                let mut listing = crate::browse::directories(&path);
                listing.request_id = request_id;
                let _ = out_tx.send(ServerMsg::Directories(listing));
            });
            Ok(())
        }
        ClientMsg::ListFiles { checkout } => {
            reply_with(daemon, out_tx, checkout, move |path| ServerMsg::Files {
                checkout,
                files: crate::browse::files(&path),
            })
        }
        ClientMsg::ListCommits {
            request_id,
            checkout,
        } => reply_with(
            daemon,
            out_tx,
            checkout,
            move |path| match crate::diff::list_commits(&path) {
                Ok(commits) => ServerMsg::Commits {
                    request_id,
                    checkout,
                    commits,
                },
                Err(error) => ServerMsg::CommitsFailed {
                    request_id,
                    checkout,
                    message: error.to_string(),
                },
            },
        ),
        ClientMsg::ListCommitFiles { checkout, commit } => reply_with(
            daemon,
            out_tx,
            checkout,
            move |path| match crate::diff::commit_summary(&path, &commit) {
                Ok(files) => ServerMsg::CommitFiles {
                    checkout,
                    commit,
                    files,
                },
                Err(error) => ServerMsg::CommitFilesFailed {
                    checkout,
                    commit,
                    message: error.to_string(),
                },
            },
        ),
        msg => return Err(msg),
    };
    Ok(result)
}

fn dispatch_worktree_change(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
) -> DispatchResult {
    let result = match msg {
        // Both do real subprocess I/O (`git worktree add`/`remove`), so they
        // run on their own task instead of blocking this connection's
        // message loop — a slow worktree op must not stall keystrokes going
        // to some other pane. Each reports its own error asynchronously.
        ClientMsg::CreateWorktree { checkout, branch } => {
            spawn_reporting(daemon, out_tx, move |d| async move {
                d.create_worktree(checkout, branch).await
            })
        }
        ClientMsg::RemoveCheckout { checkout } => {
            spawn_reporting(daemon, out_tx, move |d| async move {
                d.remove_checkout(checkout).await
            })
        }
        // Same reasoning: `git init` is a subprocess, and the directory it
        // lands in may not exist yet.
        ClientMsg::InitRepository { project, path } => {
            spawn_reporting(daemon, out_tx, move |d| async move {
                d.init_repository(project, &path).await
            })
        }
        msg => return Err(msg),
    };
    Ok(result)
}

fn dispatch_branch_or_editor(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
) -> DispatchResult {
    let result = match msg {
        ClientMsg::SwitchBranch { checkout, branch } => {
            spawn_reporting(daemon, out_tx, move |d| async move {
                d.switch_branch(checkout, &branch).await
            })
        }
        ClientMsg::CreateBranch { checkout, branch } => {
            spawn_reporting(daemon, out_tx, move |d| async move {
                d.create_branch(checkout, &branch).await
            })
        }
        ClientMsg::Fetch { checkout } => {
            spawn_reporting(
                daemon,
                out_tx,
                move |d| async move { d.fetch(checkout).await },
            )
        }
        ClientMsg::Pull { checkout } => {
            spawn_reporting(
                daemon,
                out_tx,
                move |d| async move { d.pull(checkout).await },
            )
        }
        // Not `spawn_reporting`: an unmerged branch comes back as a
        // question for the user rather than as an error, and only this
        // call has one to send.
        ClientMsg::DeleteBranch {
            checkout,
            branch,
            force,
        } => {
            let daemon = daemon.clone();
            let out_tx = out_tx.clone();
            tokio::spawn(async move {
                let msg = match daemon.delete_branch(checkout, &branch, force).await {
                    Ok(BranchDeletion::Deleted) => return,
                    Ok(BranchDeletion::NotMerged) => ServerMsg::BranchNotMerged { checkout, branch },
                    Err(error) => ServerMsg::Error {
                        message: error.to_string(),
                    },
                };
                let _ = out_tx.send(msg);
            });
            Ok(())
        }
        ClientMsg::OpenInEditor {
            checkout,
            path,
            line,
            external,
            command,
        } => {
            let daemon = daemon.clone();
            spawn_pane(out_tx, move || {
                daemon
                    .spawn_editor(checkout, &path, line, external, command.as_deref())
                    .map(|_| ())
            })
        }
        msg => return Err(msg),
    };
    Ok(result)
}

fn dispatch_review(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    review_task: &mut Option<tokio::task::JoinHandle<()>>,
) -> DispatchResult {
    let result = match msg {
        ClientMsg::Review {
            request_id,
            checkout,
            base,
            commit,
        } => daemon.checkout_path(checkout).map(|path| {
            if let Some(task) = review_task.take() {
                task.abort();
            }
            let out_tx = out_tx.clone();
            *review_task = Some(tokio::spawn(async move {
                let Ok(permit) = REVIEW_PERMIT.acquire().await else {
                    return;
                };
                let generated = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    match commit.as_deref() {
                        Some(rev) => crate::diff::generate_commit(&path, rev),
                        None => crate::diff::generate(&path, base),
                    }
                })
                .await;
                let message = match generated {
                    Ok(Ok(generated)) => ServerMsg::Review(argus_protocol::Review {
                        request_id,
                        checkout,
                        base: if generated.commit.is_some() {
                            argus_protocol::ReviewBase::Commit
                        } else {
                            base
                        },
                        files: generated.files,
                        commit: generated.commit,
                    }),
                    Ok(Err(error)) => ServerMsg::ReviewFailed {
                        request_id,
                        checkout,
                        message: error.to_string(),
                    },
                    Err(error) => ServerMsg::ReviewFailed {
                        request_id,
                        checkout,
                        message: error.to_string(),
                    },
                };
                let _ = out_tx.send(message);
            }));
        }),
        ClientMsg::ReviewComment {
            checkout,
            recipient,
            anchor,
            body,
        } => daemon
            .submit_review_comment(checkout, recipient, *anchor, body)
            .map(|(id, delivered)| {
                let _ = out_tx.send(ServerMsg::ReviewCommentSaved { id, delivered });
            }),
        msg => return Err(msg),
    };
    Ok(result)
}

/// Resolves a checkout to its path, then answers on a blocking thread.
fn reply_with(
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    checkout: argus_protocol::CheckoutId,
    build: impl FnOnce(std::path::PathBuf) -> ServerMsg + Send + 'static,
) -> anyhow::Result<()> {
    let path = daemon.checkout_path(checkout)?;
    let out_tx = out_tx.clone();
    tokio::task::spawn_blocking(move || {
        let _ = out_tx.send(build(path));
    });
    Ok(())
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
async fn writer_task<W>(wr: W, mut rx: mpsc::UnboundedReceiver<ServerMsg>)
where
    W: AsyncWrite + Unpin,
{
    let mut wr = tokio::io::BufWriter::new(wr);
    while let Some(msg) = rx.recv().await {
        if write_frame(&mut wr, &msg).await.is_err() {
            break;
        }
        // Whatever else is already waiting rides along on this flush.
        let mut batched = 0;
        while batched < MAX_BATCHED_MESSAGES {
            let Ok(msg) = rx.try_recv() else { break };
            if write_frame(&mut wr, &msg).await.is_err() {
                return;
            }
            batched += 1;
        }
        if tokio::io::AsyncWriteExt::flush(&mut wr).await.is_err() {
            break;
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

        tokio::spawn(writer_task(client, rx));

        for i in 0..MAX_BATCHED_MESSAGES * 2 {
            let msg: ServerMsg = read_msg(&mut daemon).await.expect("a framed message");
            assert!(
                matches!(msg, ServerMsg::Error { ref message } if *message == i.to_string()),
                "message {i} arrived out of order: {msg:?}"
            );
        }
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

        fn send(&mut self, msg: ClientMsg) {
            handle_client_msg(
                msg,
                &self.daemon,
                &self.tx,
                &mut self.subs,
                &mut self.review_task,
                self.viewer,
            );
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

        spawn_pane(&tx, || {
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
