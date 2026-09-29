//! The one connection to the daemon, and what the server keeps of it for
//! the phones: the agent list, whether the daemon is there, and each
//! watched conversation's updates.
//!
//! Every phone shares this connection. A conversation two phones watch is
//! watched once, and a phone that starts watching makes the daemon send a
//! fresh tail, which the others take as a replacement for what they hold.
//! The connection is made again whenever it drops, the way the terminal
//! client makes it, except after the daemon says it is stopping: coming
//! back would start another, which is not what stopping asked for.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use argus_protocol::{
    read_known_msg, write_msg, AgentTelemetry, ClientMsg, Earlier, Hello, PaneId, ServerMsg,
    WorkspaceId, WorkspaceTree, WIDE_TREE,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc, watch};

use crate::phone::{self, ToPhone};

/// What a phone connection asks of the daemon connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    /// Start following a conversation, and have its tail sent fresh.
    Watch(u64),
    /// Stop following it, once nobody else is.
    Unwatch(u64),
    /// Have its tail sent fresh again, for a phone that fell behind.
    Refresh(u64),
    Earlier(u64, Earlier),
}

/// What every phone connection reads from: the latest of each whole-state
/// message, and a stream of conversation updates tagged by pane.
#[derive(Clone)]
pub struct Feeds {
    pub server: watch::Receiver<Arc<String>>,
    pub agents: watch::Receiver<Arc<String>>,
    pub conversations: broadcast::Sender<(u64, Arc<String>)>,
}

/// Why the connection loop ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// The daemon said it was stopping.
    Stopped,
}

struct Publish {
    server: watch::Sender<Arc<String>>,
    agents: watch::Sender<Arc<String>>,
    conversations: broadcast::Sender<(u64, Arc<String>)>,
}

impl Publish {
    fn server(&self, connected: bool, daemon: Option<&Hello>) {
        let ours = env!("CARGO_PKG_VERSION");
        let daemon_version = daemon
            .map(|hello| hello.version.clone())
            .filter(|version| version != ours);
        send(
            &self.server,
            &ToPhone::Server {
                version: ours.to_string(),
                connected,
                daemon_version,
            },
        );
    }

    fn agents(&self, tree: &[WorkspaceTree]) {
        send(
            &self.agents,
            &ToPhone::Agents {
                workspaces: phone::agents(tree),
            },
        );
    }

    fn conversation(&self, pane: u64, msg: &ToPhone) {
        if let Ok(json) = serde_json::to_string(msg) {
            let _ = self.conversations.send((pane, Arc::new(json)));
        }
    }
}

fn send(tx: &watch::Sender<Arc<String>>, msg: &ToPhone) {
    if let Ok(json) = serde_json::to_string(msg) {
        tx.send_replace(Arc::new(json));
    }
}

/// Starts the daemon connection: the feeds phones read, where they send
/// what they ask, and the task keeping the connection up.
pub fn start<C, F, S>(
    connect: C,
) -> (
    Feeds,
    mpsc::UnboundedSender<Ask>,
    tokio::task::JoinHandle<Ended>,
)
where
    C: Fn() -> F + Send + Sync + 'static,
    F: std::future::Future<Output = anyhow::Result<S>> + Send + 'static,
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let initial = |msg: &ToPhone| Arc::new(serde_json::to_string(msg).unwrap_or_default());
    let (server, server_rx) = watch::channel(initial(&ToPhone::Server {
        version: env!("CARGO_PKG_VERSION").to_string(),
        connected: false,
        daemon_version: None,
    }));
    let (agents, agents_rx) = watch::channel(initial(&ToPhone::Agents {
        workspaces: Vec::new(),
    }));
    let (conversations, _) = broadcast::channel(256);
    let publish = Publish {
        server,
        agents,
        conversations: conversations.clone(),
    };
    let (asks, asks_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(keep_connected(connect, publish, asks_rx));
    let feeds = Feeds {
        server: server_rx,
        agents: agents_rx,
        conversations,
    };
    (feeds, asks, task)
}

async fn keep_connected<C, F, S>(
    connect: C,
    publish: Publish,
    mut asks: mpsc::UnboundedReceiver<Ask>,
) -> Ended
where
    C: Fn() -> F,
    F: std::future::Future<Output = anyhow::Result<S>>,
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut watching: HashMap<u64, usize> = HashMap::new();
    let mut backoff = Duration::from_millis(250);
    loop {
        match connect().await {
            Ok(stream) => {
                backoff = Duration::from_millis(250);
                if session(stream, &publish, &mut asks, &mut watching).await == Some(Ended::Stopped)
                {
                    publish.server(false, None);
                    return Ended::Stopped;
                }
            }
            Err(error) => tracing::warn!("argus web could not reach argusd: {error:#}"),
        }
        publish.server(false, None);
        // Asks keep arriving while there is no daemon; the counts they
        // change are what the next connection watches again.
        let wait = tokio::time::sleep(backoff);
        tokio::pin!(wait);
        loop {
            tokio::select! {
                _ = &mut wait => break,
                ask = asks.recv() => match ask {
                    Some(ask) => { count(&mut watching, &ask); }
                    None => return Ended::Stopped,
                },
            }
        }
        backoff = (backoff * 2).min(Duration::from_secs(5));
    }
}

/// Applies an ask to the watch counts, saying what the daemon should be
/// sent for it, if anything.
fn count(watching: &mut HashMap<u64, usize>, ask: &Ask) -> Option<ClientMsg> {
    match *ask {
        Ask::Watch(pane) => {
            *watching.entry(pane).or_insert(0) += 1;
            Some(ClientMsg::WatchTranscript { pane: PaneId(pane) })
        }
        Ask::Unwatch(pane) => {
            let left = watching.get_mut(&pane).map(|n| {
                *n = n.saturating_sub(1);
                *n
            });
            match left {
                Some(0) => {
                    watching.remove(&pane);
                    Some(ClientMsg::UnwatchTranscript { pane: PaneId(pane) })
                }
                _ => None,
            }
        }
        Ask::Refresh(pane) => watching
            .contains_key(&pane)
            .then_some(ClientMsg::WatchTranscript { pane: PaneId(pane) }),
        Ask::Earlier(pane, before) => Some(ClientMsg::EarlierTranscript {
            pane: PaneId(pane),
            before,
        }),
    }
}

/// One connection's life: greet, watch again what phones are watching,
/// then carry messages until it drops. `Some(Stopped)` when the daemon
/// said it was stopping.
async fn session<S>(
    stream: S,
    publish: &Publish,
    asks: &mut mpsc::UnboundedReceiver<Ask>,
    watching: &mut HashMap<u64, usize>,
) -> Option<Ended>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut rd, mut wr) = tokio::io::split(stream);
    // Reading a frame is not cancellation-safe, so it gets a task of its
    // own rather than a branch of the select below.
    let (from_daemon, mut incoming) = mpsc::channel::<ServerMsg>(256);
    let reader = tokio::spawn(async move {
        while let Ok(msg) = read_known_msg::<_, ServerMsg>(&mut rd).await {
            if from_daemon.send(msg).await.is_err() {
                break;
            }
        }
    });

    let hello = Hello::this_build().and(WIDE_TREE);
    if write_msg(&mut wr, &ClientMsg::Hello(hello)).await.is_err() {
        reader.abort();
        return None;
    }
    for pane in watching.keys() {
        let _ = write_msg(&mut wr, &ClientMsg::WatchTranscript { pane: PaneId(*pane) }).await;
    }

    let mut state = State::default();
    let ended = loop {
        tokio::select! {
            msg = incoming.recv() => {
                let Some(msg) = msg else { break None };
                if let Some(ended) = state.take(msg, publish) {
                    break Some(ended);
                }
            }
            ask = asks.recv() => {
                let Some(ask) = ask else { break Some(Ended::Stopped) };
                if let Some(msg) = count(watching, &ask) {
                    if write_msg(&mut wr, &msg).await.is_err() {
                        break None;
                    }
                }
            }
        }
    };
    reader.abort();
    ended
}

/// What this connection has been told.
#[derive(Default)]
struct State {
    daemon: Option<Hello>,
    tree: Vec<WorkspaceTree>,
    /// Whether the daemon sends every workspace. One from before sends
    /// only the open workspace's tree, which stands in for all of them.
    wide: bool,
}

impl State {
    fn take(&mut self, msg: ServerMsg, publish: &Publish) -> Option<Ended> {
        match msg {
            ServerMsg::Hello(hello) => {
                publish.server(true, Some(&hello));
                self.daemon = Some(hello);
            }
            ServerMsg::WideTree(tree) => {
                self.wide = true;
                self.tree = tree;
                publish.agents(&self.tree);
            }
            ServerMsg::Tree(projects) if !self.wide => {
                if self.daemon.is_none() {
                    publish.server(true, None);
                }
                self.tree = vec![WorkspaceTree {
                    id: WorkspaceId(0),
                    name: "open workspace".to_string(),
                    open: true,
                    projects,
                }];
                publish.agents(&self.tree);
            }
            ServerMsg::PaneTelemetry { pane, telemetry } => {
                if self.merge_telemetry(pane, telemetry) {
                    publish.agents(&self.tree);
                }
            }
            ServerMsg::Transcript {
                pane,
                fresh,
                earlier,
                updates,
            } => publish.conversation(
                pane.0,
                &ToPhone::Transcript {
                    pane: pane.0,
                    fresh,
                    earlier,
                    updates: phone::updates(updates),
                },
            ),
            ServerMsg::EarlierTranscript {
                pane,
                before,
                earlier,
                updates,
            } => publish.conversation(
                pane.0,
                &ToPhone::Earlier {
                    pane: pane.0,
                    before,
                    earlier,
                    updates: phone::updates(updates),
                },
            ),
            ServerMsg::Stopping => return Some(Ended::Stopped),
            ServerMsg::Error { message } => tracing::warn!("argusd: {message}"),
            _ => {}
        }
        None
    }

    fn merge_telemetry(&mut self, pane: PaneId, telemetry: AgentTelemetry) -> bool {
        let found = self
            .tree
            .iter_mut()
            .flat_map(|w| w.projects.iter_mut())
            .flat_map(|p| p.repositories.iter_mut())
            .flat_map(|r| r.checkouts.iter_mut())
            .flat_map(|c| c.panes.iter_mut())
            .find(|p| p.id == pane);
        match found {
            Some(p) if p.telemetry != telemetry => {
                p.telemetry = telemetry;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_two_phones_watch_is_let_go_of_by_the_last() {
        let mut watching = HashMap::new();
        let watch = ClientMsg::WatchTranscript { pane: PaneId(3) };
        assert!(matches!(count(&mut watching, &Ask::Watch(3)), Some(ref m) if format!("{m:?}") == format!("{watch:?}")));
        // The second watcher still asks, for the fresh tail it starts from.
        assert!(count(&mut watching, &Ask::Watch(3)).is_some());
        assert!(count(&mut watching, &Ask::Unwatch(3)).is_none());
        assert!(matches!(
            count(&mut watching, &Ask::Unwatch(3)),
            Some(ClientMsg::UnwatchTranscript { pane: PaneId(3) })
        ));
        assert!(count(&mut watching, &Ask::Refresh(3)).is_none(), "nobody is watching");
    }
}
