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
    let configured = parse_pane_url(configured)?;
    let inherited = parse_pane_url(inherited)?;
    if configured.port != inherited.port || configured.rest.is_empty() {
        return None;
    }
    Some(format!("{}{}", inherited.base(), configured.rest))
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

/// A pane base plus the endpoint being asked for, on the board this
/// process's environment asks for.
pub(super) fn endpoint_url(base: &str, endpoint: Endpoint) -> String {
    let scope = requested_scope(std::env::var(ARTIFACT_SCOPE_VAR).ok().as_deref());
    let url = argus_protocol::endpoint_url(base, endpoint, scope);
    match NAMED_FEATURE.get() {
        Some(slug) => argus_protocol::feature_url(&url, slug),
        None => url,
    }
}

/// The feature this run's board command names, set once by `main`. One per
/// process, as the scope is: the helper runs one command and exits.
static NAMED_FEATURE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub(super) fn name_feature(slug: &str) {
    let _ = NAMED_FEATURE.set(slug.to_string());
}

/// What a feature command's own words are, so `feature <slug>` can be told
/// from one of them.
const FEATURE_SUBCOMMANDS: &[&str] = &[
    "list", "open", "use", "note", "export", "done", "reopen", "retitle", "brief", "drop", "hold",
    "unhold", "wait", "unwait",
];

/// The feature a board command names, and its arguments without the name.
///
/// `--feature <slug>`, anywhere in any board command, or `feature <slug>`
/// on its own. A name that is not a slug is refused here, before it
/// travels.
pub(super) fn named_feature<'a>(
    command: &str,
    rest: &[&'a str],
) -> Result<(Option<&'a str>, Vec<&'a str>), String> {
    if let [only] = rest {
        if command == "feature" && !FEATURE_SUBCOMMANDS.contains(only) && is_slug(only) {
            return Ok((Some(only), Vec::new()));
        }
    }
    let Some(at) = rest.iter().position(|arg| *arg == "--feature") else {
        return Ok((None, rest.to_vec()));
    };
    let Some(slug) = rest.get(at + 1) else {
        return Err("--feature wants a feature's slug; `feature list` names them".to_string());
    };
    if !is_slug(slug) {
        return Err(format!(
            "`{slug}` is not a feature slug; `feature list` names them"
        ));
    }
    let mut left = rest.to_vec();
    left.drain(at..=at + 1);
    Ok((Some(slug), left))
}

/// The pane base a URL names, or `None` for one not on the loopback
/// listener.
pub(super) fn pane_base(url: &str) -> Option<String> {
    parse_pane_url(url).map(|url| url.base())
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
pub(super) fn request(
    path: &str,
    authority: &str,
    token: &str,
    session: Option<&str>,
    body: &str,
) -> String {
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
