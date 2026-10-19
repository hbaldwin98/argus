//! The loopback pane API: which daemon call each agent's or extension's
//! request reaches, behind its token.
//!
//! Not part of the daemon's state because it is a protocol surface, the
//! same reason `harness`, `git::watch` and `connection` are their own
//! modules: what it calls on the `Daemon` is the `pub(crate)` surface the
//! pane API needs.
//! The one dependency the other way is `gen_token` and `valid_session_id`,
//! which state calls because what a token and a session header are is
//! answered here. Its grammar lives further out still, in
//! `argus_protocol::hook`, and what each endpoint is posted and answers
//! with in `argus_protocol::hook::call`, since `argus-hook` builds both.
//!
//! The server binds loopback only and checks a per-boot bearer token, which
//! is all that stands between a pane's status and any other local process.

use std::path::PathBuf;
use std::sync::Arc;

use argus_protocol::hook::call::{self, Call, Form, Json};
use argus_protocol::{parse_request_target, Endpoint, PaneId, MAX_HOOK_BODY};

use crate::state::{Daemon, Filing};

mod http;

use http::{Request, Response};

/// Binds the loopback listener and serves it in the background, answering
/// with the port it took. The bind is synchronous, so the port is known
/// before the caller goes on.
pub(crate) fn start(daemon: Arc<Daemon>) -> anyhow::Result<u16> {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    std_listener.set_nonblocking(true)?;
    let port = std_listener.local_addr()?.port();
    let listener = tokio::net::TcpListener::from_std(std_listener)?;

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
    Ok(port)
}

/// The shortest and longest a failed accept waits before trying again.
const ACCEPT_BACKOFF_MIN: std::time::Duration = std::time::Duration::from_millis(50);
const ACCEPT_BACKOFF_MAX: std::time::Duration = std::time::Duration::from_secs(2);

/// A body as `F` reads it, or the 400 that says what it is not, in the
/// reader's own words: an agent that sent a block kind the schema lacks is
/// told which.
fn read_as<T, F: Form<T>>(body: &[u8], noun: &str) -> Result<T, Response> {
    F::read(body).map_err(|why| Response::text(400, "Bad Request", format!("not a {noun}: {why}")))
}

fn read<C: Call>(body: &[u8]) -> Result<C::Body, Response> {
    read_as::<_, C::BodyForm>(body, C::BODY)
}

/// The answer as `F` writes it; a refusal is a 409 carrying the daemon's
/// words, for the agent to put in front of the person.
fn written<T, F: Form<T>>(result: anyhow::Result<T>) -> Response {
    let value = match result {
        Ok(value) => value,
        Err(error) => return Response::text(409, "Conflict", error.to_string()),
    };
    match F::write(&value) {
        Ok(body) => Response { code: 200, reason: "OK", body },
        Err(why) => Response::text(500, "Internal Server Error", why),
    }
}

/// The call's reply, 200 with its form's bytes (none for `Empty`).
fn reply<C: Call>(result: anyhow::Result<C::Reply>) -> Response {
    written::<_, C::ReplyForm>(result)
}

/// An extension's answer, which has no declaration: its other side is any
/// language, not a Rust binary.
fn reply_json<T: serde::Serialize + serde::de::DeserializeOwned>(result: anyhow::Result<T>) -> Response {
    written::<_, Json>(result)
}

/// Read, then run, then reply: every synchronous call.
fn answer<C: Call>(body: &[u8], run: impl FnOnce(C::Body) -> anyhow::Result<C::Reply>) -> Response {
    match read::<C>(body) {
        Ok(body) => reply::<C>(run(body)),
        Err(refusal) => refusal,
    }
}

async fn handle_hook_request(
    stream: tokio::net::TcpStream,
    daemon: Arc<Daemon>,
) -> anyhow::Result<()> {
    let (rd, mut wr) = tokio::io::split(stream);
    let mut reader = tokio::io::BufReader::new(rd);

    let request = http::read_head(&mut reader, daemon.hook_token()).await?;
    if let Some((name, endpoint)) = argus_protocol::parse_extension_path(&request.target) {
        let name = name.to_string();
        return serve_extension(reader, wr, daemon, &name, endpoint, request).await;
    }
    // The one request that is read rather than posted, and held open.
    if request.method == "GET" {
        if let Some((pane, Endpoint::Inbox)) = parse_request_target(&request.target).0 {
            if !request.authorized {
                return Response::empty(401, "Unauthorized").write(&mut wr).await;
            }
            return serve_inbox(reader, wr, daemon, pane, request.reporter).await;
        }
    }
    let path = if request.method == "POST" { request.target.as_str() } else { "" };
    let (endpoint, artifact_scope) = parse_request_target(path);
    let filing = Filing {
        scope: artifact_scope,
        feature: argus_protocol::requested_feature(path),
    };
    let reporter = request.reporter.clone();
    // The server trusts nothing about a request beyond its bearer token.
    let max_body = endpoint.map_or(MAX_HOOK_BODY, |(_, endpoint)| endpoint.max_body());
    let body = http::read_body(&mut reader, request.content_length, max_body, request.authorized).await;

    let response = match body {
        _ if !request.authorized => Response::empty(401, "Unauthorized"),
        None => Response::text(413, "Content Too Large", format!("request body is too large: at most {max_body} bytes")),
        Some(body) => match endpoint {
            Some((pane, Endpoint::Status(report))) => {
                let note = String::from_utf8_lossy(&body).to_string();
                daemon.report_pane_status(
                    pane,
                    reporter.as_deref(),
                    report.status(),
                    Some(note),
                    request.background,
                );
                let daemon = daemon.clone();
                let (reporter, directory) = (reporter.clone(), request.directory.clone());
                let _ = tokio::task::spawn_blocking(move || {
                    daemon.follow_status_report(
                        pane,
                        reporter.as_deref(),
                        report,
                        directory.as_deref(),
                    )
                })
                .await;
                Response::empty(200, "OK")
            }
            Some((pane, Endpoint::Title)) => {
                daemon.report_pane_title(
                    pane,
                    reporter.as_deref(),
                    &String::from_utf8_lossy(&body),
                );
                Response::empty(200, "OK")
            }
            Some((pane, Endpoint::Checkout)) => match read::<call::Checkout>(&body) {
                Ok(directory) => {
                    let destination = PathBuf::from(directory.trim());
                    let daemon = daemon.clone();
                    let reporter = reporter.clone();
                    let moved = tokio::task::spawn_blocking(move || {
                        daemon.move_agent_into(pane, reporter.as_deref(), &destination)
                    })
                    .await
                    .unwrap_or_else(|error| Err(error.into()));
                    reply::<call::Checkout>(moved)
                }
                Err(refusal) => refusal,
            },
            Some((pane, Endpoint::Session)) => {
                daemon.set_pane_session_id(pane, &String::from_utf8_lossy(&body));
                Response::empty(200, "OK")
            }
            Some((pane, Endpoint::Comments)) => {
                reply::<call::Comments>(daemon.review_comments_for_agent(pane))
            }
            Some((pane, Endpoint::Decisions)) => {
                reply::<call::Decisions>(daemon.decisions_for_agent(pane, filing))
            }
            Some((pane, Endpoint::Decide)) => answer::<call::Decide>(&body, |write| {
                daemon.record_agent_decision(pane, reporter.as_deref(), write, filing)
            }),
            Some((pane, Endpoint::DecisionChange)) => answer::<call::ChangeDecision>(&body, |change| {
                daemon.change_decision_for_agent(pane, change, filing)
            }),
            Some((pane, Endpoint::Features)) => {
                reply::<call::Features>(daemon.feature_board_for_agent(pane, filing))
            }
            Some((pane, Endpoint::Feature)) => answer::<call::ChangeFeature>(&body, |action| {
                daemon.feature_action_for_agent(pane, reporter.as_deref(), action, filing)
            }),
            Some((pane, Endpoint::Telemetry)) => answer::<call::Telemetry>(&body, |report| {
                daemon.report_pane_telemetry(pane, reporter.as_deref(), report);
                Ok(())
            }),
            Some((pane, Endpoint::Tasks)) => answer::<call::Tasks>(&body, |action| {
                daemon.part_action_for_agent(pane, reporter.as_deref(), action, filing)
            }),
            Some((pane, Endpoint::Diagrams)) => answer::<call::Diagrams>(&body, |action| {
                daemon.part_action_for_agent(pane, reporter.as_deref(), action, filing)
            }),
            Some((pane, Endpoint::Reconcile)) => answer::<call::Reconcile>(&body, |change| {
                daemon.reconcile_for_agent(pane, change, filing)
            }),
            Some((pane, Endpoint::TrackerMap)) => answer::<call::TrackerMap>(&body, |draft| {
                daemon.tracker_map_for_agent(pane, &draft, filing.scope)
            }),
            Some((pane, Endpoint::View)) => {
                answer::<call::ChangeView>(&body, |change| daemon.change_view_for_agent(pane, change))
            }
            Some((pane, Endpoint::SavedView)) => {
                answer::<call::SavedView>(&body, |change| daemon.saved_view_for_agent(pane, change))
            }
            Some((pane, Endpoint::Agents)) => reply::<call::Agents>(daemon.agents_for_agent(pane)),
            Some((pane, Endpoint::Agent)) => match read::<call::Launch>(&body) {
                Ok(launch) => reply::<call::Launch>(daemon.launch_agent_for_agent(pane, launch).await),
                Err(refusal) => refusal,
            },
            _ => Response::empty(200, "OK"),
        },
    };
    response.write(&mut wr).await
}

/// One request to an extension's half of the pane API, from reading its
/// body to writing the answer; the event stream is held open.
async fn serve_extension<R, W>(
    mut reader: R,
    mut wr: W,
    daemon: Arc<Daemon>,
    name: &str,
    endpoint: argus_protocol::ExtEndpoint,
    request: Request,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use argus_protocol::extensions::MAX_EXTENSION_BODY;
    use argus_protocol::{ExtDialogRaised, ExtEndpoint, ExtPaneOpened};

    let body = http::read_body(&mut reader, request.content_length, MAX_EXTENSION_BODY, false).await;
    let response = if !daemon.extension_authorized(name, request.bearer.as_deref()) {
        Response::empty(401, "Unauthorized")
    } else if request.method != endpoint.method() {
        Response::text(405, "Method Not Allowed", format!("{} takes {}", endpoint.suffix(), endpoint.method()))
    } else if let Some(body) = body {
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
                Ok(()) => Response::empty(200, "OK"),
                Err(error) => Response::text(409, "Conflict", error.to_string()),
            },
            ExtEndpoint::About => match read_as::<argus_protocol::ExtAbout, Json>(&body, "summary and guide") {
                Ok(about) => match daemon.publish_about(name, about) {
                    Ok(()) => Response::empty(200, "OK"),
                    Err(error) => Response::text(409, "Conflict", error.to_string()),
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
    } else {
        Response::text(413, "Content Too Large", "request body is too large".into())
    };
    response.write(&mut wr).await
}

/// A whole model, or one view of it: an extension with one view need not
/// wrap it in a list. Read as a list or as one view by its first byte,
/// rather than as an untagged pair, whose refusal says only that neither
/// matched.
fn read_views(body: &[u8]) -> Result<Vec<argus_protocol::ExtView>, Response> {
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
    let Some((generation, events)) = daemon.open_extension_events(name) else {
        return Response::empty(409, "Conflict").write(&mut wr).await;
    };
    let ended = http::serve_events(rd, wr, events).await;
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
    let Some((generation, items)) = daemon.open_inbox(pane, reporter.as_deref()) else {
        return Response::empty(409, "Conflict").write(&mut wr).await;
    };
    let ended = http::serve_events(rd, wr, items).await;
    daemon.close_inbox(pane, generation);
    ended
}

/// A harness session id is opaque to Argus — it only has to be one
/// nonempty, bounded, control-free line, since it goes on to enter a
/// child's argv.
pub(crate) fn valid_session_id(raw: &str) -> Option<String> {
    const MAX: usize = 512;
    let id = raw.trim();
    (!id.is_empty() && id.len() <= MAX && !id.chars().any(char::is_control)).then(|| id.to_string())
}

/// Not cryptographically strong — see the `hook_token` field on `Daemon` —
/// just enough entropy that it isn't a fixed, guessable string.
pub(crate) fn gen_token() -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_validated_without_restricting_cli_specific_syntax() {
        assert_eq!(
            valid_session_id("  thread/abc:123  ").as_deref(),
            Some("thread/abc:123")
        );
        assert!(valid_session_id("").is_none());
        assert!(valid_session_id("bad\nid").is_none());
        assert!(valid_session_id(&"x".repeat(513)).is_none());
    }

    #[test]
    fn gen_token_is_not_a_fixed_string() {
        assert_eq!(gen_token().len(), 32);
        assert_ne!(gen_token(), gen_token());
    }
}
