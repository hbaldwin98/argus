//! The command Argus's managed agent hooks run, and the one an agent runs
//! itself to say what it is doing.
//!
//! ```text
//! argus-hook title "fixing the pty deadlock"
//! argus-hook status waiting "needs the staging database password"
//! argus-hook status needs-review "ready for review"
//! argus-hook status done "reviewed and complete"
//! argus-hook status working
//! argus-hook checkout                            # reports the current directory
//! argus-hook session <id>                        # records exact resume identity
//! argus-hook delegate [--template NAME] "task"  # opens another agent pane
//! argus-hook handoff [--template NAME]           # reads a handoff from stdin
//! argus-hook comments                            # reads durable review feedback
//! argus-hook say "text"                          # prints, calls nobody
//! argus-hook <url> <token> [--note-from-stdin]  # the installed hook form
//! ```
//!
//! The named forms read `ARGUS_HOOK_URL` and `ARGUS_HOOK_TOKEN` from the
//! environment, which every agent pane is handed. That is what makes status
//! harness-agnostic: a CLI that can run one command at some point in its
//! lifecycle needs nothing from Argus but these variables. The explicit form
//! is what Argus writes into a harness's own hook config, where there is no
//! guarantee the environment survives.
//!
//! It **always exits 0**, whatever happens. That is the entire reason it
//! exists instead of a `curl` invocation: a hook command that exits non-zero
//! is reported to the user as a failed turn. A daemon that has since exited —
//! or a port that now belongs to nobody — must degrade to "pane status stops
//! updating", never to an error on every prompt in that directory. `curl`
//! exits 7 on a refused connection, which is exactly what this avoids.
//!
//! Installed hooks write only the JSON the runner needs to let the turn
//! continue — Cursor wants `permission`, Claude wants `decision` — never a
//! human-readable message. Some agent CLIs inject a hook's stdout into the
//! model's context, so staying silent keeps Argus's bookkeeping out of the
//! conversation. The deliberate `say` and `delegate` commands do return
//! useful output.
//!
//! On Windows it is a GUI-subsystem binary. Not because it has a UI — it
//! has none — but because the agent CLI that runs it decides how it is
//! spawned, and we cannot ask that CLI to pass `CREATE_NO_WINDOW`. A
//! console-subsystem binary spawned from a process without a console gets
//! its own console *window*, which flashes on screen on every hook event.
//! Declaring the GUI subsystem means no console is ever allocated. Safe
//! precisely because this program reads and writes nothing on stdio it was
//! not handed.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use argus_protocol::{
    DelegateRequest, DelegateResponse, Endpoint, HandoffRequest, Report, ReviewComment,
    MAX_DELEGATE_TASK_BYTES, MAX_HANDOFF_BYTES,
};

const TIMEOUT: Duration = Duration::from_secs(2);
const NOTE_FLAG: &str = "--note-from-stdin";
const SESSION_KEY_FLAG: &str = "--session-id-from-stdin";
const OWNS_SESSION_FLAG: &str = "--owns-session";
/// Names the conversation a report comes from, so the daemon can tell the
/// agent that owns a pane from one spawned inside it — which inherits this
/// process's environment and would otherwise rewrite its parent's row.
const SESSION_HEADER: &str = "X-Argus-Session";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    dispatch(args.first().map(String::as_str), &rest);
}

type NamedHandler = fn(&[&str]);

const NAMED_HANDLERS: &[(&str, NamedHandler)] = &[
    ("say", say),
    ("title", title),
    ("status", status),
    ("checkout", checkout),
    ("session", session),
    ("delegate", delegate),
    ("handoff", handoff),
    ("comments", comments),
];

fn dispatch(command: Option<&str>, rest: &[&str]) {
    match command {
        // The installed-hook form uses an absolute URL and token because a
        // harness's hook config cannot count on inheriting the environment.
        Some(url) if url.starts_with("http://") => installed_hook(url, rest),
        Some(name) => {
            if let Some((_, handler)) = NAMED_HANDLERS.iter().find(|(key, _)| *key == name) {
                handler(rest);
            }
        }
        None => {}
    }
}

fn say(rest: &[&str]) {
    // Deliberately on stdout: this is context for the model.
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", rest.join(" "));
    let _ = out.flush();
}

fn title(rest: &[&str]) {
    let text = rest.join(" ");
    if !text.trim().is_empty() {
        let _ = post(
            &endpoint_url(&env_url(), Endpoint::Title),
            &env_token(),
            &text,
        );
    }
}

fn status(rest: &[&str]) {
    // A state the pane API has no name for could only ever be refused at the
    // other end, so it is refused here instead of travelling.
    let Some(report) = rest.first().and_then(|s| Report::parse(s)) else {
        return;
    };
    // Anything after the state is the reason, so
    // `status waiting "needs a password"` reads the way you'd say it.
    let note = rest[1..].join(" ");
    let _ = post(
        &endpoint_url(&env_url(), Endpoint::Status(report)),
        &env_token(),
        &note,
    );
}

fn checkout(rest: &[&str]) {
    if let Some(path) = reported_checkout(rest) {
        let _ = post(
            &endpoint_url(&env_url(), Endpoint::Checkout),
            &env_token(),
            &path.to_string_lossy(),
        );
    }
}

fn session(rest: &[&str]) {
    let id = rest.join(" ");
    if !id.is_empty() {
        let _ = post(
            &endpoint_url(&env_url(), Endpoint::Session),
            &env_token(),
            &id,
        );
    }
}

fn delegate(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(
        out,
        "{}",
        delegation_message(rest, &env_url(), &env_token())
    );
    let _ = out.flush();
}

fn delegation_message(rest: &[&str], base_url: &str, token: &str) -> String {
    match request_delegation(rest, base_url, token) {
        Ok(response) => opened(response),
        Err(error) => format!("could not open agent: {error}"),
    }
}

/// What the agent that asked is told. The pane is open, but the harness
/// inside it is still starting and is given its message once it can read
/// one — so this says the sending is under way, rather than leaving the
/// caller to conclude from a silent pane that it should ask again.
fn opened(response: DelegateResponse) -> String {
    format!(
        "opened agent pane {}; it is sent its message once it finishes starting",
        response.pane.0
    )
}

fn request_delegation(
    rest: &[&str],
    base_url: &str,
    token: &str,
) -> Result<DelegateResponse, String> {
    let request = delegate_args(rest).map_err(str::to_string)?;
    request_agent(&request, Endpoint::Delegate, base_url, token)
}

fn request_agent(
    request: &impl serde::Serialize,
    endpoint: Endpoint,
    base_url: &str,
    token: &str,
) -> Result<DelegateResponse, String> {
    let body = serde_json::to_string(&request).map_err(|_| "invalid request".to_string())?;
    let (status, response_body) = post_response(&endpoint_url(base_url, endpoint), token, &body)
        .ok_or_else(|| "daemon unavailable".to_string())?;
    if status != 201 {
        let reason = response_body.trim();
        return Err(if reason.is_empty() {
            "daemon refused the request".to_string()
        } else {
            reason.to_string()
        });
    }
    serde_json::from_str(&response_body).map_err(|_| "invalid daemon response".to_string())
}

fn delegate_args(rest: &[&str]) -> Result<DelegateRequest, &'static str> {
    let (template, task_args) = if rest.first() == Some(&"--template") {
        let template = rest
            .get(1)
            .filter(|name| !name.trim().is_empty())
            .ok_or("--template requires a name and task")?;
        (Some((*template).to_string()), &rest[2..])
    } else {
        (None, rest)
    };
    let task = task_args.join(" ");
    if task.trim().is_empty() {
        return Err("delegate requires a task");
    }
    if task.len() > MAX_DELEGATE_TASK_BYTES {
        return Err("delegate task exceeds 2048 bytes");
    }
    Ok(DelegateRequest { template, task })
}

fn handoff(rest: &[&str]) {
    let message = match read_handoff(std::io::stdin().lock()) {
        Ok(input) => handoff_message(rest, &input, &env_url(), &env_token()),
        Err(error) => format!("could not open agent: {error}"),
    };
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

fn read_handoff(reader: impl Read) -> Result<String, &'static str> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_HANDOFF_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "could not read handoff from stdin")?;
    if bytes.len() > MAX_HANDOFF_BYTES {
        return Err("handoff exceeds 32768 bytes");
    }
    String::from_utf8(bytes).map_err(|_| "handoff on stdin is not UTF-8")
}

fn handoff_message(rest: &[&str], input: &str, base_url: &str, token: &str) -> String {
    match handoff_args(rest, input)
        .map_err(str::to_string)
        .and_then(|request| request_agent(&request, Endpoint::Handoff, base_url, token))
    {
        Ok(response) => opened(response),
        Err(error) => format!("could not open agent: {error}"),
    }
}

fn handoff_args(rest: &[&str], input: &str) -> Result<HandoffRequest, &'static str> {
    let template = match rest {
        [] => None,
        ["--template", name] if !name.trim().is_empty() => Some((*name).to_string()),
        ["--template"] | ["--template", _] => return Err("--template requires a name"),
        _ => return Err("handoff accepts only --template NAME"),
    };
    if input.trim().is_empty() {
        return Err("handoff requires a message on stdin");
    }
    if input.len() > MAX_HANDOFF_BYTES {
        return Err("handoff exceeds 32768 bytes");
    }
    Ok(HandoffRequest {
        template,
        message: input.to_string(),
    })
}

fn comments(rest: &[&str]) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", comments_message(rest, &env_url(), &env_token()));
    let _ = out.flush();
}

fn comments_message(rest: &[&str], base_url: &str, token: &str) -> String {
    if !rest.is_empty() {
        return "could not read comments: comments takes no arguments".to_string();
    }
    let Some((status, body)) =
        post_response(&endpoint_url(base_url, Endpoint::Comments), token, "")
    else {
        return "could not read comments: daemon unavailable".to_string();
    };
    if status != 200 {
        let reason = body.trim();
        return if reason.is_empty() {
            "could not read comments: daemon refused the request".to_string()
        } else {
            format!("could not read comments: {reason}")
        };
    }
    let Ok(comments) = serde_json::from_str::<Vec<ReviewComment>>(&body) else {
        return "could not read comments: invalid daemon response".to_string();
    };
    if comments.is_empty() {
        return "no review comments".to_string();
    }
    comments
        .iter()
        .map(|comment| {
            format!(
                "#{} [{}] {}",
                comment.id,
                comment.anchor.base.label(),
                comment.anchor.notification(&comment.body)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn installed_hook(url: &str, rest: &[&str]) {
    let Some(token) = rest.first() else { return };
    let (key, raw, note) = installed_input(rest);
    let inherited_url = env_url();
    let inherited_token = env_token();
    let (url, token) = routed_hook(url, token, &inherited_url, &inherited_token);
    let session = key.and_then(|key| raw.as_deref().and_then(|raw| json_string(raw, key)));
    let _ = post_as(&url, &token, &note, session.as_deref());
    if rest.contains(&OWNS_SESSION_FLAG) {
        post_session_id(&url, &token, session.as_deref());
    }

    let mut out = std::io::stdout();
    let _ = writeln!(
        out,
        "{}",
        hook_reply(
            raw.as_deref(),
            rest.contains(&"--inject-instructions"),
            &env_instructions(),
        )
    );
    let _ = out.flush();
}

/// The JSON a hook runner needs so it does not treat bookkeeping as a
/// denied tool or a blocked prompt. Claude Code keys off `toolCall` and
/// wants `decision`; Cursor keys off `tool_name` and wants `permission`.
fn hook_reply(raw: Option<&str>, inject_instructions: bool, instructions: &str) -> String {
    let raw = raw.unwrap_or("");
    if raw.contains("\"toolCall\"") {
        return r#"{"decision":"allow"}"#.to_string();
    }
    if raw.contains("\"tool_name\"") {
        return r#"{"permission":"allow"}"#.to_string();
    }
    if (raw.contains("\"invocationNum\"") || inject_instructions) && !instructions.is_empty() {
        return serde_json::json!({
            "injectSteps": [{ "ephemeralMessage": instructions }]
        })
        .to_string();
    }
    "{}".to_string()
}

fn env_instructions() -> String {
    std::env::var("ARGUS_INSTRUCTIONS").unwrap_or_default()
}

fn installed_input<'a>(rest: &'a [&str]) -> (Option<&'a str>, Option<String>, String) {
    let key = rest
        .iter()
        .position(|arg| *arg == SESSION_KEY_FLAG)
        .and_then(|index| rest.get(index + 1))
        .copied();
    let raw =
        (rest.contains(&NOTE_FLAG) || key.is_some()).then(|| read_hook_input(std::io::stdin()));
    let note = if rest.contains(&NOTE_FLAG) {
        raw.as_deref().map(note_from).unwrap_or_default()
    } else {
        String::new()
    };
    (key, raw, note)
}

/// Records the conversation identity Argus resumes this pane with. Only the
/// event a harness fires when *its own* session starts carries the flag that
/// gets here, so a CLI started from inside the pane cannot claim it.
fn post_session_id(url: &str, token: &str, id: Option<&str>) {
    let Some(id) = id.filter(|id| !id.is_empty()) else {
        return;
    };
    if let Some(base) = pane_base(url) {
        let _ = post_as(&endpoint_url(&base, Endpoint::Session), token, id, Some(id));
    }
}

fn reported_checkout(args: &[&str]) -> Option<std::path::PathBuf> {
    if args.is_empty() {
        std::env::current_dir().ok()
    } else {
        Some(std::path::PathBuf::from(args.join(" ")))
    }
}

fn env_url() -> String {
    std::env::var("ARGUS_HOOK_URL").unwrap_or_default()
}

fn env_token() -> String {
    std::env::var("ARGUS_HOOK_TOKEN").unwrap_or_default()
}

/// The message a harness hands its hook on stdin.
///
/// Cursor's runner writes one JSON object and then waits for stdout without
/// closing the pipe. Reading to EOF would deadlock until the hook timeout
/// killed the process — after which the status POST never ran. One complete
/// JSON value is enough; plain text still reads to the end of the stream.
fn read_hook_input(mut reader: impl Read) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Ok(s) = std::str::from_utf8(&buf) {
                    if json_value(s).is_some() {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn json_value(raw: &str) -> Option<serde_json::Value> {
    let trimmed = raw.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let mut de = serde_json::Deserializer::from_str(trimmed);
    serde::Deserialize::deserialize(&mut de).ok()
}

fn json_string(raw: &str, key: &str) -> Option<String> {
    let v = json_value(raw)?;
    if let Some(s) = v
        .get(key)
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
    {
        return Some(s.to_string());
    }
    // Cursor's sessionStart names the same id `session_id`; every other
    // event puts it on `conversation_id`. Asking for either must find both.
    for alias in ["conversation_id", "session_id"] {
        if alias == key {
            continue;
        }
        if let Some(s) = v
            .get(alias)
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

/// Repoint a checkout-wide managed hook at the pane-specific URL inherited
/// by this process. Both URLs must name panes on the same loopback listener.
fn rebase_hook_url(configured: &str, inherited: &str) -> Option<String> {
    let configured_base = pane_base(configured)?;
    let inherited_base = pane_base(inherited)?;
    if authority(&configured_base)? != authority(&inherited_base)? {
        return None;
    }
    let suffix = configured.strip_prefix(&configured_base)?;
    (!suffix.is_empty()).then(|| format!("{inherited_base}{suffix}"))
}

fn routed_hook(
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

fn authority(url: &str) -> Option<&str> {
    url.strip_prefix("http://")?.split('/').next()
}

/// A pane base (`http://host:port/pane/<id>`) plus the endpoint being asked
/// for. The suffix comes from `argus-protocol` so the daemon parses exactly
/// what is built here.
fn endpoint_url(base: &str, endpoint: Endpoint) -> String {
    format!("{}/{}", base.trim_end_matches('/'), endpoint.suffix())
}

fn pane_base(url: &str) -> Option<String> {
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

/// Harnesses hand hooks a JSON event where they can. `message` is Claude
/// Code's field for the text of what it is waiting on; a harness that sends
/// plain text instead still gets its first line used.
fn note_from(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(tool) = v.get("toolCall") {
            let name = tool.get("name").and_then(|v| v.as_str()).unwrap_or("tool");
            if let Some(cmd) = tool
                .get("args")
                .and_then(|a| a.get("CommandLine"))
                .and_then(|v| v.as_str())
            {
                return format!("{name}: {cmd}");
            }
            return name.to_string();
        }
        for key in ["message", "text", "reason", "prompt"] {
            if let Some(s) = v.get(key).and_then(|v| v.as_str()) {
                if !s.trim().is_empty() {
                    return s.trim().to_string();
                }
            }
        }
        // Valid JSON with nothing we recognize is not worth showing raw.
        return String::new();
    }
    raw.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// Best-effort POST. Every error is discarded by the caller; the return type
/// exists only so the body can use `?`.
fn post(url: &str, token: &str, body: &str) -> Option<()> {
    post_as(url, token, body, None)
}

fn post_as(url: &str, token: &str, body: &str, session: Option<&str>) -> Option<()> {
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

fn post_response(url: &str, token: &str, body: &str) -> Option<(u16, String)> {
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
fn request(path: &str, authority: &str, token: &str, session: Option<&str>, body: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_names_the_conversation_it_came_from() {
        // What lets the daemon tell the pane's own agent from a CLI started
        // inside it, which inherits the same URL and token.
        let tagged = request(
            "/pane/1/status/idle",
            "127.0.0.1:4242",
            "tok",
            Some("s-1"),
            "",
        );
        assert!(tagged.contains("\r\nX-Argus-Session: s-1\r\n"), "{tagged}");
        let untagged = request("/pane/1/status/idle", "127.0.0.1:4242", "tok", None, "");
        assert!(!untagged.contains("X-Argus-Session"), "{untagged}");
        assert!(untagged.contains("\r\nContent-Length: 0\r\n"), "{untagged}");
    }

    #[test]
    fn every_header_starts_its_own_line_at_column_zero() {
        // An indented header line is a continuation of the one above it, so
        // a stray space here is invisible on the wire and silently costs the
        // daemon whichever header it swallowed: a session header it misses
        // files a child's report on its parent's row, and a Content-Length
        // it misses drops the note the report was carrying.
        let req = request("/pane/1/title", "127.0.0.1:4242", "tok", Some("s-1"), "hi");
        let (head, body) = req
            .split_once("\r\n\r\n")
            .expect("a blank line ends the headers");
        assert_eq!(body, "hi");
        for line in head.split("\r\n") {
            assert_eq!(line.trim_start(), line, "indented header line: {req:?}");
            assert!(!line.is_empty(), "blank header line: {req:?}");
        }
        assert!(head.contains("\r\nContent-Length: 2\r\n"), "{req:?}");
    }

    #[test]
    fn a_json_event_gives_up_the_message_a_human_would_read() {
        assert_eq!(
            note_from(
                r#"{"session_id":"x","message":"Claude needs your permission to run tests"}"#
            ),
            "Claude needs your permission to run tests"
        );
    }

    #[test]
    fn one_json_event_can_supply_a_note_and_session_id() {
        let raw = r#"{"session_id":"session-123","message":"waiting"}"#;
        assert_eq!(note_from(raw), "waiting");
        assert_eq!(
            json_string(raw, "session_id").as_deref(),
            Some("session-123")
        );
    }

    #[test]
    fn cursor_session_start_names_the_id_session_id() {
        // sessionStart's documented payload uses session_id; other events
        // put the same value on conversation_id. The helper asks for either.
        let start = r#"{"session_id":"conv-9","composer_mode":"agent"}"#;
        assert_eq!(
            json_string(start, "conversation_id").as_deref(),
            Some("conv-9")
        );
        let tool = r#"{"conversation_id":"conv-9","tool_name":"Shell"}"#;
        assert_eq!(json_string(tool, "session_id").as_deref(), Some("conv-9"));
    }

    #[test]
    fn hook_stdin_stops_at_one_json_object_without_waiting_for_eof() {
        struct JsonThenHang {
            data: &'static [u8],
        }
        impl Read for JsonThenHang {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.data.is_empty() {
                    panic!("hook stdin was read past the JSON object");
                }
                let n = self.data.len().min(buf.len());
                buf[..n].copy_from_slice(&self.data[..n]);
                self.data = &self.data[n..];
                Ok(n)
            }
        }
        let raw = read_hook_input(JsonThenHang {
            data: br#"{"conversation_id":"conv-9","tool_name":"Shell"}"#,
        });
        assert_eq!(
            json_string(&raw, "conversation_id").as_deref(),
            Some("conv-9")
        );
    }

    #[test]
    fn hook_stdin_plain_text_still_reads_to_eof() {
        assert_eq!(
            read_hook_input(std::io::Cursor::new("waiting on review\n")),
            "waiting on review\n"
        );
    }

    #[test]
    fn cursor_tool_hooks_allow_with_permission_and_claude_with_decision() {
        assert_eq!(
            hook_reply(
                Some(r#"{"tool_name":"Shell","conversation_id":"c"}"#),
                false,
                ""
            ),
            r#"{"permission":"allow"}"#
        );
        assert_eq!(
            hook_reply(Some(r#"{"toolCall":{"name":"Bash"}}"#), false, ""),
            r#"{"decision":"allow"}"#
        );
        assert_eq!(hook_reply(Some(r#"{"session_id":"c"}"#), false, ""), "{}");
    }

    #[test]
    fn a_checkout_wide_hook_url_rebases_to_the_process_pane() {
        assert_eq!(
            rebase_hook_url(
                "http://127.0.0.1:4242/pane/1/status/idle",
                "http://127.0.0.1:4242/pane/9"
            )
            .as_deref(),
            Some("http://127.0.0.1:4242/pane/9/status/idle")
        );
        assert!(rebase_hook_url(
            "http://127.0.0.1:4242/pane/1/status/idle",
            "http://127.0.0.1:9999/pane/9"
        )
        .is_none());
    }

    #[test]
    fn a_rebased_hook_uses_the_process_token_too() {
        assert_eq!(
            routed_hook(
                "http://127.0.0.1:4242/pane/1/status/idle",
                "configured-token",
                "http://127.0.0.1:4242/pane/9",
                "process-token"
            ),
            (
                "http://127.0.0.1:4242/pane/9/status/idle".to_string(),
                "process-token".to_string()
            )
        );
    }

    #[test]
    fn an_incomplete_or_foreign_process_pair_keeps_the_configured_pair() {
        let configured = "http://127.0.0.1:4242/pane/1/status/idle";
        assert_eq!(
            routed_hook(configured, "configured-token", "", "process-token"),
            (configured.to_string(), "configured-token".to_string())
        );
        assert_eq!(
            routed_hook(
                configured,
                "configured-token",
                "http://127.0.0.1:9999/pane/9",
                "process-token"
            ),
            (configured.to_string(), "configured-token".to_string())
        );
        assert_eq!(
            routed_hook(
                configured,
                "configured-token",
                "http://127.0.0.1:4242/pane/9",
                ""
            ),
            (configured.to_string(), "configured-token".to_string())
        );
    }

    #[test]
    fn plain_text_falls_back_to_its_first_real_line() {
        // A harness that hands its hooks text rather than JSON.
        assert_eq!(
            note_from("\n\n  waiting on review  \nmore"),
            "waiting on review"
        );
    }

    #[test]
    fn json_with_nothing_recognizable_shows_nothing() {
        // Better an empty note than a wall of serialized event under a row.
        assert_eq!(note_from(r#"{"session_id":"x","cwd":"/tmp"}"#), "");
        assert_eq!(note_from(""), "");
    }

    #[test]
    fn an_explicit_checkout_path_keeps_spaces() {
        assert_eq!(
            reported_checkout(&["C:\\Source\\my", "checkout"]),
            Some(std::path::PathBuf::from("C:\\Source\\my checkout"))
        );
    }

    #[test]
    fn checkout_without_a_path_reports_the_current_directory() {
        assert_eq!(reported_checkout(&[]), std::env::current_dir().ok());
    }

    #[test]
    fn delegation_accepts_an_optional_template_and_joins_the_task() {
        assert_eq!(
            delegate_args(&["review", "DESIGN.md"]).unwrap(),
            DelegateRequest {
                template: None,
                task: "review DESIGN.md".into(),
            }
        );
        assert_eq!(
            delegate_args(&["--template", "codex", "review", "the", "diff"]).unwrap(),
            DelegateRequest {
                template: Some("codex".into()),
                task: "review the diff".into(),
            }
        );
    }

    #[test]
    fn delegation_requires_a_task_and_a_template_name() {
        assert_eq!(delegate_args(&[]), Err("delegate requires a task"));
        assert_eq!(
            delegate_args(&["--template"]),
            Err("--template requires a name and task")
        );
        assert_eq!(
            delegate_args(&["--template", "codex"]),
            Err("delegate requires a task")
        );
    }

    #[test]
    fn delegation_posts_the_request_and_reports_the_created_pane() {
        use std::io::BufRead as _;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response_body = serde_json::to_string(&DelegateResponse {
            pane: argus_protocol::PaneId(9),
        })
        .unwrap();
        let response = format!(
            "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let content_length = head
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .unwrap()
                .parse()
                .unwrap();
            let mut body = vec![0; content_length];
            reader.read_exact(&mut body).unwrap();
            reader.get_mut().write_all(response.as_bytes()).unwrap();
            (head, body)
        });

        let message = delegation_message(
            &["--template", "codex", "review", "the", "diff"],
            &format!("http://{address}/pane/4"),
            "secret",
        );
        let (head, body) = server.join().unwrap();

        assert!(message.starts_with("opened agent pane 9;"), "{message}");
        assert!(head.starts_with("POST /pane/4/delegate HTTP/1.1\r\n"));
        assert!(head.contains("\r\nAuthorization: Bearer secret\r\n"));
        assert_eq!(
            serde_json::from_slice::<DelegateRequest>(&body).unwrap(),
            DelegateRequest {
                template: Some("codex".into()),
                task: "review the diff".into(),
            }
        );
        assert_eq!(
            delegation_message(&[], "", ""),
            "could not open agent: delegate requires a task"
        );
    }

    #[test]
    fn handoff_accepts_stdin_and_an_optional_template() {
        assert_eq!(
            handoff_args(&[], "# Handoff\nContinue the review").unwrap(),
            HandoffRequest {
                template: None,
                message: "# Handoff\nContinue the review".into(),
            }
        );
        assert_eq!(
            handoff_args(&["--template", "codex"], "continue").unwrap(),
            HandoffRequest {
                template: Some("codex".into()),
                message: "continue".into(),
            }
        );
    }

    #[test]
    fn handoff_requires_bounded_stdin_and_valid_options() {
        assert_eq!(
            handoff_args(&[], "   "),
            Err("handoff requires a message on stdin")
        );
        assert_eq!(
            handoff_args(&["--template"], "continue"),
            Err("--template requires a name")
        );
        assert_eq!(
            handoff_args(&["unexpected"], "continue"),
            Err("handoff accepts only --template NAME")
        );
        assert_eq!(
            handoff_args(&[], &"x".repeat(MAX_HANDOFF_BYTES + 1)),
            Err("handoff exceeds 32768 bytes")
        );
    }

    #[test]
    fn handoff_stdin_is_read_through_a_bounded_utf8_buffer() {
        assert_eq!(
            read_handoff(std::io::Cursor::new("continue")),
            Ok("continue".into())
        );
        assert_eq!(
            read_handoff(std::io::Cursor::new(vec![b'x'; MAX_HANDOFF_BYTES + 1])),
            Err("handoff exceeds 32768 bytes")
        );
        assert_eq!(
            read_handoff(std::io::Cursor::new(vec![0xff])),
            Err("handoff on stdin is not UTF-8")
        );
    }

    #[test]
    fn handoff_posts_stdin_without_using_the_checkout() {
        use std::io::BufRead as _;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response_body = serde_json::to_string(&DelegateResponse {
            pane: argus_protocol::PaneId(12),
        })
        .unwrap();
        let response = format!(
            "HTTP/1.1 201 Created\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let content_length = head
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .unwrap()
                .parse()
                .unwrap();
            let mut body = vec![0; content_length];
            reader.read_exact(&mut body).unwrap();
            reader.get_mut().write_all(response.as_bytes()).unwrap();
            (head, body)
        });

        let message = handoff_message(
            &["--template", "codex"],
            "# Handoff\nContinue the review",
            &format!("http://{address}/pane/4"),
            "secret",
        );
        let (head, body) = server.join().unwrap();

        assert!(message.starts_with("opened agent pane 12;"), "{message}");
        assert!(head.starts_with("POST /pane/4/handoff HTTP/1.1\r\n"));
        assert_eq!(
            serde_json::from_slice::<HandoffRequest>(&body).unwrap(),
            HandoffRequest {
                template: Some("codex".into()),
                message: "# Handoff\nContinue the review".into(),
            }
        );
    }

    #[test]
    fn comments_are_read_from_the_daemon_and_rendered_in_order() {
        use std::io::BufRead as _;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let comments = vec![ReviewComment {
            id: 4,
            anchor: argus_protocol::ReviewAnchor {
                base: argus_protocol::ReviewBase::Staged,
                commit: None,
                path: "src/main.rs".to_string(),
                old_path: None,
                old_start: Some(9),
                old_end: Some(9),
                new_start: Some(10),
                new_end: Some(10),
                text: vec!["+changed".to_string()],
            },
            body: "fix this".to_string(),
        }];
        let response_body = serde_json::to_string(&comments).unwrap();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            reader.get_mut().write_all(response.as_bytes()).unwrap();
            head
        });

        let message = comments_message(&[], &format!("http://{address}/pane/4"), "secret");
        let head = server.join().unwrap();

        assert_eq!(message, "#4 [staged] src/main.rs:10 `+changed`: fix this");
        assert!(head.starts_with("POST /pane/4/comments HTTP/1.1\r\n"));
        assert!(head.contains("\r\nAuthorization: Bearer secret\r\n"));
        assert_eq!(
            comments_message(&["extra"], "", ""),
            "could not read comments: comments takes no arguments"
        );
    }
}
