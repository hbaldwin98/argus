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

use argus_protocol::hook_call::{self, Call, Form, Json};
use argus_protocol::{parse_request_target, Endpoint, PaneId, MAX_HOOK_BODY};

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

/// A body as `F` reads it, or the 400 that says what it is not, in the
/// reader's own words: an agent that sent a block kind the schema lacks is
/// told which.
fn read_as<T, F: Form<T>>(body: &[u8], noun: &str) -> Result<T, HookResponse> {
    F::read(body).map_err(|why| HookResponse::text(400, "Bad Request", format!("not a {noun}: {why}")))
}

fn read<C: Call>(body: &[u8]) -> Result<C::Body, HookResponse> {
    read_as::<_, C::BodyForm>(body, C::BODY)
}

/// The answer as `F` writes it; a refusal is a 409 carrying the daemon's
/// words, for the agent to put in front of the person.
fn written<T, F: Form<T>>(result: anyhow::Result<T>) -> HookResponse {
    let value = match result {
        Ok(value) => value,
        Err(error) => return HookResponse::text(409, "Conflict", error.to_string()),
    };
    match F::write(&value) {
        Ok(body) => HookResponse { code: 200, reason: "OK", body },
        Err(why) => HookResponse::text(500, "Internal Server Error", why),
    }
}

/// The call's reply, 200 with its form's bytes (none for `Empty`).
fn reply<C: Call>(result: anyhow::Result<C::Reply>) -> HookResponse {
    written::<_, C::ReplyForm>(result)
}

/// An extension's answer, which has no declaration: its other side is any
/// language, not a Rust binary.
fn reply_json<T: serde::Serialize + serde::de::DeserializeOwned>(result: anyhow::Result<T>) -> HookResponse {
    written::<_, Json>(result)
}

/// Read, then run, then reply: every synchronous call.
fn answer<C: Call>(body: &[u8], run: impl FnOnce(C::Body) -> anyhow::Result<C::Reply>) -> HookResponse {
    match read::<C>(body) {
        Ok(body) => reply::<C>(run(body)),
        Err(refusal) => refusal,
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
    let max_body = endpoint.map_or(MAX_HOOK_BODY, |(_, endpoint)| endpoint.max_body());
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
        HookResponse::text(413, "Content Too Large", format!("request body is too large: at most {max_body} bytes"))
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
            Some((pane, Endpoint::Checkout)) => match read::<hook_call::Checkout>(&body) {
                Ok(directory) => {
                    let destination = PathBuf::from(directory.trim());
                    let daemon = daemon.clone();
                    let reporter = reporter.clone();
                    let moved = tokio::task::spawn_blocking(move || {
                        daemon.move_agent_into(pane, reporter.as_deref(), &destination)
                    })
                    .await
                    .unwrap_or_else(|error| Err(error.into()));
                    reply::<hook_call::Checkout>(moved)
                }
                Err(refusal) => refusal,
            },
            Some((pane, Endpoint::Session)) => {
                daemon.set_pane_session_id(pane, &String::from_utf8_lossy(&body));
                HookResponse::empty(200, "OK")
            }
            Some((pane, Endpoint::Comments)) => {
                reply::<hook_call::Comments>(daemon.review_comments_for_agent(pane))
            }
            Some((pane, Endpoint::Decisions)) => {
                reply::<hook_call::Decisions>(daemon.decisions_for_agent(pane, filing))
            }
            Some((pane, Endpoint::Decide)) => answer::<hook_call::Decide>(&body, |write| {
                daemon.record_agent_decision(pane, reporter.as_deref(), write, filing)
            }),
            Some((pane, Endpoint::DecisionChange)) => answer::<hook_call::ChangeDecision>(&body, |change| {
                daemon.change_decision_for_agent(pane, change, filing)
            }),
            Some((pane, Endpoint::Features)) => {
                reply::<hook_call::Features>(daemon.feature_board_for_agent(pane, filing))
            }
            Some((pane, Endpoint::Feature)) => answer::<hook_call::ChangeFeature>(&body, |action| {
                daemon.feature_action_for_agent(pane, reporter.as_deref(), action, filing)
            }),
            Some((pane, Endpoint::Telemetry)) => answer::<hook_call::Telemetry>(&body, |report| {
                daemon.report_pane_telemetry(pane, reporter.as_deref(), report);
                Ok(())
            }),
            Some((pane, Endpoint::Tasks)) => answer::<hook_call::Tasks>(&body, |action| {
                daemon.part_action_for_agent(pane, reporter.as_deref(), action, filing)
            }),
            Some((pane, Endpoint::Diagrams)) => answer::<hook_call::Diagrams>(&body, |action| {
                daemon.part_action_for_agent(pane, reporter.as_deref(), action, filing)
            }),
            Some((pane, Endpoint::Reconcile)) => answer::<hook_call::Reconcile>(&body, |change| {
                daemon.reconcile_for_agent(pane, change, filing)
            }),
            Some((pane, Endpoint::TrackerMap)) => answer::<hook_call::TrackerMap>(&body, |draft| {
                daemon.tracker_map_for_agent(pane, &draft, filing.scope)
            }),
            Some((pane, Endpoint::View)) => {
                answer::<hook_call::ChangeView>(&body, |change| daemon.change_view_for_agent(pane, change))
            }
            Some((pane, Endpoint::SavedView)) => {
                answer::<hook_call::SavedView>(&body, |change| daemon.saved_view_for_agent(pane, change))
            }
            Some((pane, Endpoint::Agent)) => match read::<hook_call::Launch>(&body) {
                Ok(launch) => reply::<hook_call::Launch>(daemon.launch_agent_for_agent(pane, launch).await),
                Err(refusal) => refusal,
            },
            _ => HookResponse::empty(200, "OK"),
        }
    };
    wr.write_all(&response.bytes()).await?;
    Ok(())
}

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
    use argus_protocol::extensions::MAX_EXTENSION_BODY;
    use argus_protocol::{ExtDialogRaised, ExtEndpoint, ExtPaneOpened};
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
            ExtEndpoint::View => match read_views(&body) {
                Ok(views) => reply_json(daemon.publish_views(name, views)),
                Err(refusal) => refusal,
            },
            ExtEndpoint::Commands => match read_as::<Vec<argus_protocol::ExtCommand>, Json>(&body, "list of commands") {
                Ok(commands) => reply_json(daemon.publish_commands(name, commands)),
                Err(refusal) => refusal,
            },
            ExtEndpoint::Dialog => match read_as::<argus_protocol::ExtDialog, Json>(&body, "dialog") {
                Ok(dialog) => reply_json(daemon.raise_dialog(name, dialog).map(|dialog| ExtDialogRaised { dialog })),
                Err(refusal) => refusal,
            },
            ExtEndpoint::Notice => match daemon.extension_notice(name, &String::from_utf8_lossy(&body)) {
                Ok(()) => HookResponse::empty(200, "OK"),
                Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
            },
            ExtEndpoint::About => match read_as::<argus_protocol::ExtAbout, Json>(&body, "summary and guide") {
                Ok(about) => match daemon.publish_about(name, about) {
                    Ok(()) => HookResponse::empty(200, "OK"),
                    Err(error) => HookResponse::text(409, "Conflict", error.to_string()),
                },
                Err(refusal) => refusal,
            },
            ExtEndpoint::Pane => match read_as::<argus_protocol::ExtPane, Json>(&body, "pane") {
                Ok(pane) => {
                    let (daemon, name) = (daemon.clone(), name.to_string());
                    let opened = tokio::task::spawn_blocking(move || daemon.open_extension_pane(&name, pane))
                        .await
                        .unwrap_or_else(|error| Err(error.into()));
                    reply_json(opened.map(|pane| ExtPaneOpened { pane }))
                }
                Err(refusal) => refusal,
            },
        }
    };
    wr.write_all(&response.bytes()).await?;
    Ok(())
}

/// A whole model, or one view of it: an extension with one view need not
/// wrap it in a list. Read as a list or as one view by its first byte,
/// rather than as an untagged pair, whose refusal says only that neither
/// matched.
fn read_views(body: &[u8]) -> Result<Vec<argus_protocol::ExtView>, HookResponse> {
    if body.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'[') {
        read_as::<_, Json>(body, "view")
    } else {
        read_as::<_, Json>(body, "view").map(|view| vec![view])
    }
}

/// Holds an extension's event stream open, as a pane's inbox is held: the
/// newest connection takes over, and it closes the moment the extension's
/// end of the socket does.
async fn serve_extension_events<R, W>(rd: R, mut wr: W, daemon: Arc<Daemon>, name: &str) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;

    let Some((generation, events)) = daemon.open_extension_events(name) else {
        wr.write_all(&HookResponse::empty(409, "Conflict").bytes()).await?;
        return Ok(());
    };
    let ended = serve_events(rd, wr, events).await;
    daemon.close_extension_events(name, generation);
    ended
}

/// Holds a pane's inbox open, one server-sent event per item, until the
/// plugin hangs up or a newer connection takes the pane.
async fn serve_inbox<R, W>(
    rd: R,
    mut wr: W,
    daemon: Arc<Daemon>,
    pane: PaneId,
    reporter: Option<String>,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;

    let Some((generation, items)) = daemon.open_inbox(pane, reporter.as_deref()) else {
        wr.write_all(&HookResponse::empty(409, "Conflict").bytes()).await?;
        return Ok(());
    };
    let ended = serve_events(rd, wr, items).await;
    daemon.close_inbox(pane, generation);
    ended
}

/// How often a quiet event stream says it is still there, so the other end
/// notices a daemon that went away and the daemon notices one that did.
const HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(15);

/// An event stream's head, then one server-sent event per item and a
/// heartbeat while it is quiet, until the channel closes or the other end
/// goes. Whatever ends it, the caller closes its end of the bookkeeping.
///
/// The other end never writes after its request, so anything read — its
/// end of the socket closing — means it has gone, and the stream ends at
/// once rather than at the next item, which would otherwise be sent into a
/// connection nobody holds and lost. An item that cannot be written as JSON
/// ends the stream the same way.
async fn serve_events<T, R, W>(
    mut rd: R,
    mut wr: W,
    mut events: tokio::sync::mpsc::UnboundedReceiver<T>,
) -> anyhow::Result<()>
where
    T: serde::Serialize,
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\r\n";
    wr.write_all(head.as_bytes()).await?;
    wr.flush().await?;
    let mut scratch = [0u8; 64];
    loop {
        let event = tokio::select! {
            event = events.recv() => match event.map(|event| serde_json::to_string(&event)) {
                Some(Ok(event)) => format!("data: {event}\n\n"),
                Some(Err(error)) => return Err(error.into()),
                None => return Ok(()),
            },
            _ = rd.read(&mut scratch) => return Ok(()),
            _ = tokio::time::sleep(HEARTBEAT) => ": still here\n\n".to_string(),
        };
        wr.write_all(event.as_bytes()).await?;
        wr.flush().await?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    /// An item no JSON can say.
    struct Unsayable;

    impl serde::Serialize for Unsayable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("unsayable"))
        }
    }

    #[tokio::test]
    async fn an_event_stream_ends_on_an_item_it_cannot_write() {
        // It used to return past the caller's close, leaving the stream's
        // generation open until a newer connection replaced it.
        let (client, server) = tokio::io::duplex(4096);
        let (rd, wr) = tokio::io::split(server);
        let (tx, events) = tokio::sync::mpsc::unbounded_channel();
        tx.send(Unsayable).unwrap();

        let ended = tokio::time::timeout(std::time::Duration::from_secs(1), serve_events(rd, wr, events)).await;
        assert!(ended.expect("the stream ended").is_err());
        drop(tx);

        let mut seen = String::new();
        let (mut client, _) = tokio::io::split(client);
        let _ = client.read_to_string(&mut seen).await;
        assert!(seen.starts_with("HTTP/1.1 200 OK"), "{seen}");
        assert!(!seen.contains("data:"), "{seen}");
    }

    #[tokio::test]
    async fn an_event_stream_ends_when_its_channel_closes() {
        let (client, server) = tokio::io::duplex(4096);
        let (rd, wr) = tokio::io::split(server);
        let (tx, events) = tokio::sync::mpsc::unbounded_channel();
        tx.send("one").unwrap();
        drop(tx);

        let ended = tokio::time::timeout(std::time::Duration::from_secs(1), serve_events(rd, wr, events)).await;
        assert!(ended.expect("the stream ended").is_ok());

        let mut seen = String::new();
        let (mut client, _) = tokio::io::split(client);
        let _ = client.read_to_string(&mut seen).await;
        assert!(seen.ends_with("data: \"one\"\n\n"), "{seen}");
    }
}
