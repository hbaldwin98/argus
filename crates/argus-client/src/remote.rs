//! Reaching a daemon on another machine: ssh runs `argus bridge` there, and
//! its stdin and stdout are the connection.
//!
//! Keys or ssh-agent only, so ssh runs in batch mode: the terminal belongs
//! to the client, and ssh has nowhere to ask for a password or a host key.
//! When it cannot log in, what it printed is turned into what to do about
//! it.

use std::ffi::OsStr;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::task::JoinHandle;

use argus_protocol::GREETING_WAIT;

use crate::launch::{greet, Connected, Greeted};

/// How much of what ssh prints is kept for explaining a failure. Enough
/// for its last lines, which are the ones that say what went wrong.
const STDERR_KEPT: usize = 16 * 1024;

/// How long a failed ssh gets to finish saying why after its connection
/// closes.
const EXIT_WAIT: Duration = Duration::from_secs(5);

/// Connects to the daemon on `host` over ssh and greets it.
pub async fn connect(host: &str) -> anyhow::Result<Connected> {
    connect_with(OsStr::new("ssh"), host).await
}

async fn connect_with(ssh: &OsStr, host: &str) -> anyhow::Result<Connected> {
    let session = Session::start(ssh, host)?;
    match greet(session.channels, GREETING_WAIT).await {
        Greeted::Up(connected) => Ok(connected),
        // Its daemon predates the greeting and hung up on it: again,
        // without one.
        Greeted::Refused => Ok(Connected {
            channels: Session::start(ssh, host)?.channels,
            daemon: None,
            opening: Vec::new(),
        }),
        Greeted::Closed => {
            let (stderr, code) = tokio::time::timeout(EXIT_WAIT, session.exit)
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            anyhow::bail!("{}", ssh_failure(host, &stderr, code))
        }
    }
}

/// One ssh process carrying a connection.
struct Session {
    channels: crate::Connection,
    /// What ssh printed and the code it left with, once it has. The task
    /// also owns the process, which is killed if the client goes first.
    exit: JoinHandle<(String, Option<i32>)>,
}

impl Session {
    fn start(ssh: &OsStr, host: &str) -> anyhow::Result<Session> {
        let mut child = Command::new(ssh)
            .args(ssh_args(host))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| anyhow::anyhow!("could not run ssh: {error}"))?;
        let (Some(stdout), Some(stdin), Some(mut stderr)) =
            (child.stdout.take(), child.stdin.take(), child.stderr.take())
        else {
            anyhow::bail!("ssh started without its pipes");
        };

        let channels = crate::wire::connection_channels(tokio::io::join(stdout, stdin));
        let exit = tokio::spawn(async move {
            let mut said = Vec::new();
            let mut chunk = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut chunk).await {
                if n == 0 {
                    break;
                }
                said.extend_from_slice(&chunk[..n]);
                if said.len() > STDERR_KEPT {
                    said.drain(..said.len() - STDERR_KEPT);
                }
            }
            let code = child.wait().await.ok().and_then(|status| status.code());
            (String::from_utf8_lossy(&said).into_owned(), code)
        });
        Ok(Session { channels, exit })
    }
}

/// ssh's arguments for running the bridge on `host`.
///
/// Keepalives, so a link that dies quietly is noticed within a minute
/// rather than whenever the next keystroke fails to arrive.
fn ssh_args(host: &str) -> Vec<String> {
    [
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
        "--",
        host,
        "argus",
        "bridge",
    ]
    .iter()
    .map(|arg| arg.to_string())
    .collect()
}

/// What to tell the operator when ssh to `host` ended before the daemon
/// said anything, from what ssh printed and the code it left with.
fn ssh_failure(host: &str, stderr: &str, code: Option<i32>) -> String {
    let said = stderr.to_lowercase();
    if said.contains("permission denied") {
        return format!(
            "ssh could not log in to {host}: add your key to ssh-agent or to {host}'s \
             authorized_keys"
        );
    }
    if said.contains("host key verification failed") || said.contains("identification has changed")
    {
        return format!(
            "ssh does not trust {host}'s host key yet: connect once with `ssh {host}` in a terminal"
        );
    }
    if said.contains("could not resolve hostname") {
        return format!("ssh cannot find {host}");
    }
    // The remote shell's code for a command it could not find. Saying so is
    // the whole answer: connecting never installs argus there (DESIGN.md,
    // "Process model").
    if code == Some(127) || said.contains("command not found") || said.contains("argus: not found")
    {
        return format!("argus is not installed on {host}, or not on the PATH ssh sessions get");
    }
    match stderr.lines().rev().find(|line| !line.trim().is_empty()) {
        Some(line) => format!("ssh {host}: {}", line.trim()),
        None => match code {
            Some(code) => format!("ssh {host} ended with code {code}"),
            None => format!("ssh {host} ended"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_login_names_where_the_key_goes() {
        let message = ssh_failure(
            "devbox",
            "user@devbox: Permission denied (publickey,password).\n",
            Some(255),
        );
        assert!(message.contains("ssh-agent"), "{message}");
        assert!(message.contains("devbox"), "{message}");
    }

    #[test]
    fn an_unknown_host_key_says_to_accept_it_in_a_terminal() {
        let message = ssh_failure("devbox", "Host key verification failed.\n", Some(255));
        assert!(message.contains("`ssh devbox`"), "{message}");
    }

    #[test]
    fn a_missing_argus_is_named_whatever_the_remote_shell_calls_it() {
        for stderr in [
            "bash: line 1: argus: command not found\n",
            "zsh:1: command not found: argus\n",
            "sh: 1: argus: not found\n",
        ] {
            let message = ssh_failure("devbox", stderr, Some(127));
            assert!(message.contains("not installed"), "{stderr}: {message}");
        }
    }

    #[test]
    fn anything_else_is_ssh_s_own_last_word() {
        let message = ssh_failure(
            "devbox",
            "debug1: whatever\nssh: connect to host devbox port 22: Connection refused\n\n",
            Some(255),
        );
        assert_eq!(
            message,
            "ssh devbox: ssh: connect to host devbox port 22: Connection refused"
        );
        assert_eq!(ssh_failure("devbox", "", Some(3)), "ssh devbox ended with code 3");
    }

    #[test]
    fn ssh_runs_the_bridge_in_batch_mode_and_the_host_cannot_pass_for_an_option() {
        let args = ssh_args("-oProxyCommand=evil");
        assert!(args.contains(&"BatchMode=yes".to_string()));
        let separator = args.iter().position(|arg| arg == "--").unwrap();
        assert_eq!(args[separator + 1], "-oProxyCommand=evil");
        assert_eq!(args[separator + 2..], ["argus", "bridge"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_failed_ssh_is_reported_in_its_own_words() {
        // A stand-in for ssh that fails the way a refused login does.
        let fake = std::env::temp_dir().join(format!("argus-fake-ssh-{}", std::process::id()));
        std::fs::write(
            &fake,
            "#!/bin/sh\necho 'me@devbox: Permission denied (publickey).' >&2\nexit 255\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let error = connect_with(fake.as_os_str(), "devbox")
            .await
            .err()
            .expect("the login should fail");
        let _ = std::fs::remove_file(&fake);
        assert!(error.to_string().contains("ssh-agent"), "{error}");
    }
}
