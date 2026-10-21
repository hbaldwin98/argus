//! The pane API's DevTools socket: an agent's browser tool attached to its
//! pane's browser, behind the pane API's token, since the browser itself
//! listens on no port another process could reach.

use std::sync::Arc;

use argus_protocol::{query_token, PaneId};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use super::http::{Request, Response};
use crate::state::Daemon;

/// Upgrades the request to a WebSocket and relays it to and from a line of
/// the pane's browser until either end goes.
pub(super) async fn serve(
    reader: BufReader<ReadHalf<TcpStream>>,
    mut wr: WriteHalf<TcpStream>,
    daemon: Arc<Daemon>,
    pane: PaneId,
    request: Request,
) -> anyhow::Result<()> {
    if !request.authorized && query_token(&request.target) != Some(daemon.hook_token()) {
        return Response::empty(401, "Unauthorized").write(&mut wr).await;
    }
    let Some(key) = request.websocket_key else {
        let why = "the DevTools endpoint takes a WebSocket; give it to a browser tool, such as `agent-browser --cdp <url>`";
        return Response::text(400, "Bad Request", why.to_string()).write(&mut wr).await;
    };
    let Some((line, mut heard)) = daemon.attach_browser(pane) else {
        let why = "this pane has no browser open; `argus-hook browser` opens one";
        return Response::text(409, "Conflict", why.to_string()).write(&mut wr).await;
    };
    let accept = derive_accept_key(key.as_bytes());
    let head = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    wr.write_all(head.as_bytes()).await?;
    // A client sends nothing until it is answered, so nothing past the
    // head was read into the buffer that is dropped here.
    let stream = reader.into_inner().unsplit(wr);
    let socket = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
    let (mut sink, mut source) = socket.split();
    loop {
        tokio::select! {
            from_tool = source.next() => match from_tool {
                Some(Ok(Message::Text(text))) => line.send(&text),
                Some(Ok(Message::Binary(bytes))) => line.send(&String::from_utf8_lossy(&bytes)),
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            from_browser = heard.recv() => match from_browser {
                Some(text) => {
                    if sink.send(Message::text(text)).await.is_err() {
                        break;
                    }
                }
                // The browser closed, or its pane went.
                None => {
                    let _ = sink.send(Message::Close(None)).await;
                    break;
                }
            },
        }
    }
    Ok(())
}
