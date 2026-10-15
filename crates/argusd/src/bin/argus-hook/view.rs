//! `argus-hook view`: showing the person a view of something the agent read
//! with its own tools, and taking it down.
//!
//! The view's JSON is the agent's to write and the daemon's to check, so
//! this only wraps it for the `view` endpoint and relays the daemon's
//! refusal, which names the path of the block at fault, as the answer.
//! Like every deliberate command it exits 0 and says on stdout how it went.

use std::io::{Read, Write};

use argus_protocol::{AgentView, Block, Endpoint, ViewChange};

use super::*;

pub(super) fn view(rest: &[&str]) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", view_message(rest, &env_url(), &env_token(), std::io::stdin()));
}

/// `view show --stdin`, `view patch <id> <path> --stdin`, `view close <id>`.
pub(super) fn view_message(
    rest: &[&str],
    base_url: &str,
    token: &str,
    mut stdin: impl Read,
) -> String {
    let (change, done) = match rest {
        ["show", "--stdin"] => {
            let mut body = String::new();
            if let Err(e) = stdin.read_to_string(&mut body) {
                return format!("could not show the view: {e}");
            }
            let view: AgentView = match serde_json::from_str(&body) {
                Ok(view) => view,
                Err(e) => return format!("could not show the view: not a view: {e}"),
            };
            let done = format!("shown: {}, {} blocks", view.id.trim(), view.blocks.len());
            (ViewChange::Show { view }, done)
        }
        ["show", ..] => return "could not show the view: show wants --stdin with the view's JSON".into(),
        ["patch", id, path, "--stdin"] if !id.trim().is_empty() && !path.trim().is_empty() => {
            let mut body = String::new();
            if let Err(e) = stdin.read_to_string(&mut body) {
                return format!("could not patch the view: {e}");
            }
            let block: Block = match serde_json::from_str(&body) {
                Ok(block) => block,
                Err(e) => return format!("could not patch the view: not a block: {e}"),
            };
            let done = format!("patched: {id} {path}");
            (ViewChange::Patch { id: id.to_string(), path: path.to_string(), block }, done)
        }
        ["patch", ..] => {
            return "could not patch the view: patch wants the view's id, the block's path and --stdin with \
                    the block's JSON"
                .into()
        }
        ["close", id] if !id.trim().is_empty() => {
            (ViewChange::Close { id: id.to_string() }, format!("closed: {id}"))
        }
        ["close", ..] => return "could not close the view: close wants the view's id".into(),
        _ => return "view: use `view show --stdin`, `view patch <id> <path> --stdin` or `view close <id>`".into(),
    };
    let Ok(body) = serde_json::to_string(&change) else {
        return "could not change the view: unencodable".into();
    };
    let url = endpoint_url(base_url, Endpoint::View);
    let Some((status, response)) = post_response(&url, token, &body) else {
        return "could not change the view: daemon unavailable".into();
    };
    match status {
        200 => done,
        _ => format!("could not change the view: {}", response.trim()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::net::TcpListener;

    const BASE: &str = "http://127.0.0.1:1/pane/1";
    const VIEW: &str = r#"{"id":"eng-1","title":"ENG-1","blocks":[{"kind":"markdown","text":"hi"}]}"#;

    /// A daemon that answers one request with `status` and `body`, and the
    /// body it was sent, once it has.
    fn answering(status: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
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

    #[test]
    fn showing_a_view_wraps_it_for_the_endpoint_and_counts_its_blocks() {
        let (base, server) = answering("200 OK", "");
        let said = view_message(&["show", "--stdin"], &base, "t", Cursor::new(VIEW));
        assert_eq!(said, "shown: eng-1, 1 blocks");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /pane/1/view"), "{request}");
        assert!(request.contains(r#""action":"show""#) && request.contains(r#""id":"eng-1""#), "{request}");
    }

    #[test]
    fn the_daemons_refusal_is_the_answer() {
        let (base, server) = answering("409 Conflict", "blocks[0]: a table is at most 200 rows");
        let said = view_message(&["show", "--stdin"], &base, "t", Cursor::new(VIEW));
        server.join().unwrap();
        assert_eq!(said, "could not change the view: blocks[0]: a table is at most 200 rows");
    }

    #[test]
    fn closing_a_view_sends_its_id() {
        let (base, server) = answering("200 OK", "");
        assert_eq!(view_message(&["close", "eng-1"], &base, "t", Cursor::new("")), "closed: eng-1");
        let request = server.join().unwrap();
        assert!(request.contains(r#"{"action":"close","id":"eng-1"}"#), "{request}");
    }

    #[test]
    fn patching_a_view_sends_the_path_and_the_block() {
        let (base, server) = answering("200 OK", "");
        let block = r#"{"kind":"markdown","text":"newer"}"#;
        let args = ["patch", "eng-1", "blocks[1].columns[0][2]", "--stdin"];
        let said = view_message(&args, &base, "t", Cursor::new(block));
        assert_eq!(said, "patched: eng-1 blocks[1].columns[0][2]");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /pane/1/view"), "{request}");
        assert!(request.contains(r#""action":"patch""#), "{request}");
        assert!(request.contains(r#""path":"blocks[1].columns[0][2]""#), "{request}");
        assert!(request.contains(r#""text":"newer""#), "{request}");
    }

    #[test]
    fn the_daemons_refusal_of_a_patch_is_the_answer() {
        let (base, server) = answering("409 Conflict", "blocks[7]: blocks[7] is out of range, there are 3 blocks");
        let block = r#"{"kind":"markdown","text":"x"}"#;
        let said = view_message(&["patch", "eng-1", "blocks[7]", "--stdin"], &base, "t", Cursor::new(block));
        server.join().unwrap();
        assert!(said.starts_with("could not change the view: blocks[7]: "), "{said}");
    }

    #[test]
    fn a_malformed_command_is_refused_before_the_daemon_is_asked() {
        for (args, stdin) in [
            (&["show"][..], VIEW),
            (&["show", "--stdin"], "not json"),
            (&["show", "--stdin"], r#"{"id":"x"}"#),
            (&["patch", "eng-1", "blocks[0]"], r#"{"kind":"divider"}"#),
            (&["patch", "eng-1", "--stdin"], r#"{"kind":"divider"}"#),
            (&["patch", " ", "blocks[0]", "--stdin"], r#"{"kind":"divider"}"#),
            (&["patch", "eng-1", "blocks[0]", "--stdin"], "not json"),
            (&["close"], ""),
            (&["close", " "], ""),
            (&[], ""),
            (&["wipe"], ""),
        ] {
            let said = view_message(args, BASE, "t", Cursor::new(stdin));
            assert!(!said.contains("unavailable") && !said.starts_with("shown"), "{args:?}: {said}");
        }
    }
}
