//! The one connection to the daemon, and what the server keeps of it for
//! the phones: the agent list, whether the daemon is there, and each
//! watched conversation's updates.
//!
//! Every phone shares this connection. A conversation two phones watch is
//! watched once, and a phone that starts watching makes the daemon send a
//! fresh tail, which the others take as a replacement for what they hold.
//! The connection is made again whenever it drops, the way the terminal
//! client makes it, except after the daemon says it is stopping: coming
//! back would start another, which is not what stopping asked for. A
//! daemon that cannot take the greeting is not fallen back to, as the
//! terminal falls back: the server is nothing without the transcripts and
//! the outbox such a daemon lacks, so phones are told to restart it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use argus_protocol::{
    read_known_msg, write_msg, ClientMsg, Earlier, Greeting, HeldTree, Hello, PaneId, Refusal,
    Sent, ServerMsg, WorkspaceId, WorkspaceTree, GREETING_WAIT, LIVE_CHANNELS, WIDE_TREE,
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
    /// Answer a question: the pane, the question's id, the choice's.
    Answer(u64, String, String),
    /// Start an agent from a template in a checkout, answered to `reply`.
    Start {
        checkout: u64,
        template: String,
        reply: Reply,
    },
    /// Close a pane.
    Close(u64),
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
    /// Agents asked for and not yet made, by request id: the phone to tell.
    starting: HashMap<u64, Reply>,
    /// Agents made and not yet in a tree, and the phone to tell once one
    /// lists them: a phone told first would open a pane its list lacks.
    made: Vec<(PaneId, Reply)>,
    next_request: u64,
    /// Whether the next tree is this connection's first, against which a
    /// message placed with a daemon before it was not typed but lost.
    first_tree: bool,
    /// The last tree notices were decided against. Kept across
    /// reconnecting, as the terminal keeps its tree, so an agent that
    /// started waiting while the daemon was away is still announced; empty
    /// until the process's first tree, which is taken as it is.
    announced: Vec<WorkspaceTree>,
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

/// How one connection ended.
#[derive(Debug, PartialEq, Eq)]
enum Over {
    /// The daemon said it was stopping, or nobody is left to ask anything.
    Stopped,
    /// It was up, and dropped.
    Dropped,
    /// Closed before the daemon said anything, or never made.
    Closed,
    /// The daemon could not take the greeting.
    Refused,
}

/// What phones are told of the daemon.
enum Reach<'a> {
    Lost,
    /// Connected, and the daemon's greeting if it gave one.
    Up(Option<&'a Hello>),
    /// It could not take the greeting: its own, if it gave one on another
    /// protocol, or none from a daemon before the handshake.
    Refused(Option<&'a Hello>),
}

const FIRST_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

struct Publish {
    server: watch::Sender<Arc<String>>,
    agents: watch::Sender<Arc<String>>,
    conversations: broadcast::Sender<(u64, Arc<String>)>,
    screens: broadcast::Sender<(u64, Arc<String>)>,
    /// Status changes worth waking a phone for.
    notices: mpsc::UnboundedSender<crate::push::Notice>,
}

impl Publish {
    fn server(&self, reach: Reach) {
        let ours = env!("CARGO_PKG_VERSION");
        let (connected, refused, daemon) = match reach {
            Reach::Lost => (false, false, None),
            Reach::Up(daemon) => (true, false, daemon),
            Reach::Refused(daemon) => (false, true, daemon),
        };
        // A refusal names the daemon's build even when the number matches:
        // the protocol differing is the news, and the version says which.
        let daemon_version = daemon
            .map(|hello| hello.version.clone())
            .filter(|version| refused || version != ours);
        send(
            &self.server,
            &ToPhone::Server {
                version: ours.to_string(),
                connected,
                refused,
                daemon_version,
            },
        );
    }

    fn agents(&self, tree: &[WorkspaceTree], templates: &[String]) {
        send(
            &self.agents,
            &ToPhone::Agents {
                workspaces: phone::agents(tree),
                templates: templates.to_vec(),
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
        refused: false,
        daemon_version: None,
    }));
    let (agents, agents_rx) = watch::channel(initial(&ToPhone::Agents {
        workspaces: Vec::new(),
        templates: Vec::new(),
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
    let mut backoff = FIRST_BACKOFF;
    loop {
        let over = match connect().await {
            Ok(stream) => session(stream, &publish, &mut asks, &mut link).await,
            Err(error) => {
                tracing::warn!("argus web could not reach argusd: {error:#}");
                Over::Closed
            }
        };
        match over {
            Over::Stopped => {
                publish.server(Reach::Lost);
                return Ended::Stopped;
            }
            Over::Dropped => {
                backoff = FIRST_BACKOFF;
                publish.server(Reach::Lost);
            }
            Over::Closed => publish.server(Reach::Lost),
            // Already told; asking again soon would only be refused again
            // until somebody restarts the daemon.
            Over::Refused => backoff = MAX_BACKOFF,
        }
        // Nothing sent and unanswered will be answered now.
        for (_, (pane, _, reply)) in link.requests.drain() {
            let _ = reply.send(json(&lost(pane)));
        }
        for (_, reply) in link.starting.drain() {
            let _ = reply.send(json(&ToPhone::Started { pane: None }));
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
                    Some(Ask::Start { reply, .. }) => {
                        let _ = reply.send(json(&ToPhone::Started { pane: None }));
                    }
                    Some(ask) => { link.ask(ask); }
                    None => return Ended::Stopped,
                },
            }
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
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
            Ask::Answer(pane, question, choice) => Some(ClientMsg::Answer {
                pane: PaneId(pane),
                question,
                choice,
            }),
            Ask::Start {
                checkout,
                template,
                reply,
            } => {
                self.next_request += 1;
                self.starting.insert(self.next_request, reply);
                Some(ClientMsg::SpawnAgent {
                    checkout: argus_protocol::CheckoutId(checkout),
                    template,
                    request_id: self.next_request,
                })
            }
            Ask::Close(pane) => Some(ClientMsg::Kill { pane: PaneId(pane) }),
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

    /// The daemon's answer to a start: the phone is told once a tree lists
    /// the pane, which may already be the one in hand.
    fn created(&mut self, request_id: u64, created: Option<argus_protocol::Created>, tree: &[WorkspaceTree]) {
        let Some(reply) = self.starting.remove(&request_id) else {
            return;
        };
        let Some(argus_protocol::Created::Pane(pane)) = created else {
            let _ = reply.send(json(&ToPhone::Started { pane: None }));
            return;
        };
        self.made.push((pane, reply));
        self.settle(tree, false);
    }

    /// Tells each phone whose agent this tree lists which pane it is. On a
    /// connection's first tree, one it does not list went with the daemon
    /// before it.
    fn settle(&mut self, tree: &[WorkspaceTree], first: bool) {
        self.made.retain(|(pane, reply)| {
            let listed = tree.panes().any(|p| p.id == *pane);
            if listed {
                let _ = reply.send(json(&ToPhone::Started { pane: Some(pane.0) }));
            } else if first {
                let _ = reply.send(json(&ToPhone::Started { pane: None }));
            }
            !listed && !first
        });
    }

    /// Forgets placed messages a tree no longer lists: typed or taken back,
    /// by a live daemon — or, on a connection's first tree, lost with the
    /// daemon before it, which their phones are told.
    fn reconcile(&mut self, tree: &[WorkspaceTree], first: bool) {
        let listed: std::collections::HashSet<(u64, u64)> = tree
            .panes()
            .flat_map(|p| p.queued.iter().map(move |m| (p.id.0, m.id)))
            .collect();
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
        | Ask::Key(..)
        | Ask::Answer(..)
        | Ask::Start { .. }
        | Ask::Close(_) => None,
    }
}

/// One connection's life: greet, watch again what phones are watching,
/// then carry messages until it drops.
async fn session<S>(
    stream: S,
    publish: &Publish,
    asks: &mut mpsc::UnboundedReceiver<Ask>,
    link: &mut Link,
) -> Over
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

    let over = async {
        let hello = Hello::this_build().and(WIDE_TREE).and(LIVE_CHANNELS);
        if write_msg(&mut wr, &ClientMsg::Hello(hello)).await.is_err() {
            return Over::Closed;
        }
        let (daemon, opening) = match Greeting::read(&mut incoming, GREETING_WAIT).await {
            Greeting::Up { daemon, opening } => (daemon, opening),
            Greeting::Refused(refusal) => {
                let daemon = match &refusal {
                    Refusal::Predates => None,
                    Refusal::Protocol { daemon, .. } => Some(daemon),
                };
                tracing::warn!("argusd cannot take argus web's greeting: {refusal:?}");
                publish.server(Reach::Refused(daemon));
                return Over::Refused;
            }
            Greeting::Closed => return Over::Closed,
        };
        publish.server(Reach::Up(daemon.as_ref()));

        for pane in link.watching.keys() {
            let _ = write_msg(&mut wr, &ClientMsg::WatchTranscript { pane: PaneId(*pane) }).await;
        }
        for pane in link.screens.keys() {
            let _ = write_msg(&mut wr, &ClientMsg::Subscribe { pane: PaneId(*pane) }).await;
        }
        link.first_tree = true;

        let mut state = State::default();
        for msg in opening {
            if let Some(over) = state.take(msg, publish, link) {
                return over;
            }
        }
        let mut flush = tokio::time::interval(crate::screen::FLUSH);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                msg = incoming.recv() => {
                    let Some(msg) = msg else { return Over::Dropped };
                    if let Some(over) = state.take(msg, publish, link) {
                        return over;
                    }
                }
                ask = asks.recv() => {
                    let Some(ask) = ask else { return Over::Stopped };
                    // A phone joining a screen others already watch starts from
                    // the whole of it, as does one that fell behind.
                    if let Ask::Screen(pane) | Ask::RefreshScreen(pane) = ask {
                        if let Some(screen) = state.screens.get_mut(&pane) {
                            screen.refresh();
                        }
                    }
                    if let Some(msg) = link.ask(ask) {
                        if write_msg(&mut wr, &msg).await.is_err() {
                            return Over::Dropped;
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
        }
    }
    .await;
    reader.abort();
    over
}

/// What this connection has been told.
#[derive(Default)]
struct State {
    tree: Vec<WorkspaceTree>,
    /// The screens phones are watching, as the daemon streams them.
    screens: HashMap<u64, crate::screen::Screen>,
    /// The templates an agent can be started from.
    templates: Vec<String>,
    /// Whether the daemon sends every workspace. One from before sends
    /// only the open workspace's tree, which stands in for all of them.
    wide: bool,
}

impl State {
    fn take(&mut self, msg: ServerMsg, publish: &Publish, link: &mut Link) -> Option<Over> {
        match msg {
            // A greeting answered after the wait for it ran out.
            ServerMsg::Hello(hello) if !hello.speaks_ours() => {
                publish.server(Reach::Refused(Some(&hello)));
                return Some(Over::Refused);
            }
            ServerMsg::Hello(hello) => publish.server(Reach::Up(Some(&hello))),
            ServerMsg::WideTree(tree) => {
                self.wide = true;
                self.tree = tree;
                let first = std::mem::take(&mut link.first_tree);
                link.reconcile(&self.tree, first);
                announce(&link.announced, &self.tree, publish);
                link.announced = self.tree.clone();
                publish.agents(&self.tree, &self.templates);
                // After the list, so a phone told which agent it started
                // already has it to open.
                link.settle(&self.tree, first);
            }
            ServerMsg::Sent {
                request_id, sent, ..
            } => link.answered(request_id, sent),
            ServerMsg::Templates(names) => {
                self.templates = names;
                publish.agents(&self.tree, &self.templates);
            }
            ServerMsg::Created {
                request_id,
                created,
            } => link.created(request_id, created, &self.tree),
            ServerMsg::Tree(projects) if !self.wide => {
                self.tree = vec![WorkspaceTree {
                    id: WorkspaceId(0),
                    name: "open workspace".to_string(),
                    open: true,
                    projects,
                }];
                publish.agents(&self.tree, &self.templates);
                link.settle(&self.tree, false);
            }
            ServerMsg::PaneTelemetry { pane, telemetry } => {
                if self.tree.apply_telemetry(pane, telemetry) {
                    publish.agents(&self.tree, &self.templates);
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
            ServerMsg::Stopping => return Some(Over::Stopped),
            ServerMsg::Error { message } => tracing::warn!("argusd: {message}"),
            _ => {}
        }
        None
    }
}

/// Tells phones of each agent whose state changed from `previous` in a way
/// worth waking them for.
fn announce(previous: &[WorkspaceTree], tree: &[WorkspaceTree], publish: &Publish) {
    let changed = argus_protocol::transitions(previous, tree)
        .into_iter()
        .filter(|t| t.pane.kind == argus_protocol::PaneKind::Agent);
    for argus_protocol::Transition { pane, before, after } in changed {
        let Some(verb) = crate::push::worth_telling(before, after.status) else {
            continue;
        };
        let who = after
            .child
            .map(str::to_string)
            .or_else(|| pane.template.clone())
            .unwrap_or_else(|| "The agent".to_string());
        let body = match after.note {
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
        link.reconcile(&tree_queuing(7, &[4]), false);
        assert!(phone.try_recv().is_err());

        // A new connection whose first tree lacks it: that daemon never had it.
        link.reconcile(&tree_queuing(7, &[]), true);
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
        link.reconcile(&tree_queuing(7, &[]), false);
        assert!(phone.try_recv().is_err(), "typed, not lost");
        assert!(link.placed.is_empty());
    }
}
