//! Reaching a daemon through `argus bridge`: starting the bridge, here or
//! on another machine over ssh, greeting, and connecting again when it goes.
//!
//! The window never opens the daemon's socket and never starts `argusd`
//! itself. It runs `argus bridge` and speaks the protocol over the child's
//! stdin and stdout, as `argus --host` does over ssh, so connect-or-start
//! stays in one place: the terminal client's `connection/launch`. A remote
//! host is the same child with ssh in front, and ssh's own words are turned
//! into what to do about them by the protocol's `ssh_bridge`, as the
//! terminal client's.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use argus_protocol::connection::ssh_bridge::{ssh_args, ssh_failure};
use argus_protocol::{
    read_known_msg, write_msg, ClientMsg, Greeting, Hello, Refusal, ServerMsg, GREETING_WAIT,
    PROTOCOL,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::connection::shared::Shared;

/// The gap between failed rounds. Each round already waits on `argus
/// bridge`, which spends half a minute trying to start a daemon.
const ROUND_GAP: Duration = Duration::from_secs(2);

/// How much of the bridge's stderr is kept: the end of it, where its
/// error is.
const SAID_LIMIT: usize = 16 * 1024;

/// How long a bridge that closed on us is given to exit, for its code.
const EXIT_WAIT: Duration = Duration::from_secs(2);

/// Which daemon the window reaches, and by what program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// This machine's, through the `argus` at this path.
    Local(PathBuf),
    /// The one on `host`, through `ssh`. The program is a field so a test
    /// can stand a script in for ssh.
    Ssh { ssh: PathBuf, host: String },
}

impl Target {
    /// This machine's daemon, or `host`'s when one is named.
    pub fn for_host(host: Option<&str>) -> Target {
        match host {
            Some(host) => Target::Ssh { ssh: PathBuf::from("ssh"), host: host.to_owned() },
            None => Target::Local(argus_exe()),
        }
    }

    fn command(&self) -> Command {
        match self {
            Target::Local(exe) => {
                let mut command = Command::new(exe);
                command.arg("bridge");
                command
            }
            Target::Ssh { ssh, host } => {
                let mut command = Command::new(ssh);
                command.args(ssh_args(host));
                command
            }
        }
    }

    /// What was being run, for the error of failing to run it.
    fn describe(&self) -> String {
        match self {
            Target::Local(exe) => format!("{} bridge", exe.display()),
            Target::Ssh { ssh, host } => format!("{} {host}", ssh.display()),
        }
    }
}

/// Keeps the window connected for as long as it is open, to whichever host
/// it is switched to: the bridge of the last is dropped, which kills it,
/// and a connection loop starts for the next.
pub async fn run(shared: Arc<Shared>) {
    let mut host = shared.watch_host();
    loop {
        let target = Target::for_host(host.borrow_and_update().as_deref());
        let keeping = tokio::spawn(keep(shared.clone(), target));
        if host.changed().await.is_err() {
            return;
        }
        keeping.abort();
        // The aborted loop never reached its own `disconnected`.
        shared.disconnected();
    }
}

async fn keep(shared: Arc<Shared>, target: Target) {
    let mut greet = true;
    loop {
        match connect_once(&shared, &target, greet).await {
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

async fn connect_once(shared: &Shared, target: &Target, greet: bool) -> Ended {
    let mut bridge = match Bridge::start(target) {
        Ok(bridge) => bridge,
        Err(err) => {
            return Ended::Failed(format!("could not run {}: {err}", target.describe()));
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
        if !crate::code::edit::is_edit(&msg) {
            shared.act(|s| s.receive(msg));
        } else if let Some(event) = crate::code::edit::event(msg) {
            shared.tell_page(crate::connection::page::Event::Edit(event));
        }
    }
    shared.disconnected();
    Ended::Closed { stopped }
}

/// `argus` beside this executable: the bundle's sidecar in a `.app` or an
/// install, or the build beside it in a `target/` directory. A GUI launch
/// on macOS or Windows usually has no shell `PATH`, so the bundle has to be
/// found first.
fn argus_exe() -> PathBuf {
    let dirs: Vec<PathBuf> = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
        .into_iter()
        .collect();
    let file = format!("argus{}", std::env::consts::EXE_SUFFIX);
    locate(&file, &dirs, |path| path.exists())
}

/// `file` in the first of `dirs` that holds it, else the bare name for
/// `PATH` to find. `PATH` is the last resort, so a bundled sidecar always
/// wins over whatever an older install left on the search path.
fn locate(file: &str, dirs: &[PathBuf], exists: impl Fn(&Path) -> bool) -> PathBuf {
    dirs.iter()
        .map(|dir| dir.join(file))
        .find(|path| exists(path))
        .unwrap_or_else(|| PathBuf::from(file))
}

struct Bridge {
    host: Option<String>,
    child: Child,
    /// The end of what the bridge printed on stderr: the only account of a
    /// daemon that could not be reached.
    said: Arc<Mutex<Vec<u8>>>,
}

impl Bridge {
    fn start(target: &Target) -> std::io::Result<Bridge> {
        let mut command = target.command();
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // `argus` and ssh are console programs; started from a window it would
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
        let host = match target {
            Target::Ssh { host, .. } => Some(host.clone()),
            Target::Local(_) => None,
        };
        Ok(Bridge { host, child, said })
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
        match &self.host {
            Some(host) => ssh_failure(host, &String::from_utf8_lossy(&said), code),
            None => last_words(&said, code),
        }
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
    use std::collections::HashSet;
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

    #[cfg(unix)]
    #[tokio::test]
    async fn a_failed_ssh_is_reported_in_its_own_words() {
        // A stand-in for ssh that fails the way a refused login does.
        let fake = std::env::temp_dir().join(format!("argus-desktop-fake-ssh-{}", std::process::id()));
        std::fs::write(&fake, "#!/bin/sh\necho 'me@devbox: Permission denied (publickey).' >&2\nexit 255\n")
            .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let target = Target::Ssh { ssh: fake.clone(), host: "devbox".into() };
        let mut bridge = Bridge::start(&target).unwrap();
        let (stdout, stdin) = (bridge.child.stdout.take().unwrap(), bridge.child.stdin.take().unwrap());
        let (_link, greeting) = open(stdout, stdin, true).await;
        assert!(matches!(greeting, Greeting::Closed), "{greeting:?}");
        let words = bridge.last_words().await;
        let _ = std::fs::remove_file(&fake);
        assert!(words.contains("ssh-agent") && words.contains("devbox"), "{words}");
    }

    #[test]
    fn a_host_is_reached_through_ssh_and_none_through_argus() {
        let Target::Ssh { host, .. } = Target::for_host(Some("devbox")) else {
            panic!("a host goes through ssh");
        };
        assert_eq!(host, "devbox");
        assert!(matches!(Target::for_host(None), Target::Local(_)));
    }

    #[test]
    fn a_silent_bridge_is_reported_by_its_exit_code() {
        assert_eq!(last_words(b"", Some(127)), "argus bridge exited with code 127 before argusd said anything");
        assert_eq!(last_words(b"  \n", None), "argus bridge ended before argusd said anything");
    }

    // A set of paths the lookup may find, standing in for the filesystem.
    fn present(paths: &[&str]) -> impl Fn(&Path) -> bool {
        let set: HashSet<PathBuf> = paths.iter().map(PathBuf::from).collect();
        move |path| set.contains(path)
    }

    #[test]
    fn the_bundle_beside_the_desktop_wins_over_path() {
        let dirs = [PathBuf::from("/app/Contents/MacOS")];
        let found = locate("argus", &dirs, present(&["/app/Contents/MacOS/argus"]));
        assert_eq!(found, PathBuf::from("/app/Contents/MacOS/argus"));
    }

    #[test]
    fn path_is_the_last_resort_when_no_directory_holds_argus() {
        let dirs = [PathBuf::from("/app/Contents/MacOS")];
        let found = locate("argus", &dirs, present(&[]));
        assert_eq!(found, PathBuf::from("argus"));
    }

    #[test]
    fn the_first_directory_that_holds_argus_wins() {
        let dirs = [PathBuf::from("/first"), PathBuf::from("/second")];
        let found = locate("argus", &dirs, present(&["/first/argus", "/second/argus"]));
        assert_eq!(found, PathBuf::from("/first/argus"));
    }

    #[test]
    fn a_later_directory_is_used_when_the_first_lacks_argus() {
        let dirs = [PathBuf::from("/first"), PathBuf::from("/second")];
        let found = locate("argus", &dirs, present(&["/second/argus"]));
        assert_eq!(found, PathBuf::from("/second/argus"));
    }
}
