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
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new(exe);
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
        std::process::Command::new(exe)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()?;
    }

    Ok(())
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
}
