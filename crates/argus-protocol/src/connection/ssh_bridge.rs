//! How a client runs `argus bridge` on another machine through ssh: the
//! arguments, and what to tell the operator when ssh could not get there.
//!
//! Here rather than in a client because the terminal client and the desktop
//! window both reach a remote daemon this way, and two copies of the batch
//! mode, the keepalives and the failure wording would drift apart. Spawning
//! stays with each client, which owns its own pipes and window flags.

/// ssh's arguments for running the bridge on `host`.
///
/// Keepalives, so a link that dies quietly is noticed within a minute
/// rather than whenever the next keystroke fails to arrive.
pub fn ssh_args(host: &str) -> Vec<String> {
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
pub fn ssh_failure(host: &str, stderr: &str, code: Option<i32>) -> String {
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
    // the whole answer: connecting never installs argus there, since a
    // client that drops binaries on another host looks like malware and
    // installing software is the user's act.
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
}
