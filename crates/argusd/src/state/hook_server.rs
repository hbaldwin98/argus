//! The loopback pane API: the HTTP receiver an agent's hooks POST to.
//!
//! Separate from the tree it reports into because it is a protocol surface,
//! not daemon state — the same reason `harness`, `watch` and `session` are
//! their own modules. Its grammar lives further out still, in
//! `argus_protocol::hook`, since `argus-hook` builds the paths this parses.
//!
//! The server binds loopback only and checks a per-boot bearer token, which
//! is all that stands between a pane's status and any other local process.

use std::path::PathBuf;
use std::sync::Arc;

use argus_protocol::{parse_request_target, Endpoint, PaneId, MAX_DIAGRAM_BODY_BYTES};

use super::Daemon;

impl Daemon {
    /// Binds the loopback HTTP status receiver hook commands POST to (see
    /// `hooks::install_claude_hooks`) and starts serving it in the
    /// background. The bind itself is synchronous so `hook_port` is set
    /// before the daemon's client socket starts accepting — no window where
    /// a client could spawn an agent whose hooks point nowhere.
    pub fn start_hook_server(self: &Arc<Self>) -> anyhow::Result<()> {
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        std_listener.set_nonblocking(true)?;
        let port = std_listener.local_addr()?.port();
        self.hook_port
            .store(port, std::sync::atomic::Ordering::Relaxed);
        let listener = tokio::net::TcpListener::from_std(std_listener)?;

        let daemon = self.clone();
        tokio::spawn(async move {
            // One failed accept used to end this loop for the life of the
            // daemon, so a moment of fd pressure left every hook silently
            // doing nothing until a restart. Back off and keep listening.
            let mut backoff = ACCEPT_BACKOFF_MIN;
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        backoff = ACCEPT_BACKOFF_MIN;
                        let daemon = daemon.clone();
                        tokio::spawn(async move {
                            let _ = handle_hook_request(stream, daemon).await;
                        });
                    }
                    Err(e) => {
                        tracing::warn!("could not accept a hook: {e}; retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(ACCEPT_BACKOFF_MAX);
                    }
                }
            }
        });
        Ok(())
    }
}

const MAX_BODY: usize = 4096;
const MAX_DRAFT_BODY: usize = 64 * 1024;
/// Diagram adds carry Mermaid source in JSON; allow room for encoding overhead.
const MAX_DIAGRAM_HOOK_BODY: usize = MAX_DIAGRAM_BODY_BYTES + 4096;

fn max_hook_body(endpoint: Option<(PaneId, Endpoint)>) -> usize {
    match endpoint {
        Some((_, Endpoint::Diagrams)) => MAX_DIAGRAM_HOOK_BODY,
        Some((_, Endpoint::Transcript)) => argus_protocol::MAX_PUSH_BYTES,
        // A tenth of a second of a reply, or more after a stall.
        Some((_, Endpoint::Draft)) => MAX_DRAFT_BODY,
        _ => MAX_BODY,
    }
}

/// The shortest and longest a failed accept waits before trying again.
const ACCEPT_BACKOFF_MIN: std::time::Duration = std::time::Duration::from_millis(50);
const ACCEPT_BACKOFF_MAX: std::time::Duration = std::time::Duration::from_secs(2);

struct HookResponse {
    code: u16,
    reason: &'static str,
    body: Vec<u8>,
}

impl HookResponse {
    fn empty(code: u16, reason: &'static str) -> Self {
        Self {
            code,
            reason,
            body: Vec::new(),
        }
    }

    fn text(code: u16, reason: &'static str, body: String) -> Self {
        Self {
            code,
            reason,
            body: body.into_bytes(),
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.code,
            self.reason,
            self.body.len()
        )
        .into_bytes();
        response.extend_from_slice(&self.body);
        response
    }
}

/// What the daemon now holds, as JSON, or its refusal as text the agent
/// can put in front of the user. Every read and write an agent makes here
/// is answered this way.
fn json_reply<T: serde::Serialize>(result: anyhow::Result<T>) -> HookResponse {
    let value = match result {
        Ok(value) => value,
        Err(error) => return HookResponse::text(409, "Conflict", error.to_string()),
    };
    match serde_json::to_vec(&value) {
        Ok(body) => HookResponse {
            code: 200,
            reason: "OK",
            body,
        },
        Err(e) => HookResponse::text(500, "Internal Server Error", e.to_string()),
    }
}

/// A write's body, or the refusal that says it was not one.
fn decode<T: serde::de::DeserializeOwned>(body: &[u8], what: &str) -> Result<T, HookResponse> {
    serde_json::from_slice(body)
        .map_err(|_| HookResponse::text(400, "Bad Request", format!("not a {what}")))
}

async fn handle_hook_request(
    stream: tokio::net::TcpStream,
    daemon: Arc<Daemon>,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    let (rd, mut wr) = tokio::io::split(stream);
    let mut reader = BufReader::new(rd);

    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;
    let (method, target) = request_line.split_once(' ').unwrap_or(("", ""));
    let target = target.split(' ').next().unwrap_or("").to_string();
    let path = if method == "POST" { target.clone() } else { String::new() };

    let headers = read_hook_headers(&mut reader, &daemon.hook_token).await?;
    // The one request that is read rather than posted, and held open.
    if method == "GET" {
        if let Some((pane, Endpoint::Inbox)) = parse_request_target(&target).0 {
            if !headers.authorized {
                wr.write_all(&HookResponse::empty(401, "Unauthorized").bytes()).await?;
                return Ok(());
            }
            return serve_inbox(reader, wr, daemon, pane, headers.reporter).await;
        }
    }
    let (authorized, content_length, reporter) =
        (headers.authorized, headers.content_length, headers.reporter);
    let (endpoint, artifact_scope) = parse_request_target(&path);
    let filing = super::features::Filing {
        scope: artifact_scope,
        feature: argus_protocol::requested_feature(&path),
    };
    // The server trusts nothing about a request beyond its bearer token.
    let max_body = max_hook_body(endpoint);
    let too_large = content_length > max_body;
    let mut body = vec![0u8; if too_large { 0 } else { content_length }];
    if !body.is_empty() {
        let _ = reader.read_exact(&mut body).await;
    }

    let response = if !authorized {
        HookResponse::empty(401, "Unauthorized")
    } else if too_large {
        HookResponse::text(413, "Content Too Large", "request body is too large".into())
    } else {
        match endpoint {
            Some((pane, Endpoint::Status(report))) => {
                let note = String::from_utf8_lossy(&body).to_string();
                daemon.report_pane_status(pane, reporter.as_deref(), report.status(), Some(note));
                HookResponse::empty(200, "OK")
            }
            Some((pane, Endpoint::Title)) => {
                daemon.report_pane_title(
                    pane,
                    reporter.as_deref(),
                    &String::from_utf8_lossy(&body),
                );
                HookResponse::empty(200, "OK")
            }
            Some((pane, Endpoint::Checkout))
                if daemon.child_of(pane, reporter.as_deref()).is_none() =>
            {
                let destination = PathBuf::from(String::from_utf8_lossy(&body).trim());
                if let Err(error) = daemon.move_agent_to_checkout(pane, &destination) {
                    tracing::warn!("pane {} could not move checkout: {error}", pane.0);
                }
                HookResponse::empty(200, "OK")
            }
            Some((pane, Endpoint::Session)) => {
                daemon.set_pane_session_id(pane, &String::from_utf8_lossy(&body));
                HookResponse::empty(200, "OK")
            }
            Some((pane, Endpoint::Comments)) => json_reply(daemon.review_comments_for_agent(pane)),
            Some((pane, Endpoint::Decisions)) => {
                json_reply(daemon.decisions_for_agent(pane, filing))
            }
            Some((pane, Endpoint::Decide)) => {
                decide_response(&daemon, pane, reporter.as_deref(), &body, filing)
            }
            Some((pane, Endpoint::DecisionChange)) => match decode(&body, "decision change") {
                Ok(change) => json_reply(daemon.change_decision_for_agent(pane, change, filing)),
                Err(refusal) => refusal,
            },
            Some((pane, Endpoint::Features)) => {
                json_reply(daemon.feature_board_for_agent(pane, filing))
            }
            Some((pane, Endpoint::Feature)) => {
                feature_response(&daemon, pane, reporter.as_deref(), &body, filing)
            }
            Some((pane, Endpoint::Telemetry)) => match decode(&body, "telemetry report") {
                Ok(report) => {
                    daemon.report_pane_telemetry(pane, reporter.as_deref(), report);
                    HookResponse::empty(200, "OK")
                }
                Err(refusal) => refusal,
            },
            Some((pane, Endpoint::Transcript)) => match decode(&body, "transcript push") {
                Ok(push) => {
                    daemon.report_pushed(pane, reporter.as_deref(), push);
                    HookResponse::empty(200, "OK")
                }
                Err(refusal) => refusal,
            },
            Some((pane, Endpoint::Draft)) => match decode(&body, "draft") {
                Ok(changes) => {
                    daemon.report_draft(pane, reporter.as_deref(), changes);
                    HookResponse::empty(200, "OK")
                }
                Err(refusal) => refusal,
            },
            Some((pane, Endpoint::Tasks)) => {
                tasks_response(&daemon, pane, reporter.as_deref(), &body, filing)
            }
            Some((pane, Endpoint::Diagrams)) => {
                diagrams_response(&daemon, pane, reporter.as_deref(), &body, filing)
            }
            // A checkout move from an agent that does not own the pane is
            // dropped: the row follows the agent Argus started in it.
            _ => HookResponse::empty(200, "OK"),
        }
    };
    // Any report may name the conversation's file. Taken after the report
    // itself, so a session claim that makes this conversation the pane's
    // own is in place before the file is judged by who sent it.
    if let (true, Some((pane, _)), Some(transcript)) = (authorized, endpoint, &headers.transcript) {
        daemon.report_transcript(pane, reporter.as_deref(), transcript);
    }
    wr.write_all(&response.bytes()).await?;
    Ok(())
}

/// How often a quiet inbox says it is still there, so a plugin notices a
/// daemon that went away and a daemon notices a plugin that did.
const INBOX_HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(15);

/// Holds a pane's inbox open, one server-sent event per item, until the
/// plugin hangs up or a newer connection takes the pane.
///
/// The plugin never writes after its request, so anything read — its end of
/// the socket closing — means it has gone, and the inbox closes at once
/// rather than at the next item, which would otherwise be sent into a
/// connection nobody holds and lost.
async fn serve_inbox<R, W>(
    mut rd: R,
    mut wr: W,
    daemon: Arc<Daemon>,
    pane: PaneId,
    reporter: Option<String>,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let Some((generation, mut items)) = daemon.open_inbox(pane, reporter.as_deref()) else {
        wr.write_all(&HookResponse::empty(409, "Conflict").bytes()).await?;
        return Ok(());
    };
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\r\n";
    let mut result = wr.write_all(head.as_bytes()).await;
    let mut scratch = [0u8; 64];
    while result.is_ok() {
        let event = tokio::select! {
            item = items.recv() => match item {
                Some(item) => format!("data: {}\n\n", serde_json::to_string(&item)?),
                None => break,
            },
            _ = rd.read(&mut scratch) => break,
            _ = tokio::time::sleep(INBOX_HEARTBEAT) => ": still here\n\n".to_string(),
        };
        result = async {
            wr.write_all(event.as_bytes()).await?;
            wr.flush().await
        }
        .await;
    }
    daemon.close_inbox(pane, generation);
    Ok(())
}

/// What a hook request's headers say about it.
struct HookHeaders {
    authorized: bool,
    content_length: usize,
    /// The conversation the report comes from.
    reporter: Option<String>,
    /// The file that conversation is written to, when the harness said.
    transcript: Option<String>,
}

async fn read_hook_headers<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    token: &str,
) -> anyhow::Result<HookHeaders> {
    use tokio::io::AsyncBufReadExt;

    let mut headers = HookHeaders {
        authorized: false,
        content_length: 0,
        reporter: None,
        transcript: None,
    };
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(v) = strip_header(&line, "Authorization") {
            headers.authorized = v.eq_ignore_ascii_case(&format!("Bearer {token}"));
        } else if let Some(v) = strip_header(&line, "Content-Length") {
            headers.content_length = v.parse().unwrap_or(0);
        } else if let Some(v) = strip_header(&line, argus_protocol::SESSION_HEADER) {
            headers.reporter = valid_session_id(v);
        } else if let Some(v) = strip_header(&line, argus_protocol::TRANSCRIPT_HEADER) {
            headers.transcript = Some(v.to_string()).filter(|v| !v.is_empty());
        }
    }
    Ok(headers)
}

/// Answers with the decision as recorded, because its id is what the next
/// decision hangs off — the one write here whose answer the agent has to
/// keep.
fn decide_response(
    daemon: &Arc<Daemon>,
    source: PaneId,
    session: Option<&str>,
    body: &[u8],
    filing: super::features::Filing,
) -> HookResponse {
    match decode(body, "decision") {
        Ok(write) => json_reply(daemon.record_agent_decision(source, session, write, filing)),
        Err(refusal) => refusal,
    }
}

/// Answers with the board as it stands afterwards, because every one of
/// these changes what the next `decide` from this checkout is filed under
/// — the agent has to be able to see where it now is.
fn feature_response(
    daemon: &Arc<Daemon>,
    source: PaneId,
    session: Option<&str>,
    body: &[u8],
    filing: super::features::Filing,
) -> HookResponse {
    use argus_protocol::FeatureAction;

    let action: FeatureAction = match decode(body, "feature change") {
        Ok(action) => action,
        Err(refusal) => return refusal,
    };
    json_reply(match action {
        FeatureAction::Open(write) => {
            daemon.open_feature_for_agent(source, session, write, filing.scope)
        }
        FeatureAction::Select { slug } => {
            daemon.select_feature_for_agent(source, &slug, filing.scope)
        }
        FeatureAction::Append { text } => daemon.append_to_feature_for_agent(source, &text, filing),
        FeatureAction::Done { slug } => daemon.move_feature_for_agent(
            source,
            session,
            &slug,
            argus_protocol::FeatureState::Done,
            filing.scope,
        ),
        FeatureAction::Reopen { slug } => daemon.move_feature_for_agent(
            source,
            session,
            &slug,
            argus_protocol::FeatureState::Open,
            filing.scope,
        ),
        FeatureAction::Retitle { slug, title } => {
            daemon.retitle_feature_for_agent(source, &slug, &title, filing.scope)
        }
        FeatureAction::Rewrite { slug, body } => {
            daemon.rewrite_feature_for_agent(source, &slug, &body, filing.scope)
        }
        FeatureAction::Drop { slug } => daemon.drop_feature_for_agent(source, &slug, filing.scope),
        FeatureAction::Hold { slug, reason } => {
            daemon.hold_feature_for_agent(source, &slug, Some(&reason), filing.scope)
        }
        FeatureAction::Unhold { slug } => {
            daemon.hold_feature_for_agent(source, &slug, None, filing.scope)
        }
        FeatureAction::Wait { slug, on } => {
            daemon.wait_feature_for_agent(source, &slug, &on, true, filing.scope)
        }
        FeatureAction::Unwait { slug, on } => {
            daemon.wait_feature_for_agent(source, &slug, &on, false, filing.scope)
        }
    })
}

/// Every change to the current feature's task list, and the read.
///
/// One endpoint rather than five: they all answer with the same list, and
/// an agent that has just added three tasks needs to see the ids they were
/// given before it can take one up.
fn tasks_response(
    daemon: &Arc<Daemon>,
    source: PaneId,
    session: Option<&str>,
    body: &[u8],
    filing: super::features::Filing,
) -> HookResponse {
    match decode::<argus_protocol::TaskAction>(body, "task change") {
        Ok(action) => json_reply(daemon.part_action_for_agent(source, session, action, filing)),
        Err(refusal) => refusal,
    }
}

fn diagrams_response(
    daemon: &Arc<Daemon>,
    source: PaneId,
    session: Option<&str>,
    body: &[u8],
    filing: super::features::Filing,
) -> HookResponse {
    match decode::<argus_protocol::DiagramAction>(body, "diagram change") {
        Ok(action) => json_reply(daemon.part_action_for_agent(source, session, action, filing)),
        Err(refusal) => refusal,
    }
}

/// A harness session id is opaque to Argus — it only has to be one
/// nonempty, bounded, control-free line, since it goes on to enter a
/// child's argv.
pub(super) fn valid_session_id(raw: &str) -> Option<String> {
    const MAX: usize = 512;
    let id = raw.trim();
    (!id.is_empty() && id.len() <= MAX && !id.chars().any(char::is_control)).then(|| id.to_string())
}

/// Not cryptographically strong — see `Daemon::hook_token`'s doc comment —
/// just enough entropy that it isn't a fixed, guessable string.
pub(super) fn gen_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TOKEN: AtomicU64 = AtomicU64::new(0);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let sequence = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    format!("{now:016x}{sequence:016x}")
}

/// One header's value, matched without regard to case — which HTTP allows
/// and the harnesses in the wild disagree about.
fn strip_header<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let (head, value) = line.split_once(':')?;
    head.eq_ignore_ascii_case(name).then(|| value.trim())
}
