//! A scene served live to a page in a browser: the page's calls reach a
//! real Session, the stand-in daemon answers what they ask, and the events
//! that come of it go back, so a click in `page/test/screens/audit.mjs`
//! is answered as the window would answer it.
//!
//! Plain HTTP on std's listener, one request at a time: the page is one
//! tab, and a test needs no server crate for it. `POST /connect` with
//! `{"scene": name}` starts that scene afresh and answers what a loaded
//! page hears; `POST /call` with a `Call` answers what it changed.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use serde_json::Value as Json;

use super::{answer, scenes, NOW};
use crate::connection::page::Call;
use crate::session::Session;

/// One request's path and body, or `None` for a connection that sent no
/// whole request.
fn read(stream: &TcpStream) -> Option<(String, Vec<u8>)> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let path = line.split_whitespace().nth(1)?.to_string();
    let mut length = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).ok()?;
        if line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().ok()?;
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some((path, body))
}

fn reply(mut stream: &TcpStream, status: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(body.as_bytes()));
}

/// What the page hears now: the session's changes, then its panes' rows.
fn heard(s: &mut Session, fresh: bool) -> String {
    let mut events = if fresh { s.everything(NOW) } else { s.events(NOW) };
    events.extend(s.take_frames());
    serde_json::to_string(&events).unwrap()
}

/// Answers one request against the session on stage, starting a scene's
/// afresh on `/connect`.
fn handle(path: &str, body: &[u8], on: &mut Option<Session>) -> Result<String, String> {
    match path {
        "/connect" => {
            let asked: Json = serde_json::from_slice(body).map_err(|e| e.to_string())?;
            let name = asked["scene"].as_str().ok_or("no scene named")?;
            let scene = scenes().into_iter().find(|s| s.name == name).ok_or(format!("no scene {name}"))?;
            Ok(heard(on.insert((scene.build)()), true))
        }
        "/call" => {
            let s = on.as_mut().ok_or("no scene connected")?;
            let call: Call = serde_json::from_slice(body).map_err(|e| e.to_string())?;
            let sent = s.call(call);
            answer(s, &sent);
            Ok(heard(s, false))
        }
        _ => Err(format!("no route {path}")),
    }
}

#[test]
#[ignore = "serves scenes to the audit until killed; run it by name"]
fn serve_screens() {
    let port = std::env::var("ARGUS_SCREENS_PORT").unwrap_or_else(|_| "7878".into());
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).unwrap();
    eprintln!("serving scenes on http://127.0.0.1:{port}");
    let mut on = None;
    for stream in listener.incoming().flatten() {
        let Some((path, body)) = read(&stream) else { continue };
        match handle(&path, &body, &mut on) {
            Ok(events) => reply(&stream, "200 OK", &events),
            Err(why) => {
                eprintln!("{path}: {why}");
                reply(&stream, "400 Bad Request", "[]");
            }
        }
    }
}
