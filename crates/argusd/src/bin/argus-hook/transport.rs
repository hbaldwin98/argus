//! How a request reaches the daemon: the loopback HTTP post, the typed call
//! each endpoint answers, which URL a pane's reports go to, and the
//! credentials the environment carries.

use argus_protocol::hook::call::{Call, Form};

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
    "unhold", "wait", "unwait", "link", "unlink",
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

/// Whether this event's conversation left work running behind its turn.
/// Set once, by the installed hook form, and carried on the reports it
/// makes: the status report is the one the daemon reads it from.
static BACKGROUND: std::sync::OnceLock<()> = std::sync::OnceLock::new();

pub(super) fn name_background_work() {
    let _ = BACKGROUND.set(());
}

/// The directory this event's conversation is working in, set the same
/// way and carried on the same reports. The daemon follows a turn that
/// ends in another worktree, so the agent never has to say it moved.
static DIRECTORY: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub(super) fn name_directory(directory: &str) {
    // A line break would end the header and start one the daemon would
    // read as the agent's; no real directory has one.
    if !directory.is_empty() && !directory.contains(['\r', '\n']) {
        let _ = DIRECTORY.set(directory.to_string());
    }
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

    let background = BACKGROUND.get().is_some();
    let directory = DIRECTORY.get().map(String::as_str);
    let req = request(path, authority, token, session, background, directory, body);
    stream.write_all(req.as_bytes()).ok()?;
    // The daemon's reply is deliberately not read: nothing here acts on it,
    // and not waiting keeps the agent's turn from stalling on a slow answer.
    Some(())
}

/// Why a call to the pane API brought back no reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Refusal {
    /// The body could not be written; nothing was sent.
    Unencodable,
    /// Larger than the endpoint takes; refused here, before it travelled.
    TooLarge { size: usize, most: usize },
    /// No daemon answered in time.
    Unavailable,
    /// The daemon refused, in its words; empty when it gave none.
    Refused(String),
    /// Answered, but not with what the endpoint replies: a write it carried
    /// landed, only the account of it did not.
    Unreadable,
}

impl Refusal {
    /// The refusal after what was being done, for the agent that ran the
    /// command rather than for a log.
    pub(super) fn said(&self, doing: &str) -> String {
        match self {
            Refusal::Unencodable => format!("{doing}: unencodable"),
            Refusal::TooLarge { size, most } => {
                format!("{doing}: the request is {size} bytes, and this takes at most {most}")
            }
            Refusal::Unavailable => format!("{doing}: daemon unavailable"),
            Refusal::Refused(words) if words.is_empty() => format!("{doing}: daemon refused the request"),
            Refusal::Refused(words) => format!("{doing}: {words}"),
            Refusal::Unreadable => format!("{doing}: invalid daemon response"),
        }
    }
}

/// One call to the pane API on this run's board, answered.
pub(super) fn call<C: Call>(base_url: &str, token: &str, body: &C::Body) -> Result<C::Reply, Refusal> {
    call_within::<C>(base_url, token, body, TIMEOUT)
}

/// [`call`], waiting up to `wait` for the answer: for work slower than a
/// report's, such as making a worktree.
pub(super) fn call_within<C: Call>(
    base_url: &str,
    token: &str,
    body: &C::Body,
    wait: Duration,
) -> Result<C::Reply, Refusal> {
    let body = encoded::<C>(body)?;
    let url = endpoint_url(base_url, C::ENDPOINT);
    let (status, answer) = post_response_within(&url, token, &body, wait).ok_or(Refusal::Unavailable)?;
    if status != 200 {
        return Err(Refusal::Refused(answer.trim().to_string()));
    }
    C::ReplyForm::read(answer.as_bytes()).map_err(|_| Refusal::Unreadable)
}

/// Best effort, the reply unread, as a hook's report is: nothing here may
/// stall a turn.
pub(super) fn send<C: Call>(base_url: &str, token: &str, body: &C::Body, session: Option<&str>) {
    if let Ok(body) = encoded::<C>(body) {
        let _ = post_as(&endpoint_url(base_url, C::ENDPOINT), token, &body, session);
    }
}

/// The body as `C` writes it, refused here when the daemon would refuse it
/// for its size: past the bound it would only be drained and dropped.
fn encoded<C: Call>(body: &C::Body) -> Result<String, Refusal> {
    let bytes = C::BodyForm::write(body).map_err(|_| Refusal::Unencodable)?;
    let most = C::ENDPOINT.max_body();
    if bytes.len() > most {
        return Err(Refusal::TooLarge { size: bytes.len(), most });
    }
    // `request` writes text; every form writes UTF-8, so this never refuses.
    String::from_utf8(bytes).map_err(|_| Refusal::Unencodable)
}

fn post_response_within(
    url: &str,
    token: &str,
    body: &str,
    wait: Duration,
) -> Option<(u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = authority.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;
    stream.set_read_timeout(Some(wait)).ok()?;
    stream
        .write_all(request(path, authority, token, None, false, None, body).as_bytes())
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
    background: bool,
    directory: Option<&str>,
    body: &str,
) -> String {
    let session = match session.filter(|id| !id.is_empty()) {
        Some(id) => format!("{SESSION_HEADER}: {id}\r\n"),
        None => String::new(),
    };
    let background = match background {
        true => format!("{BACKGROUND_HEADER}: agents\r\n"),
        false => String::new(),
    };
    let directory = match directory {
        Some(directory) => format!("{DIRECTORY_HEADER}: {directory}\r\n"),
        None => String::new(),
    };
    let mut req = String::new();
    req.push_str(&format!("POST {path} HTTP/1.1\r\n"));
    req.push_str(&format!("Host: {authority}\r\n"));
    req.push_str(&format!("Authorization: Bearer {token}\r\n"));
    req.push_str(&session);
    req.push_str(&background);
    req.push_str(&directory);
    req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    req.push_str("Connection: close\r\n\r\n");
    req.push_str(body);
    req
}

#[cfg(test)]
/// A daemon that answers one request with `status` and `body`, and the
/// body it was sent, once it has. Shared by the commands' tests.
pub(super) fn answering(status: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/pane/1", listener.local_addr().unwrap());
    let reply = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut seen = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream.read(&mut chunk).unwrap();
            seen.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&seen);
            let Some((head, rest)) = text.split_once("\r\n\r\n") else { continue };
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length: "))
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0);
            if rest.len() >= length || n == 0 {
                break;
            }
        }
        stream.write_all(reply.as_bytes()).unwrap();
        String::from_utf8_lossy(&seen).to_string()
    });
    (base, server)
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::{AgentView, ViewChange, MAX_VIEW_BYTES};

    const NOBODY: &str = "http://127.0.0.1:1/pane/1";

    #[test]
    fn a_call_is_posted_to_its_endpoint_and_answered_with_its_reply() {
        let list = r#"{"project_name":"argus","feature":"pushes","tasks":[]}"#;
        let (base, server) = answering("200 OK", list);
        let answered = call::<hook::call::Tasks>(&base, "t", &TaskAction::List).unwrap();
        assert_eq!(answered.feature.as_deref(), Some("pushes"));
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /pane/1/tasks"), "{request}");
        assert!(request.ends_with(r#""List""#), "{request}");
    }

    #[test]
    fn a_refusal_is_the_daemons_words_after_what_was_being_done() {
        let (base, server) = answering("409 Conflict", "no such task\n");
        let refusal = call::<hook::call::Tasks>(&base, "t", &TaskAction::List).unwrap_err();
        server.join().unwrap();
        assert_eq!(refusal, Refusal::Refused("no such task".into()));
        assert_eq!(refusal.said("could not change task"), "could not change task: no such task");
    }

    #[test]
    fn a_refusal_without_words_says_the_daemon_refused() {
        let (base, server) = answering("409 Conflict", "");
        let refusal = call::<hook::call::Tasks>(&base, "t", &TaskAction::List).unwrap_err();
        server.join().unwrap();
        assert_eq!(refusal.said("could not change task"), "could not change task: daemon refused the request");
    }

    #[test]
    fn a_daemon_that_is_not_there_is_unavailable() {
        let refusal = call::<hook::call::Tasks>(NOBODY, "t", &TaskAction::List).unwrap_err();
        assert_eq!(refusal, Refusal::Unavailable);
        assert_eq!(refusal.said("could not change task"), "could not change task: daemon unavailable");
    }

    #[test]
    fn a_body_past_its_endpoints_bound_is_refused_before_it_is_sent() {
        // Nothing listens on NOBODY: a body that travelled would come back
        // unavailable, not too large.
        let view = AgentView { id: "big".into(), title: "x".repeat(MAX_VIEW_BYTES), ..Default::default() };
        let refusal = call::<hook::call::ChangeView>(NOBODY, "t", &ViewChange::Show { view }).unwrap_err();
        let Refusal::TooLarge { size, most } = refusal else {
            panic!("expected the size refused: {refusal:?}");
        };
        assert_eq!(most, MAX_VIEW_BYTES);
        assert!(size > most);
    }

    #[test]
    fn an_answer_that_is_not_the_reply_is_unreadable() {
        let (base, server) = answering("200 OK", "not json");
        let refusal = call::<hook::call::Tasks>(&base, "t", &TaskAction::List).unwrap_err();
        server.join().unwrap();
        assert_eq!(refusal, Refusal::Unreadable);
    }
}
