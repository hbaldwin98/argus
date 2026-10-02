//! Relaying the pane's inbox for a plugin that cannot hold it open itself.
//!
//! The inbox is a stream of server-sent events kept open for as long as the
//! agent runs. Claude Code's mods have no streaming HTTP — `$.http.fetch`
//! answers once the body is read, which an inbox never is — but they can
//! read a child's output as it comes. So the mod runs `argus-hook inbox`,
//! which holds the stream and prints each item as one JSON line, and exits
//! when the stream ends, leaving the mod to open it again.
//!
//! ```text
//! argus-hook inbox <session-id>
//! ```

use std::io::{BufRead, BufReader};

use super::*;

pub(super) fn inbox(rest: &[&str]) {
    let session = rest.first().copied().filter(|id| !id.is_empty());
    let url = endpoint_url(&env_url(), Endpoint::Inbox);
    let _ = relay(&url, &env_token(), session, std::io::stdout());
}

/// Copies the inbox at `url` to `out`, one item a line, until the daemon
/// closes it or refuses it.
///
/// The session is the agent's own: only the pane's own session may open
/// its inbox, so a CLI started inside the pane cannot take it.
pub(super) fn relay(url: &str, token: &str, session: Option<&str>, mut out: impl Write) -> Option<()> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = rest.split_at(rest.find('/')?);
    let mut stream = TcpStream::connect_timeout(&authority.parse().ok()?, TIMEOUT).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;
    let session = session.map(|id| format!("{SESSION_HEADER}: {id}\r\n")).unwrap_or_default();
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {authority}\r\nAuthorization: Bearer {token}\r\n{session}\r\n"
    );
    stream.write_all(request.as_bytes()).ok()?;

    // No read timeout: a quiet inbox says it is still there every fifteen
    // seconds, and a daemon that has gone closes the socket.
    let mut lines = BufReader::new(stream).lines();
    let status = lines.next()?.ok()?;
    if status.split_whitespace().nth(1) != Some("200") {
        return None;
    }
    for line in lines {
        let line = line.ok()?;
        if let Some(item) = line.strip_prefix("data: ") {
            writeln!(out, "{item}").ok()?;
            out.flush().ok()?;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_item_becomes_a_line_and_nothing_else_does() {
        use std::io::BufRead as _;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
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
            reader
                .get_mut()
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n\
                      data: {\"Message\":{\"text\":\"hi\",\"steer\":false}}\n\n\
                      : still here\n\n\
                      data: \"Interrupt\"\n\n",
                )
                .unwrap();
            head
        });

        let mut out = Vec::new();
        relay(&format!("http://{address}/pane/4/inbox"), "tok", Some("s-1"), &mut out);
        let head = server.join().unwrap();

        assert!(head.starts_with("GET /pane/4/inbox HTTP/1.1\r\n"), "{head}");
        assert!(head.contains("\r\nAuthorization: Bearer tok\r\n"), "{head}");
        assert!(head.contains("\r\nX-Argus-Session: s-1\r\n"), "{head}");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "{\"Message\":{\"text\":\"hi\",\"steer\":false}}\n\"Interrupt\"\n"
        );
    }

    #[test]
    fn a_refused_inbox_prints_nothing() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut scratch = [0u8; 512];
            let _ = stream.read(&mut scratch);
            stream.write_all(b"HTTP/1.1 409 Conflict\r\n\r\ndata: no\n\n").unwrap();
        });

        let mut out = Vec::new();
        assert!(relay(&format!("http://{address}/pane/4/inbox"), "tok", None, &mut out).is_none());
        server.join().unwrap();
        assert!(out.is_empty());
    }
}
