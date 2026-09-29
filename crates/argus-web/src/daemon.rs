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
    read_known_msg, write_msg, AgentTelemetry, ClientMsg, Earlier, Hello, PaneId, Sent, ServerMsg,
    WorkspaceId, WorkspaceTree, WIDE_TREE,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc, watch};

use crate::phone::{self, ToPhone};

/// Where the answer to one phone's ask goes: that phone alone.
pub type Reply = mpsc::UnboundedSender<Arc<String>>;

/// What a phone connection asks of the daemon connection.
#[derive(Debug, Clone)]
pub enum Ask {
    /// Start following a conversation, and have its tail sent fresh.
    Watch(u64),
    /// Stop following it, once nobody else is.
    Unwatch(u64),
    /// Have its tail sent fresh again, for a phone that fell behind.
    Refresh(u64),
    Earlier(u64, Earlier),
    /// Say something to an agent, answered to `reply`.
    Send {
        pane: u64,
        text: String,
        now: bool,
        reply: Reply,
    },
    /// Take back a queued message.
    Cancel(u64, u64),
    /// Interrupt an agent.
    Stop(u64),
    /// Start watching a pane's screen, which is sent whole.
    Screen(u64),
    /// Stop watching it, once nobody else is.
    Unscreen(u64),
    /// Send the whole screen again, for a phone that fell behind.
    RefreshScreen(u64),
    /// Keys straight to a pane.
    Key(u64, &'static [u8]),
}

/// What the daemon connection keeps across reconnecting.
#[derive(Default)]
struct Link {
    /// How many phones watch each conversation.
    watching: HashMap<u64, usize>,
    /// How many phones watch each screen.
    screens: HashMap<u64, usize>,
    /// Messages sent and not yet answered, by request id: the pane, the
    /// text, and the phone to answer.
    requests: HashMap<u64, (u64, String, Reply)>,
    /// Messages the daemon queued for a phone, by pane and queue id, until
    /// a tree no longer lists them.
    placed: HashMap<(u64, u64), (String, Reply)>,
    next_request: u64,
    /// Whether the next tree is this connection's first, against which a
    /// message placed with a daemon before it was not typed but lost.
    first_tree: bool,
}

fn json(msg: &ToPhone) -> Arc<String> {
    Arc::new(serde_json::to_string(msg).unwrap_or_default())
}

/// What every phone connection reads from: the latest of each whole-state
/// message, and a stream of conversation updates tagged by pane.
#[derive(Clone)]
pub struct Feeds {
    pub server: watch::Receiver<Arc<String>>,
    pub agents: watch::Receiver<Arc<String>>,
    pub conversations: broadcast::Sender<(u64, Arc<String>)>,
    pub screens: broadcast::Sender<(u64, Arc<String>)>,
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
    screens: broadcast::Sender<(u64, Arc<String>)>,
    /// Status changes worth waking a phone for.
    notices: mpsc::UnboundedSender<crate::push::Notice>,
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

    fn screen(&self, pane: u64, update: crate::screen::ScreenUpdate) {
        if let Ok(json) = serde_json::to_string(&ToPhone::Screen { pane, update }) {
            let _ = self.screens.send((pane, Arc::new(json)));
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
    mpsc::UnboundedReceiver<crate::push::Notice>,
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
    let (screens, _) = broadcast::channel(64);
    let (notices, notices_rx) = mpsc::unbounded_channel();
    let publish = Publish {
        server,
        agents,
        conversations: conversations.clone(),
        screens: screens.clone(),
        notices,
    };
    let (asks, asks_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(keep_connected(connect, publish, asks_rx));
    let feeds = Feeds {
        server: server_rx,
        agents: agents_rx,
        conversations,
        screens,
    };
    (feeds, asks, notices_rx, task)
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
    let mut link = Link::default();
    let mut backoff = Duration::from_millis(250);
    loop {
        match connect().await {
            Ok(stream) => {
                backoff = Duration::from_millis(250);
                if session(stream, &publish, &mut asks, &mut link).await == Some(Ended::Stopped) {
                    publish.server(false, None);
                    return Ended::Stopped;
                }
            }
            Err(error) => tracing::warn!("argus web could not reach argusd: {error:#}"),
        }
        publish.server(false, None);
        // Nothing sent and unanswered will be answered now.
        for (_, (pane, _, reply)) in link.requests.drain() {
            let _ = reply.send(json(&lost(pane)));
        }
        // Asks keep arriving while there is no daemon; the counts they
        // change are what the next connection watches again.
        let wait = tokio::time::sleep(backoff);
        tokio::pin!(wait);
        loop {
            tokio::select! {
                _ = &mut wait => break,
                ask = asks.recv() => match ask {
                    Some(Ask::Send { pane, reply, .. }) => { let _ = reply.send(json(&lost(pane))); }
                    Some(ask) => { link.ask(ask); }
                    None => return Ended::Stopped,
                },
            }
        }
        backoff = (backoff * 2).min(Duration::from_secs(5));
    }
}

fn lost(pane: u64) -> ToPhone {
    ToPhone::Sent {
        pane,
        outcome: "refused",
        reason: Some("argus web has lost argusd; try again once it is back".to_string()),
    }
}

impl Link {
    /// Applies an ask to what the link keeps, saying what the daemon should
    /// be sent for it, if anything.
    fn ask(&mut self, ask: Ask) -> Option<ClientMsg> {
        match ask {
            Ask::Send {
                pane,
                text,
                now,
                reply,
            } => {
                self.next_request += 1;
                let request_id = self.next_request;
                self.requests.insert(request_id, (pane, text.clone(), reply));
                Some(ClientMsg::SendToAgent {
                    pane: PaneId(pane),
                    text,
                    now,
                    request_id,
                })
            }
            Ask::Cancel(pane, id) => Some(ClientMsg::CancelQueued {
                pane: PaneId(pane),
                id,
            }),
            Ask::Stop(pane) => Some(ClientMsg::Interrupt { pane: PaneId(pane) }),
            Ask::Key(pane, bytes) => Some(ClientMsg::Input {
                pane: PaneId(pane),
                bytes: bytes.to_vec(),
            }),
            // Subscribing gets a whole grid; never a Resize, so the phone
            // never changes the size the desktop gave the pane.
            Ask::Screen(pane) => {
                let watchers = self.screens.entry(pane).or_insert(0);
                *watchers += 1;
                (*watchers == 1).then_some(ClientMsg::Subscribe { pane: PaneId(pane) })
            }
            Ask::Unscreen(pane) => match self.screens.get_mut(&pane) {
                Some(watchers) if *watchers <= 1 => {
                    self.screens.remove(&pane);
                    Some(ClientMsg::Unsubscribe { pane: PaneId(pane) })
                }
                Some(watchers) => {
                    *watchers -= 1;
                    None
                }
                None => None,
            },
            Ask::RefreshScreen(_) => None,
            other => count(&mut self.watching, &other),
        }
    }

    /// The daemon's answer to a message, to the phone that sent it.
    fn answered(&mut self, request_id: u64, sent: Sent) {
        let Some((pane, text, reply)) = self.requests.remove(&request_id) else {
            return;
        };
        if let Sent::Queued { id } = sent {
            self.placed.insert((pane, id), (text, reply.clone()));
        }
        let _ = reply.send(json(&phone::sent(pane, sent)));
    }

    /// Forgets placed messages a tree no longer lists: typed or taken back,
    /// by a live daemon — or, on a connection's first tree, lost with the
    /// daemon before it, which their phones are told.
    fn reconcile(&mut self, tree: &[WorkspaceTree]) {
        let listed: std::collections::HashSet<(u64, u64)> = tree
            .iter()
            .flat_map(|w| w.projects.iter())
            .flat_map(|p| p.repositories.iter())
            .flat_map(|r| r.checkouts.iter())
            .flat_map(|c| c.panes.iter())
            .flat_map(|p| p.queued.iter().map(move |m| (p.id.0, m.id)))
            .collect();
        let first = std::mem::take(&mut self.first_tree);
        self.placed.retain(|&(pane, id), (text, reply)| {
            if listed.contains(&(pane, id)) {
                return true;
            }
            if first {
                let _ = reply.send(json(&ToPhone::NotSent {
                    pane,
                    text: text.clone(),
                }));
            }
            false
        });
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
        Ask::Send { .. }
        | Ask::Cancel(..)
        | Ask::Stop(_)
        | Ask::Screen(_)
        | Ask::Unscreen(_)
        | Ask::RefreshScreen(_)
        | Ask::Key(..) => None,
    }
}

/// One connection's life: greet, watch again what phones are watching,
/// then carry messages until it drops. `Some(Stopped)` when the daemon
/// said it was stopping.
async fn session<S>(
    stream: S,
    publish: &Publish,
    asks: &mut mpsc::UnboundedReceiver<Ask>,
    link: &mut Link,
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
    for pane in link.watching.keys() {
        let _ = write_msg(&mut wr, &ClientMsg::WatchTranscript { pane: PaneId(*pane) }).await;
    }
    for pane in link.screens.keys() {
        let _ = write_msg(&mut wr, &ClientMsg::Subscribe { pane: PaneId(*pane) }).await;
    }
    link.first_tree = true;

    let mut state = State::default();
    let mut flush = tokio::time::interval(crate::screen::FLUSH);
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let ended = loop {
        tokio::select! {
            msg = incoming.recv() => {
                let Some(msg) = msg else { break None };
                if let Some(ended) = state.take(msg, publish, link) {
                    break Some(ended);
                }
            }
            ask = asks.recv() => {
                let Some(ask) = ask else { break Some(Ended::Stopped) };
                // A phone joining a screen others already watch starts from
                // the whole of it, as does one that fell behind.
                if let Ask::Screen(pane) | Ask::RefreshScreen(pane) = ask {
                    if let Some(screen) = state.screens.get_mut(&pane) {
                        screen.refresh();
                    }
                }
                if let Some(msg) = link.ask(ask) {
                    if write_msg(&mut wr, &msg).await.is_err() {
                        break None;
                    }
                }
            }
            _ = flush.tick() => {
                state.screens.retain(|pane, _| link.screens.contains_key(pane));
                for (pane, screen) in &mut state.screens {
                    if let Some(update) = screen.flush() {
                        publish.screen(*pane, update);
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
    /// The screens phones are watching, as the daemon streams them.
    screens: HashMap<u64, crate::screen::Screen>,
    /// Each agent's loudest status in the last tree, to tell what changed.
    /// Empty until this connection's first tree, which is taken as it is
    /// rather than announced.
    statuses: HashMap<u64, argus_protocol::PaneStatus>,
    /// Whether the daemon sends every workspace. One from before sends
    /// only the open workspace's tree, which stands in for all of them.
    wide: bool,
}

impl State {
    fn take(&mut self, msg: ServerMsg, publish: &Publish, link: &mut Link) -> Option<Ended> {
        match msg {
            ServerMsg::Hello(hello) => {
                publish.server(true, Some(&hello));
                self.daemon = Some(hello);
            }
            ServerMsg::WideTree(tree) => {
                self.wide = true;
                self.tree = tree;
                link.reconcile(&self.tree);
                self.announce(publish);
                publish.agents(&self.tree);
            }
            ServerMsg::Sent {
                request_id, sent, ..
            } => link.answered(request_id, sent),
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
            ServerMsg::PaneRows {
                pane,
                rows,
                cols,
                runs,
                cursor,
                ..
            } => match self.screens.get_mut(&pane.0) {
                Some(screen) => screen.replace(rows, cols, &runs, cursor),
                None => {
                    let screen = crate::screen::Screen::new(rows, cols, &runs, cursor);
                    self.screens.insert(pane.0, screen);
                }
            },
            ServerMsg::RowDamage {
                pane,
                scroll,
                runs,
                cursor,
                ..
            } => {
                if let Some(screen) = self.screens.get_mut(&pane.0) {
                    screen.damage(scroll, &runs, cursor);
                }
            }
            ServerMsg::PaneClosed { pane, .. } => {
                self.screens.remove(&pane.0);
            }
            ServerMsg::Stopping => return Some(Ended::Stopped),
            ServerMsg::Error { message } => tracing::warn!("argusd: {message}"),
            _ => {}
        }
        None
    }

    /// Tells phones of each agent whose state changed in a way worth
    /// waking them for.
    fn announce(&mut self, publish: &Publish) {
        let agents = self
            .tree
            .iter()
            .flat_map(|w| w.projects.iter())
            .flat_map(|p| p.repositories.iter())
            .flat_map(|r| r.checkouts.iter())
            .flat_map(|c| c.panes.iter())
            .filter(|p| p.kind == argus_protocol::PaneKind::Agent);
        let mut seen = HashMap::new();
        for pane in agents {
            let loudest = pane.loudest_state();
            seen.insert(pane.id.0, loudest.status);
            let before = self.statuses.get(&pane.id.0).copied();
            let Some(verb) = before.and_then(|before| crate::push::worth_telling(before, loudest.status)) else {
                continue;
            };
            let who = loudest
                .child
                .map(str::to_string)
                .or_else(|| pane.template.clone())
                .unwrap_or_else(|| "The agent".to_string());
            let body = match loudest.note {
                Some(note) if !note.is_empty() => format!("{who} {verb}: {note}"),
                _ => format!("{who} {verb}"),
            };
            let _ = publish.notices.send(crate::push::Notice {
                title: pane.title.clone(),
                body,
                pane: pane.id.0,
                url: format!("/#/pane/{}", pane.id.0),
            });
        }
        self.statuses = seen;
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

    fn tree_queuing(pane: u64, ids: &[u64]) -> Vec<WorkspaceTree> {
        let mut tree: WorkspaceTree = serde_json::from_value(serde_json::json!({
            "id": 0, "name": "default", "open": true, "projects": [{
                "id": 0, "name": "p", "root": null, "repositories": [{
                    "id": 0, "name": "r", "branches": [], "default_branch": null,
                    "remote_branches": [], "checkouts": [{
                        "id": 0, "name": "main", "path": "/r", "git": null, "primary": true,
                        "panes": [{
                            "id": pane, "kind": "Agent", "title": "a", "status": "Idle",
                            "note": null, "template": null, "children": [], "telemetry": {}
                        }]
                    }]
                }]
            }]
        }))
        .unwrap();
        let panes = &mut tree.projects[0].repositories[0].checkouts[0].panes;
        panes[0].queued = ids
            .iter()
            .map(|&id| argus_protocol::QueuedMessage { id, text: "t".into() })
            .collect();
        vec![tree]
    }

    #[test]
    fn a_message_a_restarted_daemon_forgot_is_reported_not_sent() {
        let mut link = Link::default();
        let (reply, mut phone) = mpsc::unbounded_channel();
        let Some(ClientMsg::SendToAgent { request_id, .. }) = link.ask(Ask::Send {
            pane: 7,
            text: "later please".into(),
            now: false,
            reply,
        }) else {
            panic!("a send goes to the daemon");
        };
        link.answered(request_id, Sent::Queued { id: 4 });
        assert!(phone.try_recv().unwrap().contains("\"outcome\":\"queued\""));

        // The same daemon lists it: still waiting.
        link.reconcile(&tree_queuing(7, &[4]));
        assert!(phone.try_recv().is_err());

        // A new connection whose first tree lacks it: that daemon never had it.
        link.first_tree = true;
        link.reconcile(&tree_queuing(7, &[]));
        let notice = phone.try_recv().unwrap();
        assert!(notice.contains("\"type\":\"not_sent\""), "{notice}");
        assert!(notice.contains("later please"), "{notice}");
        assert!(link.placed.is_empty());
    }

    #[test]
    fn a_message_typed_by_a_live_daemon_is_simply_forgotten() {
        let mut link = Link::default();
        let (reply, mut phone) = mpsc::unbounded_channel();
        link.ask(Ask::Send { pane: 7, text: "x".into(), now: false, reply });
        link.answered(1, Sent::Queued { id: 4 });
        let _ = phone.try_recv();
        link.reconcile(&tree_queuing(7, &[]));
        assert!(phone.try_recv().is_err(), "typed, not lost");
        assert!(link.placed.is_empty());
    }
}
