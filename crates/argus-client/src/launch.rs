//! Getting a connection to the daemon, starting one if nothing is
//! listening. The daemon normally outlives the client that started it, while
//! the explicit server command can replace it through the protocol.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use argus_protocol::{
    read_known_msg, transport, write_msg, ClientMsg, FramingError, Greeting, Hello, Refusal,
    ServerMsg, GREETING_WAIT,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::sleep;

const CONTROL_ACK_TIMEOUT: Duration = Duration::from_secs(5);
const DAEMON_EXIT_TIMEOUT: Duration = Duration::from_secs(10);
const DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub async fn ensure_daemon_and_connect(
) -> anyhow::Result<impl AsyncRead + AsyncWrite + Unpin + Send + 'static> {
    if !transport::is_daemon_listening() {
        spawn_daemon()?;
    }
    let mut last_err = None;
    for attempt in 0..60u64 {
        match transport::connect().await {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                last_err = Some(e);
                sleep(Duration::from_millis(25 * (attempt + 1).min(20))).await;
            }
        }
    }
    Err(anyhow::anyhow!(
        "could not connect to argusd: {:?}",
        last_err
    ))
}

/// A connection to the daemon, and what greeting it found out.
pub struct Connected {
    pub channels: crate::Connection,
    /// The daemon's greeting, or `None` from a daemon that gave none.
    pub daemon: Option<Hello>,
    /// What the daemon sent before its answer, for the app to take first.
    pub opening: Vec<ServerMsg>,
}

/// What this client makes of how its greeting went.
pub(crate) enum Greeted {
    /// The connection is up, whether or not the daemon greeted back.
    Up(Connected),
    /// A daemon from before the handshake hung up on the greeting.
    Refused,
    /// Closed before the daemon said anything.
    Closed,
}

/// Connects to the daemon, starting one if nothing is listening, and
/// greets it.
///
/// A daemon from before the handshake hangs up on the greeting, and is
/// answered by connecting again without one.
pub async fn connect() -> anyhow::Result<Connected> {
    let channels = crate::wire::connection_channels(ensure_daemon_and_connect().await?);
    match greet(channels, GREETING_WAIT).await {
        Greeted::Up(connected) => Ok(connected),
        Greeted::Refused => Ok(Connected {
            channels: crate::wire::connection_channels(ensure_daemon_and_connect().await?),
            daemon: None,
            opening: Vec::new(),
        }),
        Greeted::Closed => anyhow::bail!("argusd closed the connection before saying anything"),
    }
}

/// Greets the daemon on `channels` and reads until it answers, keeping what
/// comes first.
pub(crate) async fn greet(channels: crate::Connection, wait: Duration) -> Greeted {
    let (in_tx, mut out_rx) = channels;
    let _ = in_tx.send(ClientMsg::Hello(Hello::this_build()));

    let (daemon, opening) = match Greeting::read(&mut out_rx, wait).await {
        Greeting::Up { daemon, opening } => (daemon, opening),
        // Carried on with, the way this client always has: the status bar
        // raises the alarm, and the one command that fixes it is still
        // reachable from here.
        Greeting::Refused(Refusal::Protocol { daemon, opening }) => (Some(daemon), opening),
        Greeting::Refused(Refusal::Predates) => return Greeted::Refused,
        Greeting::Closed => return Greeted::Closed,
    };
    Greeted::Up(Connected {
        channels: (in_tx, out_rx),
        daemon,
        opening,
    })
}

pub async fn restart_daemon() -> anyhow::Result<()> {
    if !transport::is_daemon_listening() {
        anyhow::bail!("argusd is not running");
    }

    let mut stream = transport::connect()
        .await
        .context("could not connect to argusd for restart")?;
    write_msg(&mut stream, &ClientMsg::Restart)
        .await
        .context("could not ask argusd to restart")?;

    tokio::time::timeout(CONTROL_ACK_TIMEOUT, async {
        loop {
            let reply = read_known_msg::<_, ServerMsg>(&mut stream)
                .await
                .map_err(|error| control_read_error(error, "restart"))?;
            match reply {
                ServerMsg::Restarting => return Ok(()),
                ServerMsg::Error { message } => {
                    anyhow::bail!("argusd refused to restart: {message}");
                }
                _ => {}
            }
        }
    })
    .await
    .context("timed out waiting for argusd to acknowledge restart")??;

    wait_for_daemon_to_stop("restart").await?;
    let _replacement = ensure_daemon_and_connect().await?;
    Ok(())
}

pub async fn stop_daemon() -> anyhow::Result<()> {
    if !transport::is_daemon_listening() {
        anyhow::bail!("argusd is not running");
    }

    let mut stream = transport::connect()
        .await
        .context("could not connect to argusd for stop")?;
    write_msg(&mut stream, &ClientMsg::Stop)
        .await
        .context("could not ask argusd to stop")?;

    tokio::time::timeout(CONTROL_ACK_TIMEOUT, async {
        loop {
            let reply = read_known_msg::<_, ServerMsg>(&mut stream)
                .await
                .map_err(|error| control_read_error(error, "stop"))?;
            match reply {
                ServerMsg::Stopping => return Ok(()),
                ServerMsg::Error { message } => {
                    anyhow::bail!("argusd refused to stop: {message}");
                }
                _ => {}
            }
        }
    })
    .await
    .context("timed out waiting for argusd to acknowledge stop")??;

    wait_for_daemon_to_stop("stop").await
}

/// Why a control request got no acknowledgement. A daemon older than the
/// client cannot decode a request it has never heard of, and its only answer
/// is to drop the connection — which reads as a bare EOF unless named.
fn control_read_error(error: FramingError, action: &str) -> anyhow::Error {
    let closed = matches!(
        &error,
        FramingError::Io(io) if matches!(
            io.kind(),
            std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionReset
        )
    );
    if !closed {
        return anyhow::Error::new(error).context(format!("argusd did not acknowledge {action}"));
    }
    anyhow::anyhow!(
        "argusd closed the connection instead of acknowledging {action}; it may be older \
         than this client and not know the request. Stop the argusd process (`pkill argusd`, or \
         Task Manager on Windows), then start argus again."
    )
}

/// `argus init [DIR]`: the first-run scan without the TUI. Adds the
/// directory as a project in the open workspace, waits for the tree that
/// holds it, and prints the repositories and checkouts it found.
pub async fn init(dir: Option<String>) -> anyhow::Result<()> {
    let dir = match dir {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir().context("no working directory")?,
    };
    let dir =
        std::fs::canonicalize(&dir).with_context(|| format!("cannot scan {}", dir.display()))?;
    let path = dir.to_string_lossy().into_owned();

    let mut stream = ensure_daemon_and_connect().await?;
    tokio::time::timeout(INIT_TIMEOUT, async {
        // The daemon greets with the tree; the projects already in it are
        // what the added one is told apart from.
        let known = loop {
            if let ServerMsg::Tree(tree) = read_known_msg::<_, ServerMsg>(&mut stream).await? {
                break tree.into_iter().map(|p| p.id).collect::<Vec<_>>();
            }
        };
        // Told apart by id from the projects already known, so no answer
        // is asked for.
        let add = ClientMsg::AddProject {
            path: path.clone(),
            request_id: 0,
        };
        write_msg(&mut stream, &add)
            .await
            .context("could not ask argusd to add the project")?;
        loop {
            match read_known_msg::<_, ServerMsg>(&mut stream).await? {
                ServerMsg::Tree(tree) => {
                    if let Some(project) = tree.into_iter().find(|p| !known.contains(&p.id)) {
                        print!("{}", init_summary(&project, &path));
                        return Ok(());
                    }
                }
                ServerMsg::Error { message } => anyhow::bail!("{message}"),
                _ => {}
            }
        }
    })
    .await
    .context("timed out waiting for argusd to scan the directory")?
}

const INIT_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn init_summary(project: &argus_protocol::ProjectInfo, path: &str) -> String {
    let checkouts: usize = project.repositories.iter().map(|r| r.checkouts.len()).sum();
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut out = format!(
        "added {} ({path}): {}, {}\n",
        project.name,
        plural(project.repositories.len(), "repository", "repositories"),
        plural(checkouts, "checkout", "checkouts"),
    );
    for repository in &project.repositories {
        out.push_str(&format!(
            "  {}  {}\n",
            repository.name,
            plural(repository.checkouts.len(), "checkout", "checkouts")
        ));
    }
    if project.repositories.is_empty() {
        out.push_str("  no Git repositories found under it yet\n");
    }
    out.push_str("run `argus` to open it\n");
    out
}

async fn wait_for_daemon_to_stop(operation: &str) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + DAEMON_EXIT_TIMEOUT;
    while transport::is_daemon_listening() {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("argusd did not stop after acknowledging {operation}");
        }
        sleep(DAEMON_POLL_INTERVAL).await;
    }
    Ok(())
}

fn spawn_daemon() -> anyhow::Result<()> {
    let exe = daemon_exe_path();

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new(&exe);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        cmd.spawn()?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        std::process::Command::new(&exe)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()?;
    }

    Ok(())
}

fn daemon_exe_path() -> PathBuf {
    let name = if cfg!(windows) {
        "argusd.exe"
    } else {
        "argusd"
    };
    if let Ok(mut path) = std::env::current_exe() {
        path.pop();
        path.push(name);
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::read_msg;

    /// A fake daemon on the other end of a greeting: sends its opening
    /// messages, reads the greeting, and then does what `then` says.
    async fn greeting_against<F, Fut>(then: F) -> Greeted
    where
        F: FnOnce(tokio::io::DuplexStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let (client, mut daemon) = tokio::io::duplex(1024 * 1024);
        tokio::spawn(async move {
            write_msg(&mut daemon, &ServerMsg::Tree(Vec::new())).await.unwrap();
            write_msg(&mut daemon, &ServerMsg::Templates(Vec::new()))
                .await
                .unwrap();
            let greeting: ClientMsg = read_msg(&mut daemon).await.unwrap();
            assert!(matches!(greeting, ClientMsg::Hello(_)), "{greeting:?}");
            then(daemon).await;
        });
        greet(
            crate::wire::connection_channels(client),
            Duration::from_millis(200),
        )
        .await
    }

    #[tokio::test]
    async fn a_daemon_that_greets_back_is_named_and_its_opening_kept() {
        let greeting = greeting_against(|mut daemon| async move {
            write_msg(&mut daemon, &ServerMsg::Hello(Hello::this_build()))
                .await
                .unwrap();
            std::future::pending::<()>().await;
        })
        .await;

        let Greeted::Up(connected) = greeting else {
            panic!("the connection should be up");
        };
        assert_eq!(connected.daemon, Some(Hello::this_build()));
        assert!(matches!(
            connected.opening.as_slice(),
            [ServerMsg::Tree(_), ServerMsg::Templates(_)]
        ));
    }

    #[tokio::test]
    async fn a_daemon_that_hangs_up_on_the_greeting_refused_it() {
        // What a daemon from before the handshake does with a message it
        // cannot read.
        let greeting = greeting_against(|daemon| async move { drop(daemon) }).await;
        assert!(matches!(greeting, Greeted::Refused));
    }

    #[tokio::test]
    async fn a_daemon_on_another_protocol_is_carried_on_with_for_the_status_bar_to_name() {
        let other = Hello {
            protocol: argus_protocol::PROTOCOL + 1,
            ..Hello::this_build()
        };
        let sent = other.clone();
        let greeting = greeting_against(|mut daemon| async move {
            write_msg(&mut daemon, &ServerMsg::Hello(sent)).await.unwrap();
            std::future::pending::<()>().await;
        })
        .await;

        let Greeted::Up(connected) = greeting else {
            panic!("the connection should be up");
        };
        assert_eq!(connected.daemon, Some(other));
        assert_eq!(connected.opening.len(), 2, "nothing it sent is lost");
    }

    #[tokio::test]
    async fn a_connection_closed_before_anything_is_not_mistaken_for_a_refusal() {
        let (client, daemon) = tokio::io::duplex(1024);
        drop(daemon);
        let greeting = greet(
            crate::wire::connection_channels(client),
            Duration::from_millis(200),
        )
        .await;
        assert!(matches!(greeting, Greeted::Closed));
    }

    #[test]
    fn a_daemon_that_hangs_up_is_named_as_possibly_older_than_the_client() {
        let eof = std::io::Error::from(std::io::ErrorKind::UnexpectedEof);
        let message = control_read_error(FramingError::Io(eof), "stop").to_string();
        assert!(message.contains("may be older than this client"), "{message}");

        let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let message = format!("{:#}", control_read_error(FramingError::Io(other), "stop"));
        assert!(!message.contains("older"), "{message}");
    }
}
