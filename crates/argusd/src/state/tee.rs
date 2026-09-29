//! The loopback proxy a live Claude pane sends its API traffic through, and
//! the draft read off each reply as it streams.
//!
//! Claude Code writes a reply to its transcript only once each block is
//! finished, and runs no server a second client could join, but it sends
//! its requests wherever `ANTHROPIC_BASE_URL` says — with its own login, as
//! it would to Anthropic. So a pane whose template asks for its live
//! channel is pointed at this proxy, which forwards every request untouched
//! to the API (or to the gateway the pane already named) and returns every
//! reply untouched, reading the streaming ones as they pass. The login goes
//! through and is never kept. The path each pane is given carries its id
//! and the daemon's per-boot token, so nothing else on the machine can put
//! words in a pane's draft; requests only ever go to the pane's own
//! upstream.

use std::pin::Pin;
use std::task::{Context, Poll};

use argus_protocol::{Draft, Update};
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Frame, Incoming};
use hyper::{Request, Response, StatusCode};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};

use super::*;
use crate::harness::live::claude::{worth_drafting, Events, Reading};

/// Where a pane's requests go when it named no gateway of its own.
pub(super) const ANTHROPIC_API: &str = "https://api.anthropic.com";

/// The variable pointing Claude Code at its API.
const BASE_URL_VAR: &str = "ANTHROPIC_BASE_URL";

/// Tells Claude Code its base URL is Anthropic's own, which through this
/// proxy it is. Without it a custom base URL counts as a third-party
/// gateway and switches off Remote Control, cloud sessions and the managed
/// policy fetch. Internal to Claude Code, so a release may drop it; what
/// then goes is those features in live panes, nothing else.
const FIRST_PARTY_VAR: &str = "_CLAUDE_CODE_ASSUME_FIRST_PARTY_BASE_URL";

/// The path a pane's base URL starts with, ahead of its id and the token.
const PREFIX: &str = "/claude/";

/// The least time between two pieces of a draft sent to watchers; what
/// arrives between is sent together. A reply streams many small pieces a
/// second, more than a phone needs to be told.
const DRAFT_EVERY: Duration = Duration::from_millis(100);

type Upstream = Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>;
type Reply = Response<BoxBody<Bytes, hyper::Error>>;

impl Daemon {
    /// Binds the proxy and starts serving it. Its port goes into every live
    /// Claude pane's environment, so it must be bound before one starts.
    pub fn start_tee(self: &Arc<Self>) -> anyhow::Result<()> {
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        std_listener.set_nonblocking(true)?;
        let port = std_listener.local_addr()?.port();
        let listener = tokio::net::TcpListener::from_std(std_listener)?;
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .build();
        let upstream: Upstream = Client::builder(TokioExecutor::new()).build(https);
        self.tee_port.store(port, std::sync::atomic::Ordering::Relaxed);

        let daemon = self.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                };
                let (daemon, upstream) = (daemon.clone(), upstream.clone());
                tokio::spawn(async move {
                    let service = hyper::service::service_fn(move |request| {
                        forward(daemon.clone(), upstream.clone(), request)
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        Ok(())
    }

    /// Points a pane about to start at the proxy, remembering where its
    /// requests are to go: the gateway its environment already named, or
    /// Anthropic's API. Says whether it did; a daemon whose proxy is not up
    /// leaves the pane as it was.
    pub(super) fn tee_env(&self, pane: PaneId, env: &mut Vec<(String, String)>) -> bool {
        let port = self.tee_port.load(std::sync::atomic::Ordering::Relaxed);
        if port == 0 {
            return false;
        }
        let upstream = env
            .iter()
            .find(|(key, _)| key == BASE_URL_VAR)
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var(BASE_URL_VAR).ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| ANTHROPIC_API.to_string());
        let first_party = upstream.trim_end_matches('/') == ANTHROPIC_API;
        self.tee_upstreams.lock().unwrap().insert(pane, upstream);

        env.retain(|(key, _)| key != BASE_URL_VAR && key != FIRST_PARTY_VAR);
        let base = format!("http://127.0.0.1:{port}{PREFIX}{}/{}", pane.0, self.hook_token);
        env.push((BASE_URL_VAR.to_string(), base));
        // Only a pane whose requests really reach Anthropic is told they do.
        if first_party {
            env.push((FIRST_PARTY_VAR.to_string(), "1".to_string()));
        }
        true
    }

    pub(super) fn forget_tee(&self, pane: PaneId) {
        self.tee_upstreams.lock().unwrap().remove(&pane);
    }

    /// Applies draft changes to a pane and hands them to whoever watches
    /// it. The pane keeps the draft in progress, so a watcher arriving
    /// mid-reply is sent what has been written so far.
    pub(super) fn draft(&self, pane: PaneId, changes: Vec<Draft>) {
        if changes.is_empty() {
            return;
        }
        {
            let mut inner = self.inner.lock().unwrap();
            let Some(p) = find_pane(&mut inner.projects, pane) else {
                return;
            };
            for change in &changes {
                match change {
                    Draft::Start { thinking } => p.draft = Some((*thinking, String::new())),
                    Draft::More { text } => {
                        if let Some((_, held)) = &mut p.draft {
                            held.push_str(text);
                        }
                    }
                    Draft::Done => p.draft = None,
                }
            }
        }
        self.tell_watchers(pane, false, changes.into_iter().map(Update::Draft).collect());
    }

    /// The draft a pane has in progress, as the changes that make it.
    pub(super) fn draft_so_far(&self, pane: PaneId) -> Vec<Update> {
        let inner = self.inner.lock().unwrap();
        match find_pane_ref(&inner.projects, pane).and_then(|p| p.draft.clone()) {
            Some((thinking, text)) => vec![
                Update::Draft(Draft::Start { thinking }),
                Update::Draft(Draft::More { text }),
            ],
            None => Vec::new(),
        }
    }
}

/// The pane and token a request's path names, and the path it is for
/// upstream.
fn route(path_and_query: &str) -> Option<(PaneId, &str, &str)> {
    let rest = path_and_query.strip_prefix(PREFIX)?;
    let (pane, rest) = rest.split_once('/')?;
    let (token, rest) = match rest.split_once(['/', '?']) {
        Some((token, _)) => (token, &rest[token.len()..]),
        None => (rest, ""),
    };
    Some((PaneId(pane.parse().ok()?), token, rest))
}

fn refuse(status: StatusCode, why: &'static str) -> Reply {
    let body = Full::new(Bytes::from_static(why.as_bytes()))
        .map_err(|never| match never {})
        .boxed();
    let mut reply = Response::new(body);
    *reply.status_mut() = status;
    reply
}

/// Headers about one hop, which a proxy answers for itself rather than
/// passing on.
fn hop_by_hop(name: &hyper::header::HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

/// One request from a pane, sent on to its upstream, and the reply back.
async fn forward(
    daemon: Arc<Daemon>,
    upstream: Upstream,
    request: Request<Incoming>,
) -> Result<Reply, std::convert::Infallible> {
    let target = request
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    let Some((pane, token, rest)) = route(&target) else {
        return Ok(refuse(StatusCode::NOT_FOUND, "not a pane's path"));
    };
    if token != daemon.hook_token {
        return Ok(refuse(StatusCode::FORBIDDEN, "wrong token"));
    }
    let Some(base) = daemon.tee_upstreams.lock().unwrap().get(&pane).cloned() else {
        return Ok(refuse(StatusCode::GONE, "that pane is gone"));
    };
    let Ok(uri) = format!("{}{rest}", base.trim_end_matches('/')).parse::<hyper::Uri>() else {
        return Ok(refuse(StatusCode::BAD_GATEWAY, "the pane's upstream is not a URL"));
    };

    let (parts, body) = request.into_parts();
    let Ok(body) = body.collect().await.map(|b| b.to_bytes()) else {
        return Ok(refuse(StatusCode::BAD_REQUEST, "the request ended early"));
    };
    let drafting = worth_drafting(&body);
    let mut onward = Request::builder().method(parts.method).uri(uri);
    for (name, value) in &parts.headers {
        if !hop_by_hop(name) && name != hyper::header::ACCEPT_ENCODING {
            onward = onward.header(name, value);
        }
    }
    // Uncompressed, so the stream can be read as it passes.
    onward = onward.header(hyper::header::ACCEPT_ENCODING, "identity");
    let Ok(onward) = onward.body(Full::new(body)) else {
        return Ok(refuse(StatusCode::BAD_REQUEST, "not a request that can be sent on"));
    };

    let reply = match upstream.request(onward).await {
        Ok(reply) => reply,
        Err(error) => {
            tracing::warn!("pane {}'s API request failed: {error}", pane.0);
            return Ok(refuse(StatusCode::BAD_GATEWAY, "the API could not be reached"));
        }
    };
    let streaming = reply
        .headers()
        .get(hyper::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"));
    let (mut parts, body) = reply.into_parts();
    parts.headers.remove(hyper::header::TRANSFER_ENCODING);
    parts.headers.remove(hyper::header::CONNECTION);
    let body = if drafting && streaming {
        Teed::new(daemon, pane, body).boxed()
    } else {
        body.boxed()
    };
    Ok(Response::from_parts(parts, body))
}

/// A streaming reply on its way back to the pane, read into its draft as
/// each piece passes.
struct Teed {
    inner: Incoming,
    daemon: Arc<Daemon>,
    pane: PaneId,
    events: Events,
    reading: Reading,
    /// Changes read and not yet sent, and when some last were.
    pending: Vec<Draft>,
    sent: std::time::Instant,
}

impl Teed {
    fn new(daemon: Arc<Daemon>, pane: PaneId, inner: Incoming) -> Teed {
        Teed {
            inner,
            daemon,
            pane,
            events: Events::default(),
            reading: Reading::default(),
            pending: Vec::new(),
            sent: std::time::Instant::now(),
        }
    }

    fn read(&mut self, bytes: &[u8]) {
        for event in self.events.feed(bytes) {
            self.pending.extend(self.reading.event(&event));
        }
        // A start or an end goes at once; pieces of text wait a moment.
        let settled = self.pending.iter().any(|d| !matches!(d, Draft::More { .. }));
        if settled || self.sent.elapsed() >= DRAFT_EVERY {
            self.send();
        }
    }

    fn send(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        self.sent = std::time::Instant::now();
        let changes = merge(std::mem::take(&mut self.pending));
        self.daemon.draft(self.pane, changes);
    }
}

/// Runs of pieces joined into one, so a burst goes as a single change.
fn merge(changes: Vec<Draft>) -> Vec<Draft> {
    let mut merged: Vec<Draft> = Vec::with_capacity(changes.len());
    for change in changes {
        match (merged.last_mut(), change) {
            (Some(Draft::More { text: held }), Draft::More { text }) => held.push_str(&text),
            (_, change) => merged.push(change),
        }
    }
    merged
}

impl Body for Teed {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, hyper::Error>>> {
        let polled = Pin::new(&mut self.inner).poll_frame(cx);
        match &polled {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    let data = data.clone();
                    self.read(&data);
                }
            }
            // The reply ended, whole or not: nothing is being written now.
            Poll::Ready(None) | Poll::Ready(Some(Err(_))) => {
                let finished = self.reading.finish();
                self.pending.extend(finished);
                self.send();
            }
            Poll::Pending => {}
        }
        polled
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pane_path_names_the_pane_the_token_and_what_it_asked_upstream() {
        assert_eq!(
            route("/claude/7/tok/v1/messages?beta=true"),
            Some((PaneId(7), "tok", "/v1/messages?beta=true"))
        );
        assert_eq!(route("/claude/7/tok"), Some((PaneId(7), "tok", "")));
        assert_eq!(route("/claude/7/tok?x=1"), Some((PaneId(7), "tok", "?x=1")));
        assert_eq!(route("/claude/seven/tok/v1"), None);
        assert_eq!(route("/v1/messages"), None);
    }

    #[test]
    fn a_burst_of_pieces_is_sent_as_one() {
        let merged = merge(vec![
            Draft::Start { thinking: false },
            Draft::More { text: "a".into() },
            Draft::More { text: "b".into() },
            Draft::Done,
            Draft::More { text: "c".into() },
        ]);
        assert_eq!(
            merged,
            [
                Draft::Start { thinking: false },
                Draft::More { text: "ab".into() },
                Draft::Done,
                Draft::More { text: "c".into() },
            ]
        );
    }
}
