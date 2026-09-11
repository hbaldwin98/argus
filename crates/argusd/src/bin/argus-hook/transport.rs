//! How a request reaches the daemon: the loopback HTTP post, which URL a
//! pane's reports go to, and the credentials the environment carries.

use super::*;

pub(super) fn env_url() -> String {
    std::env::var(URL_VAR).unwrap_or_default()
}

pub(super) fn env_token() -> String {
    std::env::var(TOKEN_VAR).unwrap_or_default()
}

/// Repoint a checkout-wide managed hook at the pane-specific URL inherited
/// by this process. Both URLs must name panes on the same loopback listener.
pub(super) fn rebase_hook_url(configured: &str, inherited: &str) -> Option<String> {
    let configured_base = pane_base(configured)?;
    let inherited_base = pane_base(inherited)?;
    if authority(&configured_base)? != authority(&inherited_base)? {
        return None;
    }
    let suffix = configured.strip_prefix(&configured_base)?;
    (!suffix.is_empty()).then(|| format!("{inherited_base}{suffix}"))
}

pub(super) fn routed_hook(
    configured_url: &str,
    configured_token: &str,
    inherited_url: &str,
    inherited_token: &str,
) -> (String, String) {
    match (
        rebase_hook_url(configured_url, inherited_url),
        !inherited_token.is_empty(),
    ) {
        (Some(url), true) => (url, inherited_token.to_string()),
        _ => (configured_url.to_string(), configured_token.to_string()),
    }
}

pub(super) fn authority(url: &str) -> Option<&str> {
    url.strip_prefix("http://")?.split('/').next()
}

/// A pane base (`http://host:port/pane/<id>`) plus the endpoint being asked
/// for. The suffix comes from `argus-protocol` so the daemon parses exactly
/// what is built here.
pub(super) fn endpoint_url(base: &str, endpoint: Endpoint) -> String {
    let url = format!("{}/{}", base.trim_end_matches('/'), endpoint.suffix());
    if std::env::var(ARTIFACT_SCOPE_VAR).as_deref() == Ok("workspace") {
        format!("{url}?scope=workspace")
    } else {
        url
    }
}

pub(super) fn pane_base(url: &str) -> Option<String> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = rest.split_once('/')?;
    let mut parts = path.split('/');
    if parts.next()? != "pane" {
        return None;
    }
    parts.next()?.parse::<u64>().ok()?;
    let host = authority.split(':').next()?;
    if host != "127.0.0.1" || authority.rsplit_once(':')?.1.parse::<u16>().is_err() {
        return None;
    }
    Some(format!(
        "http://{authority}/pane/{}",
        path.split('/').nth(1)?
    ))
}

/// Best-effort POST. Every error is discarded by the caller; the return type
/// exists only so the body can use `?`.
pub(super) fn post(url: &str, token: &str, body: &str) -> Option<()> {
    post_as(url, token, body, None)
}

pub(super) fn post_as(url: &str, token: &str, body: &str, session: Option<&str>) -> Option<()> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    let addr = authority.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;

    let req = request(path, authority, token, session, body);
    stream.write_all(req.as_bytes()).ok()?;
    // The daemon's reply is deliberately not read: nothing here acts on it,
    // and not waiting keeps the agent's turn from stalling on a slow answer.
    Some(())
}

pub(super) fn post_response(url: &str, token: &str, body: &str) -> Option<(u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = authority.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;
    stream.set_read_timeout(Some(TIMEOUT)).ok()?;
    stream
        .write_all(request(path, authority, token, None, body).as_bytes())
        .ok()?;

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let (head, body) = response.split_once("\r\n\r\n")?;
    let status = head.split_whitespace().nth(1)?.parse().ok()?;
    Some((status, body.to_string()))
}

/// Headers are assembled by hand rather than with a client library, so
/// each one must start its own line at column zero: a header the daemon
/// cannot recognize is not an error it can report, only a report that
/// quietly does nothing — a session header it misses files a child's work
/// under its parent's row, and a Content-Length it misses drops the note.
pub(super) fn request(path: &str, authority: &str, token: &str, session: Option<&str>, body: &str) -> String {
    let session = match session.filter(|id| !id.is_empty()) {
        Some(id) => format!("{SESSION_HEADER}: {id}\r\n"),
        None => String::new(),
    };
    let mut req = String::new();
    req.push_str(&format!("POST {path} HTTP/1.1\r\n"));
    req.push_str(&format!("Host: {authority}\r\n"));
    req.push_str(&format!("Authorization: Bearer {token}\r\n"));
    req.push_str(&session);
    req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    req.push_str("Connection: close\r\n\r\n");
    req.push_str(body);
    req
}
