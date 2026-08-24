//! The command Argus's managed agent hooks actually run.
//!
//! Usage: `argus-hook <url> <bearer-token>`
//!
//! It POSTs to the daemon's loopback status receiver and **always exits 0**,
//! whatever happens. That is the entire reason it exists instead of a `curl`
//! invocation: hooks are written into a checkout's agent config, and a hook
//! command that exits non-zero is reported to the user as a failed turn. A
//! daemon that has since exited — or a port that now belongs to nobody —
//! must degrade to "pane status stops updating", never to an error on every
//! prompt in that directory. `curl` exits 7 on a refused connection, which
//! is exactly the failure mode this avoids.
//!
//! It also writes nothing to stdout. Some agent CLIs inject a hook's stdout
//! into the model's context on success, so staying silent keeps Argus's
//! bookkeeping out of the conversation.
//!
//! On Windows it is a GUI-subsystem binary. Not because it has a UI — it
//! has none — but because the agent CLI that runs it decides how it is
//! spawned, and we cannot ask that CLI to pass `CREATE_NO_WINDOW`. A
//! console-subsystem binary spawned from a process without a console gets
//! its own console *window*, which flashes on screen on every hook event.
//! Declaring the GUI subsystem means no console is ever allocated. Safe
//! precisely because this program reads and writes nothing on stdio.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::Write;
use std::net::TcpStream;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(2);

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(url), Some(token)) = (args.next(), args.next()) else {
        return;
    };
    let _ = post(&url, &token);
}

/// Best-effort POST. Every error is discarded by the caller; the return type
/// exists only so the body can use `?`.
fn post(url: &str, token: &str) -> Option<()> {
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
         Content-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).ok()?;
    // The daemon's reply is deliberately not read: nothing here acts on it,
    // and not waiting keeps the agent's turn from stalling on a slow answer.
    Some(())
}
