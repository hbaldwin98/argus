use std::sync::Arc;

use argus_protocol::{read_msg, write_msg, ClientMsg, ServerMsg};
use tokio::io::{split, AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc};

use crate::state::Daemon;

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

    let mut tree_rx = daemon.subscribe_tree();
    let mut workspaces_rx = daemon.subscribe_workspaces();
    let mut damage_rx: Option<broadcast::Receiver<ServerMsg>> = None;

    loop {
        tokio::select! {
            msg = read_msg::<_, ClientMsg>(&mut rd) => {
                match msg {
                    Ok(cmsg) => handle_client_msg(cmsg, &daemon, &out_tx, &mut damage_rx),
                    Err(_) => break,
                }
            }
            Ok(tree) = tree_rx.recv() => {
                let _ = out_tx.send(ServerMsg::Tree(tree));
            }
            Ok(ws) = workspaces_rx.recv() => {
                let _ = out_tx.send(ServerMsg::Workspaces(ws));
            }
            dmsg = recv_optional(&mut damage_rx) => {
                if let Some(dmsg) = dmsg {
                    let _ = out_tx.send(dmsg);
                }
            }
        }
    }
}

fn handle_client_msg(
    msg: ClientMsg,
    daemon: &Arc<Daemon>,
    out_tx: &mpsc::UnboundedSender<ServerMsg>,
    damage_rx: &mut Option<broadcast::Receiver<ServerMsg>>,
) {
    let result = match msg {
        ClientMsg::Subscribe { pane } => daemon.subscribe_pane(pane).map(|(rows, cols, cells, rx)| {
            *damage_rx = Some(rx);
            let _ = out_tx.send(ServerMsg::PaneSnapshot {
                pane,
                rows,
                cols,
                cells,
            });
        }),
        ClientMsg::Unsubscribe { .. } => {
            *damage_rx = None;
            Ok(())
        }
        ClientMsg::Input { pane, bytes } => daemon.write_pane(pane, &bytes),
        ClientMsg::Resize { pane, rows, cols } => daemon.resize_pane(pane, rows, cols),
        ClientMsg::SpawnShell { checkout } => daemon.spawn_shell(checkout).map(|_| ()),
        ClientMsg::SpawnAgent { checkout, template } => {
            daemon.spawn_agent(checkout, &template).map(|_| ())
        }
        ClientMsg::Kill { pane } => daemon.close_pane(pane),
        ClientMsg::AddProject { path } => daemon.add_project(&path),
        // Both do real subprocess I/O (`git worktree add`/`remove`), so they
        // run on their own task instead of blocking this connection's
        // message loop — a slow worktree op must not stall keystrokes going
        // to some other pane. Each reports its own error asynchronously.
        ClientMsg::CreateWorktree { checkout, branch } => {
            let daemon = daemon.clone();
            let out_tx = out_tx.clone();
            tokio::spawn(async move {
                if let Err(e) = daemon.create_worktree(checkout, branch).await {
                    let _ = out_tx.send(ServerMsg::Error { message: e.to_string() });
                }
            });
            Ok(())
        }
        ClientMsg::RemoveCheckout { checkout } => {
            let daemon = daemon.clone();
            let out_tx = out_tx.clone();
            tokio::spawn(async move {
                if let Err(e) = daemon.remove_checkout(checkout).await {
                    let _ = out_tx.send(ServerMsg::Error { message: e.to_string() });
                }
            });
            Ok(())
        }
        ClientMsg::OpenWorkspace { workspace } => daemon.open_workspace(workspace),
        // Filesystem work, so off the message loop like the two above.
        ClientMsg::OpenInEditor { checkout, path, line } => {
            daemon.spawn_editor(checkout, &path, line).map(|_| ())
        }
        ClientMsg::Review { checkout, base } => daemon.checkout_path(checkout).map(|path| {
            let out_tx = out_tx.clone();
            tokio::task::spawn_blocking(move || {
                let _ = out_tx.send(ServerMsg::Review(argus_protocol::Review {
                    checkout,
                    base,
                    files: crate::diff::working_tree(&path, base),
                }));
            });
        }),
    };
    if let Err(e) = result {
        let _ = out_tx.send(ServerMsg::Error {
            message: e.to_string(),
        });
    }
}

async fn recv_optional(rx: &mut Option<broadcast::Receiver<ServerMsg>>) -> Option<ServerMsg> {
    let r = match rx.as_mut() {
        Some(r) => r,
        None => return std::future::pending().await,
    };
    match r.recv().await {
        Ok(m) => Some(m),
        Err(broadcast::error::RecvError::Lagged(_)) => None,
        Err(broadcast::error::RecvError::Closed) => {
            *rx = None;
            None
        }
    }
}

async fn writer_task<W>(mut wr: W, mut rx: mpsc::UnboundedReceiver<ServerMsg>)
where
    W: AsyncWrite + Unpin,
{
    while let Some(msg) = rx.recv().await {
        if write_msg(&mut wr, &msg).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigFile, ProjectConfig};
    use argus_protocol::{CheckoutId, PaneId, ReviewBase};

    struct Harness {
        daemon: Arc<Daemon>,
        tx: mpsc::UnboundedSender<ServerMsg>,
        rx: mpsc::UnboundedReceiver<ServerMsg>,
        damage: Option<broadcast::Receiver<ServerMsg>>,
    }

    impl Harness {
        fn new(repo: &std::path::Path) -> Self {
            let daemon = Daemon::new(ConfigFile {
                workspaces: Vec::new(),
                projects: vec![ProjectConfig {
                    name: "proj".to_string(),
                    repos: vec![repo.to_string_lossy().to_string()],
                    workspace: None,
                }],
                agents: Vec::new(),
            });
            let (tx, rx) = mpsc::unbounded_channel();
            Harness {
                daemon,
                tx,
                rx,
                damage: None,
            }
        }

        fn checkout(&self) -> CheckoutId {
            self.daemon.snapshot()[0].checkouts[0].id
        }

        fn send(&mut self, msg: ClientMsg) {
            handle_client_msg(msg, &self.daemon, &self.tx, &mut self.damage);
        }

        fn replies(&mut self) -> Vec<ServerMsg> {
            let mut out = Vec::new();
            while let Ok(m) = self.rx.try_recv() {
                out.push(m);
            }
            out
        }

        fn error(&mut self) -> String {
            match self.replies().into_iter().next() {
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
        assert!(h.error().contains("no such checkout"), "{}", h.error());
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
        assert!(h.damage.is_some(), "damage must flow after a subscribe");

        h.send(ClientMsg::Unsubscribe { pane });
        assert!(h.damage.is_none());

        let _ = h.daemon.close_pane(pane);
    }

    #[tokio::test]
    async fn subscribing_to_a_pane_that_is_gone_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::Subscribe { pane: PaneId(9999) });
        assert!(!h.error().is_empty());
        assert!(h.damage.is_none());
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
            checkout,
            base: ReviewBase::WorkingTree,
        });

        // The diff runs on a blocking thread, so the reply is not immediate.
        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), h.rx.recv())
            .await
            .expect("the diff should arrive")
            .expect("channel open");
        match reply {
            ServerMsg::Review(r) => {
                assert_eq!(r.checkout, checkout);
                assert_eq!(r.base, ReviewBase::WorkingTree);
                assert_eq!(r.files[0].path, "a.txt");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_review_of_a_checkout_that_is_gone_errors_without_spawning_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::Review {
            checkout: CheckoutId(9999),
            base: ReviewBase::WorkingTree,
        });
        assert!(h.error().contains("no such checkout"));
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
        });
        assert!(h.error().contains("inside the checkout"), "{}", h.error());
    }

    #[tokio::test]
    async fn switching_to_a_workspace_that_does_not_exist_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = Harness::new(dir.path());
        h.send(ClientMsg::OpenWorkspace {
            workspace: argus_protocol::WorkspaceId(9999),
        });
        assert!(!h.error().is_empty());
    }
}
