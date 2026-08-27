use std::path::PathBuf;
use std::time::Duration;

use argus_protocol::transport;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::sleep;

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
