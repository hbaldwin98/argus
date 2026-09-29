//! Running a pane's live channel: the harness's own server, started beside
//! the pane, and Argus's connection to it as a second client.
//!
//! Only Codex has one, and only for an agent template that sets
//! `live = true`: its app-server is experimental. The server is started
//! with the pane's environment, so hooks it runs still report to the pane,
//! and the TUI is pointed at it with `--remote`; the server lives exactly as
//! long as the pane. The connection takes the pane's inbox, so a reply, an
//! interrupt or an answer goes to Codex as a request rather than as typing,
//! and it reads the thread the pane's hooks named as it happens. Unix only:
//! the server listens on a Unix socket.

use std::path::{Path, PathBuf};

use super::*;
use crate::harness::live::LiveChannel;

/// How long a starting server has to open its socket.
#[cfg(unix)]
const SERVER_START: Duration = Duration::from_secs(5);

/// A live channel's server, stopped when the pane that owns it goes.
pub(super) struct LiveServer {
    child: std::process::Child,
    socket: PathBuf,
}

impl LiveServer {
    pub(super) fn socket(&self) -> &Path {
        &self.socket
    }
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

impl Daemon {
    /// Starts the live channel's server for a pane about to spawn, when its
    /// template asked and its harness has one: the server, and the
    /// arguments that point the TUI at it.
    pub(super) fn start_live_server(
        &self,
        pane: PaneId,
        channel: Option<LiveChannel>,
        cwd: &Path,
        env: &[(String, String)],
    ) -> Option<(LiveServer, Vec<String>)> {
        match channel? {
            LiveChannel::CodexAppServer => start_codex_server(pane, cwd, env),
        }
    }

    /// The conversation a pane's own agent claimed, as its hooks named it.
    pub(super) fn harness_session(&self, pane: PaneId) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        find_pane_ref(&inner.projects, pane)?.harness_session_id.clone()
    }

    /// Connects to a pane's live channel and keeps following it until the
    /// server goes.
    #[cfg(unix)]
    pub(super) fn follow_live(self: &Arc<Self>, pane: PaneId, socket: PathBuf) {
        let daemon = self.clone();
        tokio::spawn(async move {
            if let Err(error) = codex::follow(daemon, pane, &socket).await {
                tracing::warn!("pane {}'s live channel ended: {error:#}", pane.0);
            }
        });
    }

    #[cfg(not(unix))]
    pub(super) fn follow_live(self: &Arc<Self>, _pane: PaneId, _socket: PathBuf) {}
}

#[cfg(unix)]
fn start_codex_server(
    pane: PaneId,
    cwd: &Path,
    env: &[(String, String)],
) -> Option<(LiveServer, Vec<String>)> {
    let instance = argus_protocol::instance_name().unwrap_or_else(|| "argus".to_string());
    let socket = std::env::temp_dir().join(format!("{instance}-codex-{}.sock", pane.0));
    let _ = std::fs::remove_file(&socket);
    let url = format!("unix://{}", socket.display());
    let child = std::process::Command::new("codex")
        .args(["app-server", "--listen", &url])
        .current_dir(cwd)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(error) => {
            tracing::warn!("could not start codex app-server for pane {}: {error}", pane.0);
            return None;
        }
    };
    let server = LiveServer { child, socket };
    let deadline = std::time::Instant::now() + SERVER_START;
    while !server.socket.exists() {
        if std::time::Instant::now() > deadline {
            tracing::warn!("codex app-server for pane {} never opened its socket", pane.0);
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Some((server, vec!["--remote".to_string(), url]))
}

#[cfg(not(unix))]
fn start_codex_server(
    pane: PaneId,
    _cwd: &Path,
    _env: &[(String, String)],
) -> Option<(LiveServer, Vec<String>)> {
    tracing::warn!("pane {} asked for a live channel, which needs a Unix socket", pane.0);
    None
}

/// Argus as a second client of Codex's app-server.
#[cfg(unix)]
mod codex {
    use std::collections::HashMap;

    use argus_protocol::{InboxItem, Push, Update};
    use futures_util::{SinkExt, StreamExt};
    use serde_json::{json, Value};
    use tokio_tungstenite::tungstenite::Message;

    use super::*;
    use crate::harness::live::{answer, answered, hear, question_id, request, Heard};

    /// Where following a thread has got to.
    #[derive(Default)]
    struct Following {
        thread: Option<String>,
        turn: Option<String>,
        next_id: u64,
        /// Questions posed and not yet answered: the request each answers,
        /// and the question as posed.
        asked: HashMap<String, (Value, Update)>,
    }

    pub(super) async fn follow(daemon: Arc<Daemon>, pane: PaneId, socket: &Path) -> anyhow::Result<()> {
        let stream = tokio::net::UnixStream::connect(socket).await?;
        let (mut ws, _) = tokio_tungstenite::client_async("ws://localhost/", stream).await?;
        let greeting = json!({
            "id": 0,
            "method": "initialize",
            "params": { "clientInfo": { "name": "argus", "title": "Argus", "version": env!("CARGO_PKG_VERSION") } },
        });
        ws.send(Message::Text(greeting.to_string().into())).await?;
        ws.send(Message::Text(json!({ "method": "initialized" }).to_string().into()))
            .await?;

        let Some((generation, mut inbox)) = daemon.open_inbox(pane, None) else {
            return Ok(());
        };
        let mut state = Following {
            next_id: 1,
            ..Default::default()
        };
        let mut look = tokio::time::interval(Duration::from_millis(500));
        let result: anyhow::Result<()> = async {
            loop {
                tokio::select! {
                    message = ws.next() => {
                        let Some(message) = message else { return Ok(()) };
                        let Message::Text(text) = message? else { continue };
                        let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else { continue };
                        heard(&daemon, pane, &mut state, &value);
                    }
                    item = inbox.recv() => {
                        let Some(item) = item else { return Ok(()) };
                        if let Some(out) = said(&daemon, pane, &mut state, item) {
                            ws.send(Message::Text(out.to_string().into())).await?;
                        }
                    }
                    // The thread is the conversation Codex's hooks named;
                    // until they have, there is nothing to follow.
                    _ = look.tick(), if state.thread.is_none() => {
                        if let Some(thread) = daemon.harness_session(pane) {
                            let resume = json!({
                                "id": state.next_id,
                                "method": "thread/resume",
                                "params": { "threadId": thread },
                            });
                            state.next_id += 1;
                            state.thread = Some(thread);
                            ws.send(Message::Text(resume.to_string().into())).await?;
                        }
                    }
                }
            }
        }
        .await;
        daemon.close_inbox(pane, generation);
        result
    }

    fn push(daemon: &Daemon, pane: PaneId, updates: Vec<Update>) {
        if !updates.is_empty() {
            daemon.report_pushed(pane, None, Push { fresh: false, updates });
        }
    }

    /// What one message from the server does.
    fn heard(daemon: &Daemon, pane: PaneId, state: &mut Following, message: &Value) {
        match hear(message) {
            Heard::Said(updates) => push(daemon, pane, updates),
            Heard::TurnStarted(turn, updates) => {
                state.turn = Some(turn);
                push(daemon, pane, updates);
            }
            Heard::TurnEnded(updates) => {
                state.turn = None;
                push(daemon, pane, updates);
            }
            Heard::Asked { question, request, .. } => {
                state.asked.insert(question_id(&request), (request, question.clone()));
                push(daemon, pane, vec![question]);
            }
            // Answered on the desktop, most likely: the phone's buttons go.
            Heard::Resolved(request) => {
                if let Some((_, question)) = state.asked.remove(&question_id(&request)) {
                    push(daemon, pane, answered(&question, "answered").into_iter().collect());
                }
            }
            Heard::Nothing => {}
        }
    }

    /// What an item from the inbox sends the server, if anything.
    fn said(daemon: &Daemon, pane: PaneId, state: &mut Following, item: InboxItem) -> Option<Value> {
        if let InboxItem::Answer { question, choice } = &item {
            let (request, posed) = state.asked.remove(question)?;
            push(daemon, pane, answered(&posed, choice).into_iter().collect());
            return Some(answer(&request, choice));
        }
        let thread = state.thread.clone()?;
        let out = request(&item, &thread, state.turn.as_deref(), state.next_id)?;
        state.next_id += 1;
        Some(out)
    }
}
