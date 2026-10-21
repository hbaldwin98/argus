//! The pane API's HTTP: a request's line, headers and bounded body read off
//! the socket, the answer written back, and an event stream held open with
//! its heartbeat.
//!
//! Hand-rolled HTTP/1 rather than a server library: the listener is
//! loopback only, every client is `argus-hook` or an extension's few lines
//! of script, and a request is one line, a handful of headers and a body.

use std::path::PathBuf;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// What a request's first line and headers say about it.
pub(super) struct Request {
    pub(super) method: String,
    pub(super) target: String,
    /// Whether it carried this boot's bearer token.
    pub(super) authorized: bool,
    /// The token the request carried, for an extension's, which is its own
    /// rather than the per-boot one.
    pub(super) bearer: Option<String>,
    pub(super) content_length: usize,
    /// The conversation the report comes from.
    pub(super) reporter: Option<String>,
    /// Whether that conversation left work running behind its turn.
    pub(super) background: bool,
    /// Where that conversation is working.
    pub(super) directory: Option<PathBuf>,
    /// A WebSocket upgrade's key, which its answer must hash.
    pub(super) websocket_key: Option<String>,
}

/// The request line and headers, checked against `token`; the body is left
/// on the reader.
pub(super) async fn read_head<R: AsyncBufRead + Unpin>(reader: &mut R, token: &str) -> anyhow::Result<Request> {
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;
    let (method, target) = request_line.split_once(' ').unwrap_or(("", ""));
    let mut request = Request {
        method: method.to_string(),
        target: target.split(' ').next().unwrap_or("").to_string(),
        authorized: false,
        bearer: None,
        content_length: 0,
        reporter: None,
        background: false,
        directory: None,
        websocket_key: None,
    };
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(v) = strip_header(&line, "Authorization") {
            request.authorized = v.eq_ignore_ascii_case(&format!("Bearer {token}"));
            request.bearer = v
                .split_once(' ')
                .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
                .map(|(_, token)| token.trim().to_string());
        } else if let Some(v) = strip_header(&line, "Content-Length") {
            request.content_length = v.parse().unwrap_or(0);
        } else if let Some(v) = strip_header(&line, argus_protocol::SESSION_HEADER) {
            request.reporter = super::valid_session_id(v);
        } else if let Some(v) = strip_header(&line, argus_protocol::BACKGROUND_HEADER) {
            request.background = !v.is_empty();
        } else if let Some(v) = strip_header(&line, argus_protocol::DIRECTORY_HEADER) {
            request.directory = (!v.is_empty()).then(|| PathBuf::from(v));
        } else if let Some(v) = strip_header(&line, "Sec-WebSocket-Key") {
            request.websocket_key = Some(v.to_string());
        }
    }
    Ok(request)
}

/// One header's value, matched without regard to case — which HTTP allows
/// and the harnesses in the wild disagree about.
fn strip_header<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let (head, value) = line.split_once(':')?;
    head.eq_ignore_ascii_case(name).then(|| value.trim())
}

/// The body the headers announced, or `None` when it is past `most`. With
/// `drain`, a refused body is read past, within reason, rather than left.
pub(super) async fn read_body<R: AsyncRead + Unpin>(
    reader: &mut R,
    length: usize,
    most: usize,
    drain: bool,
) -> Option<Vec<u8>> {
    if length > most {
        if drain {
            // Closing on a body a client is still sending resets the
            // connection, and the client sees that rather than the
            // refusal. A body as big as a view's bound is the case that
            // matters.
            const DRAIN: u64 = 4 * 1024 * 1024;
            let unread = reader.take((length as u64).min(DRAIN));
            let _ = tokio::io::copy(&mut { unread }, &mut tokio::io::sink()).await;
        }
        return None;
    }
    let mut body = vec![0u8; length];
    if !body.is_empty() {
        let _ = reader.read_exact(&mut body).await;
    }
    Some(body)
}

/// One answer, written whole with the connection closed after it.
pub(super) struct Response {
    pub(super) code: u16,
    pub(super) reason: &'static str,
    pub(super) body: Vec<u8>,
}

impl Response {
    pub(super) fn empty(code: u16, reason: &'static str) -> Self {
        Self { code, reason, body: Vec::new() }
    }

    pub(super) fn text(code: u16, reason: &'static str, body: String) -> Self {
        Self { code, reason, body: body.into_bytes() }
    }

    pub(super) fn bytes(&self) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.code,
            self.reason,
            self.body.len()
        )
        .into_bytes();
        response.extend_from_slice(&self.body);
        response
    }

    pub(super) async fn write<W: AsyncWrite + Unpin>(&self, wr: &mut W) -> anyhow::Result<()> {
        wr.write_all(&self.bytes()).await?;
        Ok(())
    }
}

/// How often a quiet event stream says it is still there, so the other end
/// notices a daemon that went away and the daemon notices one that did.
const HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(15);

/// An event stream's head, then one server-sent event per item and a
/// heartbeat while it is quiet, until the channel closes or the other end
/// goes. Whatever ends it, the caller closes its end of the bookkeeping.
///
/// The other end never writes after its request, so anything read — its
/// end of the socket closing — means it has gone, and the stream ends at
/// once rather than at the next item, which would otherwise be sent into a
/// connection nobody holds and lost. An item that cannot be written as JSON
/// ends the stream the same way.
pub(super) async fn serve_events<T, R, W>(
    mut rd: R,
    mut wr: W,
    mut events: tokio::sync::mpsc::UnboundedReceiver<T>,
) -> anyhow::Result<()>
where
    T: serde::Serialize,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\r\n";
    wr.write_all(head.as_bytes()).await?;
    wr.flush().await?;
    let mut scratch = [0u8; 64];
    loop {
        let event = tokio::select! {
            event = events.recv() => match event.map(|event| serde_json::to_string(&event)) {
                Some(Ok(event)) => format!("data: {event}\n\n"),
                Some(Err(error)) => return Err(error.into()),
                None => return Ok(()),
            },
            _ = rd.read(&mut scratch) => return Ok(()),
            _ = tokio::time::sleep(HEARTBEAT) => ": still here\n\n".to_string(),
        };
        wr.write_all(event.as_bytes()).await?;
        wr.flush().await?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An item no JSON can say.
    struct Unsayable;

    impl serde::Serialize for Unsayable {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("unsayable"))
        }
    }

    #[tokio::test]
    async fn an_event_stream_ends_on_an_item_it_cannot_write() {
        let (client, server) = tokio::io::duplex(4096);
        let (rd, wr) = tokio::io::split(server);
        let (tx, events) = tokio::sync::mpsc::unbounded_channel();
        tx.send(Unsayable).unwrap();

        let ended = tokio::time::timeout(std::time::Duration::from_secs(1), serve_events(rd, wr, events)).await;
        assert!(ended.expect("the stream ended").is_err());
        drop(tx);

        let mut seen = String::new();
        let (mut client, _) = tokio::io::split(client);
        let _ = client.read_to_string(&mut seen).await;
        assert!(seen.starts_with("HTTP/1.1 200 OK"), "{seen}");
        assert!(!seen.contains("data:"), "{seen}");
    }

    #[tokio::test]
    async fn an_event_stream_ends_when_its_channel_closes() {
        let (client, server) = tokio::io::duplex(4096);
        let (rd, wr) = tokio::io::split(server);
        let (tx, events) = tokio::sync::mpsc::unbounded_channel();
        tx.send("one").unwrap();
        drop(tx);

        let ended = tokio::time::timeout(std::time::Duration::from_secs(1), serve_events(rd, wr, events)).await;
        assert!(ended.expect("the stream ended").is_ok());

        let mut seen = String::new();
        let (mut client, _) = tokio::io::split(client);
        let _ = client.read_to_string(&mut seen).await;
        assert!(seen.ends_with("data: \"one\"\n\n"), "{seen}");
    }
}
