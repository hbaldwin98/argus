//! Getting a connection to the daemon, starting one if nothing is
//! listening. The daemon normally outlives the client that started it, while
//! the explicit server command can replace it through the protocol.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use argus_protocol::{read_msg, transport, write_msg, ClientMsg, ServerMsg};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::sleep;

const RESTART_ACK_TIMEOUT: Duration = Duration::from_secs(5);
const RESTART_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const RESTART_POLL_INTERVAL: Duration = Duration::from_millis(25);

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

    tokio::time::timeout(RESTART_ACK_TIMEOUT, async {
        loop {
            match read_msg::<_, ServerMsg>(&mut stream).await? {
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

    wait_for_daemon_to_stop().await?;
    let _replacement = ensure_daemon_and_connect().await?;
    Ok(())
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
            if let ServerMsg::Tree(tree) = read_msg::<_, ServerMsg>(&mut stream).await? {
                break tree.into_iter().map(|p| p.id).collect::<Vec<_>>();
            }
        };
        write_msg(&mut stream, &ClientMsg::AddProject { path: path.clone() })
            .await
            .context("could not ask argusd to add the project")?;
        loop {
            match read_msg::<_, ServerMsg>(&mut stream).await? {
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

async fn wait_for_daemon_to_stop() -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + RESTART_STOP_TIMEOUT;
    while transport::is_daemon_listening() {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("argusd did not stop after acknowledging restart");
        }
        sleep(RESTART_POLL_INTERVAL).await;
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
