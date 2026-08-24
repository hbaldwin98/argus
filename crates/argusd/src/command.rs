//! Spawning external commands without flashing a console window.
//!
//! The client starts the daemon with `DETACHED_PROCESS` (see
//! `argus-client`'s `launch`), so on Windows `argusd` owns no console at
//! all. Windows then gives any *console* child it spawns a brand-new
//! console window — which appears on screen and vanishes when the child
//! exits. For a short-lived command like `git`, that is a window flashing
//! in the user's face.
//!
//! `CREATE_NO_WINDOW` suppresses that: the child still gets a console for
//! its stdio handles, it just never gets a visible window. Every external
//! command the daemon runs must go through here.
//!
//! Note what is *not* here: the read-only worktree listing. It used to
//! shell out on a 2-second poll, and the fix for that was to stop spawning
//! a process at all (`git::list_worktrees` now uses libgit2). This module
//! covers what genuinely needs the CLI — the mutating `worktree add`,
//! `worktree remove`, and `branch -D` — all of which are rare and
//! user-initiated.

/// `CREATE_NO_WINDOW` — the console the child gets is not shown.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// An async `git` command that won't flash a console window.
pub fn git() -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("git");
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
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
        // Cheap end-to-end proof the creation flags don't break spawning —
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
