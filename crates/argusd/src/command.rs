//! Spawning external commands without flashing a console window.
//!
//! The daemon is started detached and so owns no console. Windows gives
//! any console child of such a process a console *window* of its own,
//! which appears and vanishes on every `git` invocation; `CREATE_NO_WINDOW`
//! suppresses the window while still giving the child its stdio handles.
//! Every external command the daemon runs must go through here.

/// `CREATE_NO_WINDOW` — the console the child gets is not shown.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn git() -> tokio::process::Command {
    quiet("git")
}

/// Any other command the daemon waits on — a project's worktree setup.
pub fn quiet(program: &str) -> tokio::process::Command {
    let cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    let mut cmd = cmd;
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// A process that outlives the daemon and owns whatever window it makes:
/// a GUI editor the user asked for.
pub fn detached(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(DETACHED_PROCESS);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builder_runs_git() {
        assert_eq!(git().as_std().get_program(), "git");
    }

    #[tokio::test]
    async fn the_async_builder_produces_a_working_command() {
        // `CREATE_NO_WINDOW` must suppress the window, not the process.
        let out = git().arg("--version").output().await.unwrap();
        assert!(out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("git version"),
            "unexpected output: {:?}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}
