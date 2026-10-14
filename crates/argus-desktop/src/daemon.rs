//! Reaching this machine's daemon through `argus bridge`: starting the
//! bridge, greeting, and connecting again when it goes.
//!
//! The window never opens the daemon's socket and never starts `argusd`
//! itself. It runs `argus bridge` and speaks the protocol over the child's
//! stdin and stdout, as `argus --host` does over ssh, so connect-or-start
//! stays in one place: the terminal client's `launch`.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use argus_protocol::{
    read_known_msg, write_msg, ClientMsg, Greeting, Hello, Refusal, ServerMsg, GREETING_WAIT,
    PROTOCOL,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::shared::Shared;

/// The gap between failed rounds. Each round already waits on `argus
/// bridge`, which spends half a minute trying to start a daemon.
const ROUND_GAP: Duration = Duration::from_secs(2);

/// How much of the bridge's stderr is kept: the end of it, where its
/// error is.
const SAID_LIMIT: usize = 16 * 1024;

/// How long a bridge that closed on us is given to exit, for its code.
const EXIT_WAIT: Duration = Duration::from_secs(2);

/// Keeps the window connected for as long as it is open.
pub async fn run(shared: Arc<Shared>) {
    let mut greet = true;
    loop {
        match connect_once(&shared, greet).await {
            // A daemon from before the greeting hung up on it: it is
            // reached again at once, without one.
            Ended::Predates => {
                greet = false;
                continue;
            }
            Ended::Failed(words) => {
                tracing::warn!("argus bridge: {words}");
                shared.act(|s| {
                    s.connecting(Some(words));
                    Vec::new()
                });
            }
            // Someone ran `argus server stop`. A client never starts again
            // a daemon the person stopped; the page's Reconnect does.
            Ended::Closed { stopped: true } => {
                tracing::info!("argusd was stopped; waiting for Reconnect");
                shared.act(|s| {
                    s.stopped();
                    Vec::new()
                });
                shared.reconnect_asked().await;
                shared.act(|s| {
                    s.connecting(None);
                    Vec::new()
                });
                greet = true;
                continue;
            }
            Ended::Closed { stopped: false } => {
                tracing::info!("the connection to argusd closed; connecting again");
                shared.act(|s| {
                    s.connecting(None);
                    Vec::new()
                });
            }
        }
        greet = true;
        tokio::time::sleep(ROUND_GAP).await;
    }
}

enum Ended {
    Predates,
    /// The bridge failed or closed before the daemon said a word; what it
    /// printed says why.
    Failed(String),
    Closed { stopped: bool },
}

async fn connect_once(shared: &Shared, greet: bool) -> Ended {
    let exe = argus_exe();
    let mut bridge = match Bridge::start(&exe) {
        Ok(bridge) => bridge,
        Err(err) => {
            return Ended::Failed(format!("could not run {} bridge: {err}", exe.display()));
        }
    };
    let (Some(stdout), Some(stdin)) = (bridge.child.stdout.take(), bridge.child.stdin.take()) else {
        return Ended::Failed("argus bridge started without its stdin and stdout".into());
    };

    let (mut link, greeting) = open(stdout, stdin, greet).await;
    let (daemon, alarm, opening) = match greeting {
        Greeting::Up { daemon, opening } => (daemon, None, opening),
        // Carried on with, as the terminal client does: the status bar
        // raises the alarm, and `argus server restart` fixes it.
        Greeting::Refused(Refusal::Protocol { daemon, opening }) => {
            let alarm = format!(
                "argusd speaks protocol {}, this window {PROTOCOL}: `argus server restart` brings them together",
                daemon.protocol
            );
            (Some(daemon), Some(alarm), opening)
        }
        Greeting::Refused(Refusal::Predates) => return Ended::Predates,
        Greeting::Closed => return Ended::Failed(bridge.last_words().await),
    };

    tracing::info!(daemon = ?daemon.as_ref().map(|d| &d.version), "connected to argusd");
    shared.connected(link.outgoing.clone(), daemon, alarm, opening);
    let mut stopped = false;
    while let Some(msg) = link.incoming.recv().await {
        stopped |= matches!(msg, ServerMsg::Stopping);
        shared.act(|s| s.receive(msg));
    }
    shared.disconnected();
    Ended::Closed { stopped }
}

/// `argus` beside this executable, as an unpacked release or a `target/`
/// directory has it, else whichever `PATH` finds.
fn argus_exe() -> PathBuf {
    let name = format!("argus{}", std::env::consts::EXE_SUFFIX);
    if let Ok(mut path) = std::env::current_exe() {
        path.pop();
        path.push(&name);
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(name)
}

struct Bridge {
    child: Child,
    /// The end of what the bridge printed on stderr: the only account of a
    /// daemon that could not be reached.
    said: Arc<Mutex<Vec<u8>>>,
}

impl Bridge {
    fn start(exe: &std::path::Path) -> std::io::Result<Bridge> {
        let mut command = Command::new(exe);
        command
            .arg("bridge")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // `argus` is a console program; started from a window it would
        // flash a console of its own without this (CREATE_NO_WINDOW).
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command.spawn()?;

        let said = Arc::new(Mutex::new(Vec::new()));
        if let Some(mut stderr) = child.stderr.take() {
            let said = said.clone();
            tokio::spawn(async move {
                let mut chunk = [0u8; 4096];
                while let Ok(n @ 1..) = stderr.read(&mut chunk).await {
                    let Ok(mut said) = said.lock() else {
                        return;
                    };
                    said.extend_from_slice(&chunk[..n]);
                    let over = said.len().saturating_sub(SAID_LIMIT);
                    said.drain(..over);
                }
            });
        }
        Ok(Bridge { child, said })
    }

    /// What the bridge said as it ended, with its exit code.
    async fn last_words(&mut self) -> String {
        let code = match tokio::time::timeout(EXIT_WAIT, self.child.wait()).await {
            Ok(Ok(status)) => status.code(),
            _ => None,
        };
        // The stderr reader may still be draining what came before exit.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let said = self.said.lock().map(|s| s.clone()).unwrap_or_default();
        last_words(&said, code)
    }
}

/// The last lines a bridge printed, which is where its error is, or what
/// little can be said without them.
fn last_words(said: &[u8], code: Option<i32>) -> String {
    let text = String::from_utf8_lossy(said);
    let lines: Vec<&str> = text.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    if !lines.is_empty() {
        return lines[lines.len().saturating_sub(3)..].join("\n");
    }
    match code {
        Some(code) => format!("argus bridge exited with code {code} before argusd said anything"),
        None => "argus bridge ended before argusd said anything".into(),
    }
}

/// The two directions of one connection.
struct Link {
    outgoing: mpsc::UnboundedSender<ClientMsg>,
    incoming: mpsc::Receiver<ServerMsg>,
}

/// Starts carrying messages over `read` and `write`, greets when `greet`,
/// and reads how the greeting went. Without a greeting the daemon is taken
/// as up and silent about itself.
async fn open<R, W>(read: R, write: W, greet: bool) -> (Link, Greeting)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    // Reading a frame is not cancellation-safe, so it has its own task.
    let (in_tx, mut incoming) = mpsc::channel(256);
    tokio::spawn(async move {
        let mut read = read;
        while let Ok(msg) = read_known_msg::<_, ServerMsg>(&mut read).await {
            if in_tx.send(msg).await.is_err() {
                return;
            }
        }
    });
    let (outgoing, mut out_rx) = mpsc::unbounded_channel::<ClientMsg>();
    tokio::spawn(async move {
        let mut write = write;
        while let Some(msg) = out_rx.recv().await {
            if write_msg(&mut write, &msg).await.is_err() {
                return;
            }
        }
    });

    if !greet {
        let greeting = Greeting::Up { daemon: None, opening: Vec::new() };
        return (Link { outgoing, incoming }, greeting);
    }
    let _ = outgoing.send(ClientMsg::Hello(Hello::this_build()));
    let greeting = Greeting::read(&mut incoming, GREETING_WAIT).await;
    (Link { outgoing, incoming }, greeting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::read_msg;
    use tokio::io::duplex;

    #[tokio::test]
    async fn a_greeted_daemon_is_up_with_its_opening_kept() {
        let (window, daemon) = duplex(64 * 1024);
        let (from_daemon, to_daemon) = tokio::io::split(window);
        let (mut daemon_read, mut daemon_write) = tokio::io::split(daemon);
        tokio::spawn(async move {
            write_msg(&mut daemon_write, &ServerMsg::Tree(Vec::new())).await.unwrap();
            let hello: ClientMsg = read_msg(&mut daemon_read).await.unwrap();
            assert!(matches!(hello, ClientMsg::Hello(_)), "the window greets first");
            write_msg(&mut daemon_write, &ServerMsg::Hello(Hello::this_build())).await.unwrap();
            std::future::pending::<()>().await;
        });

        let (_link, greeting) = open(from_daemon, to_daemon, true).await;
        let Greeting::Up { daemon, opening } = greeting else {
            panic!("up: {greeting:?}");
        };
        assert_eq!(daemon, Some(Hello::this_build()));
        assert!(matches!(opening.as_slice(), [ServerMsg::Tree(_)]));
    }

    #[tokio::test]
    async fn a_daemon_that_predates_the_greeting_is_reached_without_one() {
        let (window, daemon) = duplex(64 * 1024);
        let (from_daemon, to_daemon) = tokio::io::split(window);
        let (mut daemon_read, _daemon_write) = tokio::io::split(daemon);

        let (link, greeting) = open(from_daemon, to_daemon, false).await;
        assert!(matches!(greeting, Greeting::Up { daemon: None, .. }));
        link.outgoing.send(ClientMsg::Unsubscribe { pane: argus_protocol::PaneId(1) }).unwrap();
        let first: ClientMsg = read_msg(&mut daemon_read).await.unwrap();
        assert!(matches!(first, ClientMsg::Unsubscribe { .. }), "no greeting went first: {first:?}");
    }

    #[tokio::test]
    async fn a_connection_closed_before_a_word_is_closed() {
        let (window, daemon) = duplex(1024);
        drop(daemon);
        let (from_daemon, to_daemon) = tokio::io::split(window);
        let (_link, greeting) = open(from_daemon, to_daemon, true).await;
        assert!(matches!(greeting, Greeting::Closed), "{greeting:?}");
    }

    #[test]
    fn a_bridge_that_failed_is_reported_in_its_own_last_lines() {
        let said = b"starting\n\nerror: could not start argusd\ncaused by: not found\n";
        assert_eq!(last_words(said, Some(1)), "starting\nerror: could not start argusd\ncaused by: not found");
        let long = b"1\n2\n3\n4\n5\n";
        assert_eq!(last_words(long, None), "3\n4\n5");
    }

    #[test]
    fn a_silent_bridge_is_reported_by_its_exit_code() {
        assert_eq!(last_words(b"", Some(127)), "argus bridge exited with code 127 before argusd said anything");
        assert_eq!(last_words(b"  \n", None), "argus bridge ended before argusd said anything");
    }
}
