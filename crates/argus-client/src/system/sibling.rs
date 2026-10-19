//! Starting another of Argus's executables — the daemon, the desktop
//! window — so it outlives this process: found beside this one, and
//! detached from the terminal that ran it.

use std::path::{Path, PathBuf};

/// `name` beside this executable when it is there, so an unpacked release
/// or a `target/` directory runs its own build; otherwise the bare name,
/// for `PATH` to find.
pub fn path(name: &str) -> PathBuf {
    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    if let Ok(mut path) = std::env::current_exe() {
        path.pop();
        path.push(&file);
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(file)
}

/// Starts `exe` in its own session with no stdio, so closing the terminal
/// or this process leaves it running.
pub fn spawn_detached(exe: &Path) -> std::io::Result<()> {
    detach(|| std::process::Command::new(exe))
}

fn detach(command: impl Fn() -> std::process::Command) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mut cmd = command();
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
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        const ERROR_ACCESS_DENIED: i32 = 5;
        keep_own_stdio();
        let spawn = |flags: u32| {
            command()
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | flags)
                .spawn()
        };
        // Out of an Argus shell pane's job, which ends everything left in
        // it when the pane closes. A job that allows no breakaway (an agent
        // pane's) refuses the flag outright, and there the process stays in
        // the job, as it would have anyway.
        match spawn(CREATE_BREAKAWAY_FROM_JOB) {
            Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) => {
                spawn(0)?;
            }
            result => {
                result?;
            }
        }
    }

    Ok(())
}

/// Keeps this process's stdin, stdout and stderr out of every process it
/// starts from here on. Windows hands a child every inheritable handle, not
/// only the three it is given, and a parent made these inheritable to give
/// them to us: a daemon started by `argus bridge` held the desktop's pipes
/// to the bridge for its whole life, so the desktop never saw the bridge
/// end. A child that should share them still can: `Stdio::inherit` gives
/// it a duplicate made for it.
#[cfg(windows)]
fn keep_own_stdio() {
    use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // A window started without them has none, and fails here harmlessly.
        unsafe {
            SetHandleInformation(GetStdHandle(which), HANDLE_FLAG_INHERIT, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_executable_not_beside_this_one_is_left_for_path_to_find() {
        let found = path("argus-no-such-sibling");
        let expected = format!("argus-no-such-sibling{}", std::env::consts::EXE_SUFFIX);
        assert_eq!(found, PathBuf::from(expected));
    }

    #[test]
    fn an_executable_beside_this_one_is_found_there() {
        let this = std::env::current_exe().unwrap();
        let name = this.file_stem().unwrap().to_str().unwrap();
        assert_eq!(path(name), this);
    }

    /// Which part a run of this test binary plays in the next test; unset,
    /// the two ignored helpers below do nothing.
    #[cfg(windows)]
    const ROLE: &str = "ARGUS_SIBLING_TEST_ROLE";

    /// This test binary run as one ignored test, playing `role`.
    #[cfg(windows)]
    fn helper(name: &str, role: &str) -> std::process::Command {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", &format!("system::sibling::tests::{name}"), "--ignored"]).env(ROLE, role);
        command
    }

    // The starter plays `argus bridge`: its stdout is a pipe to this test,
    // and the process it detaches plays the daemon, outliving it.
    #[cfg(windows)]
    #[test]
    fn a_detached_process_holds_none_of_its_starters_pipes() {
        use std::io::Read;
        use std::process::Stdio;
        use std::time::Duration;

        let mut starter = helper("starter", "starter")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdout = starter.stdout.take().unwrap();
        let (ended, ending) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = stdout.read_to_end(&mut Vec::new());
            let _ = ended.send(());
        });
        assert!(starter.wait().unwrap().success());
        assert!(
            ending.recv_timeout(Duration::from_secs(10)).is_ok(),
            "the starter has exited, and its stdout is still held open by the process it detached"
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "run by a_detached_process_holds_none_of_its_starters_pipes"]
    fn starter() {
        if std::env::var(ROLE).as_deref() == Ok("starter") {
            detach(|| helper("lingerer", "lingerer")).unwrap();
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "run by a_detached_process_holds_none_of_its_starters_pipes"]
    fn lingerer() {
        if std::env::var(ROLE).as_deref() == Ok("lingerer") {
            std::thread::sleep(std::time::Duration::from_secs(20));
        }
    }
}
