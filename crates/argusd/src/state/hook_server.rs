//! The loopback pane API: the HTTP receiver an agent's hooks POST to, and
//! an extension publishes through.
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
/// Diagram adds carry Mermaid source in JSON; allow room for encoding overhead.
const MAX_DIAGRAM_HOOK_BODY: usize = MAX_DIAGRAM_BODY_BYTES + 4096;

/// A decision is several bounded fields and two bounded lists, each entry
/// of which may run to the same bound; the room is for all of them full,
/// with JSON's quoting on top.
const MAX_DECISION_HOOK_BODY: usize = (6
    + argus_protocol::MAX_DECISION_OPTIONS
    + argus_protocol::MAX_DECISION_CONSEQUENCES)
    * argus_protocol::MAX_DECISION_BYTES
    * 2;

fn max_hook_body(endpoint: Option<(PaneId, Endpoint)>) -> usize {
    match endpoint {
        Some((_, Endpoint::Decide | Endpoint::DecisionChange)) => MAX_DECISION_HOOK_BODY,
        Some((_, Endpoint::Diagrams)) => MAX_DIAGRAM_HOOK_BODY,
        // Room for a map at its bound, as UTF-8 that the server refuses
        // past it in words rather than as a body cut off.
        Some((_, Endpoint::TrackerMap)) => argus_protocol::MAX_TRACKER_MAP_BYTES + 1024,
        // The view's own bound, so the 413 is the refusal of a dump.
        Some((_, Endpoint::View)) => argus_protocol::MAX_VIEW_BYTES,
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
    if let Some((name, endpoint)) = argus_protocol::parse_extension_path(&target) {
        let name = name.to_string();
        return serve_extension(reader, wr, daemon, method, &name, endpoint, headers).await;
    }
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
        (headers.authorized, headers.content_length, headers.reporter.clone());
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
    } else if too_large && authorized {
        // Read past what is refused, within reason: closing on a body a
        // client is still sending resets the connection, and the client
        // sees that rather than the refusal. A body as big as a view's
        // bound is the case that matters.
        const DRAIN: u64 = 4 * 1024 * 1024;
        let unread = (&mut reader).take((content_length as u64).min(DRAIN));
        let _ = tokio::io::copy(&mut { unread }, &mut tokio::io::sink()).await;
    }

    let response = if !authorized {
        HookResponse::empty(401, "Unauthorized")
    } else if too_large {
        HookResponse::text(413, "Content Too Large", "request body is too large".into())
    } else {
        match endpoint {
            Some((pane, Endpoint::Status(report))) => {
                let note = String::from_utf8_lossy(&body).to_string();
                daemon.report_pane_status(
                    pane,
                    reporter.as_deref(),
                    report.status(),
                    Some(note),
                    headers.background,
                );
                let daemon = daemon.clone();
                let (reporter, directory) = (reporter.clone(), headers.directory.clone());
                let _ = tokio::task::spawn_blocking(move || {
                    daemon.follow_status_report(
                        pane,
                        reporter.as_deref(),
                        report,
                        directory.as_deref(),
                    )
                })
                .await;
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
            Some((pane, Endpoint::Checkout)) => {
                let destination = PathBuf::from(String::from_utf8_lossy(&body).trim());
                let daemon = daemon.clone();
                let reporter = reporter.clone();
                let moved = tokio::task::spawn_blocking(move || {
                    daemon.move_agent_into(pane, reporter.as_deref(), &destination)
                })
                .await
                .unwrap_or_else(|error| Err(error.into()));
                match moved {
                    Ok(()) => HookResponse::empty(200, "OK"),
                    Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
                }
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
            Some((pane, Endpoint::Tasks)) => {
                tasks_response(&daemon, pane, reporter.as_deref(), &body, filing)
            }
            Some((pane, Endpoint::Diagrams)) => {
                diagrams_response(&daemon, pane, reporter.as_deref(), &body, filing)
            }
            Some((pane, Endpoint::TrackerMap)) => json_reply(daemon.tracker_map_for_agent(
                pane,
                &String::from_utf8_lossy(&body),
                filing.scope,
            )),
            // The parse error rides along: an agent that sent a block kind
            // the schema lacks needs to be told which.
            Some((pane, Endpoint::View)) => match serde_json::from_slice(&body) {
                Ok(change) => match daemon.change_view_for_agent(pane, change) {
                    Ok(()) => HookResponse::empty(200, "OK"),
                    Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
                },
                Err(error) => {
                    HookResponse::text(400, "Bad Request", format!("not a view change: {error}"))
                }
            },
            _ => HookResponse::empty(200, "OK"),
        }
    };
    wr.write_all(&response.bytes()).await?;
    Ok(())
}

/// Room for a few views each as large as an agent's may be; past it the
/// extension is dumping, not choosing what to show.
const MAX_EXTENSION_BODY: usize = 4 * argus_protocol::MAX_VIEW_BYTES;

/// One request to an extension's half of the pane API, from reading its
/// body to writing the answer; the event stream is held open.
async fn serve_extension<R, W>(
    mut reader: R,
    mut wr: W,
    daemon: Arc<Daemon>,
    method: &str,
    name: &str,
    endpoint: argus_protocol::ExtEndpoint,
    headers: HookHeaders,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use argus_protocol::ExtEndpoint;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let too_large = headers.content_length > MAX_EXTENSION_BODY;
    let mut body = vec![0u8; if too_large { 0 } else { headers.content_length }];
    if !body.is_empty() {
        let _ = reader.read_exact(&mut body).await;
    }
    let response = if !daemon.extension_authorized(name, headers.bearer.as_deref()) {
        HookResponse::empty(401, "Unauthorized")
    } else if method != endpoint.method() {
        HookResponse::text(405, "Method Not Allowed", format!("{} takes {}", endpoint.suffix(), endpoint.method()))
    } else if too_large {
        HookResponse::text(413, "Content Too Large", "request body is too large".into())
    } else {
        match endpoint {
            ExtEndpoint::Events => return serve_extension_events(reader, wr, daemon, name).await,
            ExtEndpoint::View => match decode::<OneOrMany<argus_protocol::ExtView>>(&body, "view") {
                Ok(views) => json_reply(daemon.publish_views(name, views.into_vec())),
                Err(refusal) => refusal,
            },
            ExtEndpoint::Commands => match decode::<Vec<argus_protocol::ExtCommand>>(&body, "list of commands") {
                Ok(commands) => json_reply(daemon.publish_commands(name, commands)),
                Err(refusal) => refusal,
            },
            ExtEndpoint::Dialog => match decode::<argus_protocol::ExtDialog>(&body, "dialog") {
                Ok(dialog) => json_reply(daemon.raise_dialog(name, dialog).map(|id| serde_json::json!({ "dialog": id }))),
                Err(refusal) => refusal,
            },
            ExtEndpoint::Notice => match daemon.extension_notice(name, &String::from_utf8_lossy(&body)) {
                Ok(()) => HookResponse::empty(200, "OK"),
                Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
            },
            ExtEndpoint::Pane => match decode::<argus_protocol::ExtPane>(&body, "pane") {
                Ok(pane) => {
                    let (daemon, name) = (daemon.clone(), name.to_string());
                    let opened = tokio::task::spawn_blocking(move || daemon.open_extension_pane(&name, pane))
                        .await
                        .unwrap_or_else(|error| Err(error.into()));
                    json_reply(opened.map(|pane| serde_json::json!({ "pane": pane.0 })))
                }
                Err(refusal) => refusal,
            },
        }
    };
    wr.write_all(&response.bytes()).await?;
    Ok(())
}

/// A whole model, or one view of it: an extension with one view need not
/// wrap it in a list.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    Many(Vec<T>),
    One(T),
}

impl<T> OneOrMany<T> {
    fn into_vec(self) -> Vec<T> {
        match self {
            OneOrMany::Many(all) => all,
            OneOrMany::One(one) => vec![one],
        }
    }
}

/// Holds an extension's event stream open, as a pane's inbox is held: the
/// newest connection takes over, and it closes the moment the extension's
/// end of the socket does.
async fn serve_extension_events<R, W>(mut rd: R, mut wr: W, daemon: Arc<Daemon>, name: &str) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let Some((generation, mut events)) = daemon.open_extension_events(name) else {
        wr.write_all(&HookResponse::empty(409, "Conflict").bytes()).await?;
        return Ok(());
    };
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\r\n";
    let mut result = async {
        wr.write_all(head.as_bytes()).await?;
        wr.flush().await
    }
    .await;
    let mut scratch = [0u8; 64];
    while result.is_ok() {
        let event = tokio::select! {
            event = events.recv() => match event {
                Some(event) => format!("data: {}\n\n", serde_json::to_string(&event)?),
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
    daemon.close_extension_events(name, generation);
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
    /// The token the request carried, for an extension's, which is its own
    /// rather than the per-boot one.
    bearer: Option<String>,
    content_length: usize,
    /// The conversation the report comes from.
    reporter: Option<String>,
    /// Whether that conversation left work running behind its turn.
    background: bool,
    /// Where that conversation is working.
    directory: Option<PathBuf>,
}

async fn read_hook_headers<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    token: &str,
) -> anyhow::Result<HookHeaders> {
    use tokio::io::AsyncBufReadExt;

    let mut headers = HookHeaders {
        authorized: false,
        bearer: None,
        content_length: 0,
        reporter: None,
        background: false,
        directory: None,
    };
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(v) = strip_header(&line, "Authorization") {
            headers.authorized = v.eq_ignore_ascii_case(&format!("Bearer {token}"));
            headers.bearer = v
                .split_once(' ')
                .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
                .map(|(_, token)| token.trim().to_string());
        } else if let Some(v) = strip_header(&line, "Content-Length") {
            headers.content_length = v.parse().unwrap_or(0);
        } else if let Some(v) = strip_header(&line, argus_protocol::SESSION_HEADER) {
            headers.reporter = valid_session_id(v);
        } else if let Some(v) = strip_header(&line, argus_protocol::BACKGROUND_HEADER) {
            headers.background = !v.is_empty();
        } else if let Some(v) = strip_header(&line, argus_protocol::DIRECTORY_HEADER) {
            headers.directory = (!v.is_empty()).then(|| PathBuf::from(v));
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

/// Answers with the board as it stands afterwards; the daemon's
/// `feature_action_for_agent` says why.
fn feature_response(
    daemon: &Arc<Daemon>,
    source: PaneId,
    session: Option<&str>,
    body: &[u8],
    filing: super::features::Filing,
) -> HookResponse {
    match decode::<argus_protocol::FeatureAction>(body, "feature change") {
        Ok(action) => json_reply(daemon.feature_action_for_agent(source, session, action, filing)),
        Err(refusal) => refusal,
    }
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
