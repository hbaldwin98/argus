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

use argus_protocol::{parse_pane_path, Endpoint, PaneId};

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

async fn handle_hook_request(
    stream: tokio::net::TcpStream,
    daemon: Arc<Daemon>,
) -> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    let (rd, mut wr) = tokio::io::split(stream);
    let mut reader = BufReader::new(rd);

    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;
    let path = request_line
        .strip_prefix("POST ")
        .and_then(|rest| rest.split(' ').next())
        .unwrap_or("")
        .to_string();

    let (authorized, content_length, reporter) =
        read_hook_headers(&mut reader, &daemon.hook_token).await?;
    let endpoint = parse_pane_path(&path);
    // The server trusts nothing about a request beyond its bearer token.
    let too_large = content_length > MAX_BODY;
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
            Some((pane, Endpoint::Comments)) => comments_response(&daemon, pane)?,
            Some((pane, Endpoint::Context)) => context_response(&daemon, pane)?,
            Some((pane, Endpoint::Todo)) => {
                todo_response(&daemon, pane, reporter.as_deref(), &body)
            }
            Some((pane, Endpoint::Decisions)) => decisions_response(&daemon, pane)?,
            Some((pane, Endpoint::Decide)) => {
                decide_response(&daemon, pane, reporter.as_deref(), &body)
            }
            Some((pane, Endpoint::Features)) => features_response(&daemon, pane)?,
            Some((pane, Endpoint::Feature)) => {
                feature_response(&daemon, pane, reporter.as_deref(), &body)
            }
            // A checkout move from an agent that does not own the pane is
            // dropped: the row follows the agent Argus started in it.
            _ => HookResponse::empty(200, "OK"),
        }
    };
    wr.write_all(&response.bytes()).await?;
    Ok(())
}

async fn read_hook_headers<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    token: &str,
) -> anyhow::Result<(bool, usize, Option<String>)> {
    use tokio::io::AsyncBufReadExt;

    let mut authorized = false;
    let mut content_length = 0;
    let mut reporter = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(v) = strip_header(&line, "Authorization") {
            authorized = v.eq_ignore_ascii_case(&format!("Bearer {token}"));
        } else if let Some(v) = strip_header(&line, "Content-Length") {
            content_length = v.parse().unwrap_or(0);
        } else if let Some(v) = strip_header(&line, argus_protocol::SESSION_HEADER) {
            reporter = valid_session_id(v);
        }
    }
    Ok((authorized, content_length, reporter))
}

fn context_response(daemon: &Arc<Daemon>, source: PaneId) -> anyhow::Result<HookResponse> {
    Ok(match daemon.context_for_agent(source) {
        Ok(context) => HookResponse {
            code: 200,
            reason: "OK",
            body: serde_json::to_vec(&context)?,
        },
        Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
    })
}

/// The one endpoint that changes something a human owns, so it is the one
/// endpoint that answers in prose.
///
/// Every other write here is fire-and-forget: a status report nobody reads
/// the reply to. A refused note write has to come back as something the
/// agent can put in front of the user — "this project does not allow it" is
/// a different situation from "that line is not a checkbox", and both are
/// different from having worked.
fn todo_response(
    daemon: &Arc<Daemon>,
    source: PaneId,
    session: Option<&str>,
    body: &[u8],
) -> HookResponse {
    let write: argus_protocol::TodoWrite = match serde_json::from_slice(body) {
        Ok(write) => write,
        Err(_) => {
            return HookResponse::text(400, "Bad Request", "not a note change".into());
        }
    };
    match daemon.write_agent_todo(source, session, &write) {
        Ok(counts) => HookResponse::text(
            200,
            "OK",
            format!("{} open, {} done", counts.open, counts.done),
        ),
        Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
    }
}

fn decisions_response(daemon: &Arc<Daemon>, source: PaneId) -> anyhow::Result<HookResponse> {
    Ok(match daemon.decisions_for_agent(source) {
        Ok(board) => HookResponse {
            code: 200,
            reason: "OK",
            body: serde_json::to_vec(&board)?,
        },
        Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
    })
}

/// Answers with the decision as recorded, because its id is what the next
/// decision hangs off — the one write here whose answer the agent has to
/// keep.
fn decide_response(
    daemon: &Arc<Daemon>,
    source: PaneId,
    session: Option<&str>,
    body: &[u8],
) -> HookResponse {
    let write: argus_protocol::DecisionWrite = match serde_json::from_slice(body) {
        Ok(write) => write,
        Err(_) => {
            return HookResponse::text(400, "Bad Request", "not a decision".into());
        }
    };
    match daemon.record_agent_decision(source, session, write) {
        Ok(decision) => match serde_json::to_vec(&decision) {
            Ok(body) => HookResponse {
                code: 200,
                reason: "OK",
                body,
            },
            Err(e) => HookResponse::text(500, "Internal Server Error", e.to_string()),
        },
        Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
    }
}

fn features_response(daemon: &Arc<Daemon>, source: PaneId) -> anyhow::Result<HookResponse> {
    Ok(match daemon.feature_board_for_agent(source) {
        Ok(board) => HookResponse {
            code: 200,
            reason: "OK",
            body: serde_json::to_vec(&board)?,
        },
        Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
    })
}

/// Answers with the board as it stands afterwards, because every one of
/// these changes what the next `decide` from this checkout is filed under
/// — the agent has to be able to see where it now is.
fn feature_response(
    daemon: &Arc<Daemon>,
    source: PaneId,
    session: Option<&str>,
    body: &[u8],
) -> HookResponse {
    use argus_protocol::FeatureAction;

    let action: FeatureAction = match serde_json::from_slice(body) {
        Ok(action) => action,
        Err(_) => return HookResponse::text(400, "Bad Request", "not a feature change".into()),
    };
    let board = match action {
        FeatureAction::Open(write) => daemon.open_feature_for_agent(source, session, write),
        FeatureAction::Select { slug } => daemon.select_feature_for_agent(source, &slug),
        FeatureAction::Append { text } => daemon.append_to_feature_for_agent(source, &text),
    };
    match board.and_then(|board| Ok(serde_json::to_vec(&board)?)) {
        Ok(body) => HookResponse {
            code: 200,
            reason: "OK",
            body,
        },
        Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
    }
}

fn comments_response(daemon: &Arc<Daemon>, source: PaneId) -> anyhow::Result<HookResponse> {
    Ok(match daemon.review_comments_for_agent(source) {
        Ok(comments) => HookResponse {
            code: 200,
            reason: "OK",
            body: serde_json::to_vec(&comments)?,
        },
        Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
    })
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
