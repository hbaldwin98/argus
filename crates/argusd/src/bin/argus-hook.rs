//! The command Argus's managed agent hooks run, and the one an agent runs
//! itself to say what it is doing.
//!
//! ```text
//! argus-hook title "fixing the pty deadlock"
//! argus-hook status waiting "needs the staging database password"
//! argus-hook status working
//! argus-hook checkout                            # reports the current directory
//! argus-hook say "text"                       # prints, calls nobody
//! argus-hook <url> <token> [--note-from-stdin]  # the installed hook form
//! ```
//!
//! The first two forms read `ARGUS_HOOK_URL` and `ARGUS_HOOK_TOKEN` from the
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args
        .get(1..)
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("say") => {
            // Deliberately on stdout: this is context for the model.
            let mut out = std::io::stdout();
            let _ = writeln!(out, "{}", rest.join(" "));
            let _ = out.flush();
        }
        Some("title") => {
            let text = rest.join(" ");
            if !text.trim().is_empty() {
                let _ = post(&format!("{}/title", env_url()), &env_token(), &text);
            }
        }
        Some("status") => {
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
        Some("checkout") => {
            if let Some(path) = reported_checkout(&rest) {
                let _ = post(
                    &format!("{}/checkout", env_url()),
                    &env_token(),
                    &path.to_string_lossy(),
                );
            }
        }
        // The installed-hook form: an absolute URL and the token, because a
        // harness's hook config can't count on the environment reaching it.
        Some(url) if url.starts_with("http://") => {
            let Some(token) = rest.first() else { return };
            let note = if rest.contains(&NOTE_FLAG) {
                read_note()
            } else {
                String::new()
            };
            let _ = post(url, token, &note);
        }
        _ => {}
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
fn read_note() -> String {
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return String::new();
    }
    note_from(&raw)
}

/// Harnesses hand hooks a JSON event where they can. `message` is Claude
/// Code's field for the text of what it is waiting on; a harness that sends
/// plain text instead still gets its first line used.
fn note_from(raw: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
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
            note_from(r#"{"session_id":"x","message":"Claude needs your permission to run tests"}"#),
            "Claude needs your permission to run tests"
        );
    }

    #[test]
    fn plain_text_falls_back_to_its_first_real_line() {
        // A harness that hands its hooks text rather than JSON.
        assert_eq!(note_from("\n\n  waiting on review  \nmore"), "waiting on review");
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
