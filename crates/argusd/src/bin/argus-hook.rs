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
//! argus-hook say "text"                       # prints, calls nobody
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
//! It writes nothing to stdout except for `say`. Some agent CLIs inject a
//! hook's stdout into the model's context, so staying silent keeps Argus's
//! bookkeeping out of the conversation — and `say` is the one case where
//! putting something there is the whole point.
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

const TIMEOUT: Duration = Duration::from_secs(2);
const NOTE_FLAG: &str = "--note-from-stdin";
const SESSION_KEY_FLAG: &str = "--session-id-from-stdin";

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
        let _ = post(&format!("{}/title", env_url()), &env_token(), &text);
    }
}

fn status(rest: &[&str]) {
    let Some(state) = rest.first() else { return };
    // Anything after the state is the reason, so
    // `status waiting "needs a password"` reads the way you'd say it.
    let note = rest[1..].join(" ");
    let _ = post(
        &format!("{}/status/{state}", env_url()),
        &env_token(),
        &note,
    );
}

fn checkout(rest: &[&str]) {
    if let Some(path) = reported_checkout(rest) {
        let _ = post(
            &format!("{}/checkout", env_url()),
            &env_token(),
            &path.to_string_lossy(),
        );
    }
}

fn session(rest: &[&str]) {
    let id = rest.join(" ");
    if !id.is_empty() {
        let _ = post(&format!("{}/session", env_url()), &env_token(), &id);
    }
}

fn installed_hook(url: &str, rest: &[&str]) {
    let Some(token) = rest.first() else { return };
    let (key, raw, note) = installed_input(rest);
    let inherited_url = env_url();
    let inherited_token = env_token();
    let (url, token) = routed_hook(url, token, &inherited_url, &inherited_token);
    let _ = post(&url, &token, &note);
    post_session_id(&url, &token, key, raw.as_deref());

    let mut out = std::io::stdout();
    let is_pre_tool = raw.as_deref().is_some_and(|r| r.contains("\"toolCall\""));
    let is_pre_inv = raw.as_deref().is_some_and(|r| r.contains("\"invocationNum\""));
    let instructions = env_instructions();

    if is_pre_tool {
        let _ = writeln!(out, r#"{{"decision":"allow"}}"#);
    } else if (is_pre_inv || rest.contains(&"--inject-instructions")) && !instructions.is_empty() {
        let payload = serde_json::json!({
            "injectSteps": [
                {
                    "ephemeralMessage": instructions
                }
            ]
        });
        let _ = writeln!(out, "{}", payload);
    } else {
        let _ = writeln!(out, "{{}}");
    }
    let _ = out.flush();
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
    let raw = (rest.contains(&NOTE_FLAG) || key.is_some()).then(read_stdin);
    let note = if rest.contains(&NOTE_FLAG) {
        raw.as_deref().map(note_from).unwrap_or_default()
    } else {
        String::new()
    };
    (key, raw, note)
}

fn post_session_id(url: &str, token: &str, key: Option<&str>, raw: Option<&str>) {
    let Some(id) = key.and_then(|key| raw.and_then(|raw| json_string(raw, key))) else {
        return;
    };
    if let Some(base) = pane_base(url) {
        let _ = post(&format!("{base}/session"), token, &id);
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

/// The message a harness hands its hook on stdin, reduced to the one line
/// worth showing under a pane. Only called when Argus wrote the flag asking
/// for it, so there is always a writer on the other end.
fn read_stdin() -> String {
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return String::new();
    }
    raw
}

fn json_string(raw: &str, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()?
        .get(key)?
        .as_str()
        .map(str::to_string)
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
            if let Some(cmd) = tool.get("args").and_then(|a| a.get("CommandLine")).and_then(|v| v.as_str()) {
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
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    let addr = authority.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;

    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {token}\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).ok()?;
    // The daemon's reply is deliberately not read: nothing here acts on it,
    // and not waiting keeps the agent's turn from stalling on a slow answer.
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
