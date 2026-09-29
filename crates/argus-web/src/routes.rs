//! What each request to the server gets: the page's files, pairing, and the
//! WebSocket a paired phone talks over, all behind the headers every
//! response carries.
//!
//! Two checks guard everything that can reach an agent. The request must
//! come from the page this server served — its `Origin` names the host it
//! was sent to — which stops another site in the same browser from opening
//! the socket. And it must carry a paired device's cookie, which such a
//! site never has even when it gets its own name to resolve to this
//! machine.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc};

use crate::daemon::{Ask, Feeds};
use crate::pairing::{Code, Devices};
use crate::phone::{FromPhone, ToPhone};

/// The cookie a paired device carries.
pub const COOKIE: &str = "argus_device";

/// How often a connected phone's device is looked up again, so revoking
/// one ends its session rather than only its next.
const RECHECK: Duration = Duration::from_secs(10);

/// No script but the page's own file, no style but its own sheet, nothing
/// loaded from anywhere else, and no framing. Replies are rendered to HTML
/// that runs nothing before they arrive; this is what stands behind that.
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
connect-src 'self'; manifest-src 'self'; worker-src 'self'; base-uri 'none'; form-action 'self'; \
frame-ancestors 'none'";

/// Everything the routes share.
pub struct App {
    pub feeds: Feeds,
    pub asks: mpsc::UnboundedSender<Ask>,
    pub devices: Devices,
    pub code: Mutex<Code>,
    /// The key pushes are signed with; `None` when it could not be made,
    /// which leaves everything but pushes working.
    pub vapid: Option<crate::push::Vapid>,
    /// How many of each device's pages are on screen now.
    pub visible: Mutex<std::collections::HashMap<String, usize>>,
}

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/", get(|| async { asset("text/html; charset=utf-8", INDEX) }))
        .route("/app.js", get(|| async { asset("text/javascript; charset=utf-8", APP_JS) }))
        .route("/style.css", get(|| async { asset("text/css; charset=utf-8", STYLE) }))
        .route("/sw.js", get(|| async { asset("text/javascript; charset=utf-8", WORKER) }))
        .route("/manifest.webmanifest", get(|| async { asset("application/manifest+json", MANIFEST) }))
        .route("/icon-192.png", get(|| async { asset("image/png", ICON_192) }))
        .route("/icon-512.png", get(|| async { asset("image/png", ICON_512) }))
        .route("/api/me", get(me))
        .route("/api/pair", post(pair))
        .route("/api/push/key", get(push_key))
        .route("/api/push/subscribe", post(push_subscribe))
        .route("/api/push/unsubscribe", post(push_unsubscribe))
        .route("/ws", get(ws))
        .layer(middleware::from_fn(security_headers))
        .with_state(app)
}

const INDEX: &[u8] = include_bytes!("../assets/index.html");
const APP_JS: &[u8] = include_bytes!("../assets/app.js");
const STYLE: &[u8] = include_bytes!("../assets/style.css");
const WORKER: &[u8] = include_bytes!("../assets/sw.js");
const MANIFEST: &[u8] = include_bytes!("../assets/manifest.webmanifest");
const ICON_192: &[u8] = include_bytes!("../assets/icon-192.png");
const ICON_512: &[u8] = include_bytes!("../assets/icon-512.png");

fn asset(content_type: &'static str, body: &'static [u8]) -> Response {
    ([(header::CONTENT_TYPE, content_type)], body).into_response()
}

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Whether the request came from a page served by this host: its `Origin`
/// names the same host and port the request was sent to. A request with
/// no `Origin` did not come from a browser page at all, and is refused.
pub fn same_origin(headers: &HeaderMap) -> bool {
    let (Some(origin), Some(host)) = (
        headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()),
        headers.get(header::HOST).and_then(|v| v.to_str().ok()),
    ) else {
        return false;
    };
    let Some((_, rest)) = origin.split_once("://") else {
        return false;
    };
    let origin_host = rest.split('/').next().unwrap_or(rest);
    origin_host.eq_ignore_ascii_case(host)
}

/// The device token the request's cookie carries.
pub fn device_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == COOKIE && !value.is_empty()).then(|| value.to_string())
        })
}

async fn me(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    match device_token(&headers).and_then(|token| app.devices.check(&token)) {
        Some(device) => Json(serde_json::json!({
            "device": device.name,
            "push": device.push.is_some(),
            "can_push": app.vapid.is_some(),
        }))
        .into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

/// The paired device a request comes from, when it comes from this
/// server's own page.
fn paired(app: &App, headers: &HeaderMap) -> Option<crate::pairing::Device> {
    if !same_origin(headers) {
        return None;
    }
    device_token(headers).and_then(|token| app.devices.check(&token))
}

async fn push_key(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    let (Some(_), Some(vapid)) = (paired(&app, &headers), app.vapid.as_ref()) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    Json(serde_json::json!({ "key": vapid.public_key() })).into_response()
}

async fn push_subscribe(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(subscription): Json<crate::push::Subscription>,
) -> Response {
    let Some(device) = paired(&app, &headers) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    // A push service is always reached over HTTPS; anything else is not
    // one, and would be this server making a request wherever it is told.
    if !subscription.endpoint.starts_with("https://") {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match app.devices.set_push(&device.name, Some(subscription)) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        _ => StatusCode::FORBIDDEN.into_response(),
    }
}

async fn push_unsubscribe(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    let Some(device) = paired(&app, &headers) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let _ = app.devices.set_push(&device.name, None);
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct Pairing {
    code: String,
    name: String,
}

async fn pair(State(app): State<Arc<App>>, headers: HeaderMap, Json(ask): Json<Pairing>) -> Response {
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let redeemed = app
        .code
        .lock()
        .unwrap()
        .redeem(&ask.code, &ask.name, &app.devices);
    let token = match redeemed {
        Ok(token) => token,
        Err(refusal) => {
            let body = Json(serde_json::json!({ "error": refusal.to_string() }));
            return (StatusCode::FORBIDDEN, body).into_response();
        }
    };
    // Secure only where the page arrived over HTTPS: a phone reaching this
    // over plain HTTP could not send the cookie back otherwise.
    let https = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|origin| origin.starts_with("https://"));
    let cookie = format!(
        "{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=31536000{}",
        if https { "; Secure" } else { "" }
    );
    let mut response = Json(serde_json::json!({ "device": ask.name.trim() })).into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

async fn ws(State(app): State<Arc<App>>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(token) = device_token(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(device) = app.devices.check(&token) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    upgrade.on_upgrade(move |socket| phone(socket, app, token, device.name))
}

/// One paired phone's session: whole-state messages as they change, the
/// conversations it watches, and what it asks, until it goes or its device
/// is revoked.
async fn phone(mut socket: WebSocket, app: Arc<App>, token: String, device: String) {
    let mut visible = false;
    let mut server = app.feeds.server.clone();
    let mut agents = app.feeds.agents.clone();
    let mut conversations = app.feeds.conversations.subscribe();
    let mut screens = app.feeds.screens.subscribe();
    let mut watching: HashSet<u64> = HashSet::new();
    let mut screening: HashSet<u64> = HashSet::new();
    // Answers meant for this phone alone: what became of its messages.
    let (direct, mut answers) = mpsc::unbounded_channel::<Arc<String>>();
    let mut recheck = tokio::time::interval(RECHECK);
    recheck.tick().await;

    let first = [server.borrow_and_update().clone(), agents.borrow_and_update().clone()];
    for json in first {
        if socket.send(Message::Text(json.as_str().into())).await.is_err() {
            return;
        }
    }

    loop {
        let out = tokio::select! {
            incoming = socket.recv() => {
                let Some(Ok(message)) = incoming else { break };
                match message {
                    Message::Text(text) => {
                        match serde_json::from_str::<FromPhone>(text.as_str()) {
                            Ok(FromPhone::Visible { visible: now }) => {
                                set_visible(&app, &device, &mut visible, now);
                                None
                            }
                            Ok(ask) => handle(&app, &mut watching, &mut screening, ask, &direct),
                            Err(_) => Some(error("that message is not one this server knows")),
                        }
                    }
                    Message::Close(_) => break,
                    _ => None,
                }
            }
            Some(answer) = answers.recv() => Some(answer),
            changed = server.changed() => {
                if changed.is_err() { break }
                Some(server.borrow_and_update().clone())
            }
            changed = agents.changed() => {
                if changed.is_err() { break }
                Some(agents.borrow_and_update().clone())
            }
            update = conversations.recv() => match update {
                Ok((pane, json)) => watching.contains(&pane).then_some(json),
                // Fell behind the updates: each watched conversation is sent
                // fresh rather than with a gap in it.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    for pane in &watching {
                        let _ = app.asks.send(Ask::Refresh(*pane));
                    }
                    None
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            update = screens.recv() => match update {
                Ok((pane, json)) => screening.contains(&pane).then_some(json),
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    for pane in &screening {
                        let _ = app.asks.send(Ask::RefreshScreen(*pane));
                    }
                    None
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = recheck.tick() => {
                if app.devices.check(&token).is_none() {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
                None
            }
        };
        if let Some(json) = out {
            if socket.send(Message::Text(json.as_str().into())).await.is_err() {
                break;
            }
        }
    }
    set_visible(&app, &device, &mut visible, false);
    for pane in watching {
        let _ = app.asks.send(Ask::Unwatch(pane));
    }
    for pane in screening {
        let _ = app.asks.send(Ask::Unscreen(pane));
    }
}

/// Counts this page in or out of its device's pages on screen.
fn set_visible(app: &App, device: &str, visible: &mut bool, now: bool) {
    if *visible == now {
        return;
    }
    *visible = now;
    let mut counts = app.visible.lock().unwrap();
    let count = counts.entry(device.to_string()).or_insert(0);
    if now {
        *count += 1;
    } else {
        *count = count.saturating_sub(1);
    }
}

/// What a phone's ask does; anything it answers straight away comes back,
/// and anything the daemon answers later goes to `direct`.
fn handle(
    app: &App,
    watching: &mut HashSet<u64>,
    screening: &mut HashSet<u64>,
    ask: FromPhone,
    direct: &mpsc::UnboundedSender<Arc<String>>,
) -> Option<Arc<String>> {
    match ask {
        FromPhone::Watch { pane } => {
            // Watching again still asks, for the fresh tail the page wants.
            if !watching.insert(pane) {
                let _ = app.asks.send(Ask::Refresh(pane));
                return None;
            }
            let _ = app.asks.send(Ask::Watch(pane));
        }
        FromPhone::Unwatch { pane } => {
            if watching.remove(&pane) {
                let _ = app.asks.send(Ask::Unwatch(pane));
            }
        }
        FromPhone::Earlier { pane, before } => {
            let _ = app.asks.send(Ask::Earlier(pane, before));
        }
        FromPhone::Send { pane, text, now } => {
            let _ = app.asks.send(Ask::Send {
                pane,
                text,
                now,
                reply: direct.clone(),
            });
        }
        FromPhone::Cancel { pane, id } => {
            let _ = app.asks.send(Ask::Cancel(pane, id));
        }
        FromPhone::Stop { pane } => {
            let _ = app.asks.send(Ask::Stop(pane));
        }
        FromPhone::Screen { pane } => {
            let ask = if screening.insert(pane) { Ask::Screen(pane) } else { Ask::RefreshScreen(pane) };
            let _ = app.asks.send(ask);
        }
        FromPhone::Unscreen { pane } => {
            if screening.remove(&pane) {
                let _ = app.asks.send(Ask::Unscreen(pane));
            }
        }
        FromPhone::Visible { .. } => {}
        FromPhone::Key { pane, key } => match crate::screen::key_bytes(&key) {
            Some(bytes) => {
                let _ = app.asks.send(Ask::Key(pane, bytes));
            }
            None => return Some(error("the key bar has no such key")),
        },
    }
    None
}

fn error(message: &str) -> Arc<String> {
    Arc::new(
        serde_json::to_string(&ToPhone::Error {
            message: message.to_string(),
        })
        .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    #[test]
    fn a_request_from_this_servers_page_is_same_origin() {
        assert!(same_origin(&headers(&[
            ("origin", "http://127.0.0.1:7420"),
            ("host", "127.0.0.1:7420"),
        ])));
        assert!(same_origin(&headers(&[
            ("origin", "https://box.tail1234.ts.net"),
            ("host", "box.tail1234.ts.net"),
        ])));
    }

    #[test]
    fn another_site_or_no_origin_is_not() {
        assert!(!same_origin(&headers(&[
            ("origin", "https://evil.example"),
            ("host", "127.0.0.1:7420"),
        ])));
        assert!(!same_origin(&headers(&[("host", "127.0.0.1:7420")])));
        assert!(!same_origin(&headers(&[
            ("origin", "http://127.0.0.1:9999"),
            ("host", "127.0.0.1:7420"),
        ])));
    }

    #[test]
    fn the_device_cookie_is_found_among_others() {
        let h = headers(&[("cookie", "theme=dark; argus_device=abc123; other=1")]);
        assert_eq!(device_token(&h).as_deref(), Some("abc123"));
        assert_eq!(device_token(&headers(&[("cookie", "argus_device=")])), None);
        assert_eq!(device_token(&HeaderMap::new()), None);
    }
}
