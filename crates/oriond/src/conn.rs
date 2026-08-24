use std::sync::Arc;

use orion_protocol::{read_msg, write_msg, ClientMsg, ServerMsg};
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

    let mut tree_rx = daemon.subscribe_tree();
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
