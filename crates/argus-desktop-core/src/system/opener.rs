//! Handing something to the system's opener: a link in a view, so it opens
//! in the browser the person chose, and a file or folder of a checkout, so
//! the file manager shows it.
//!
//! The URL is an agent's, read from a ticket, so only an `http` or `https`
//! one is opened: the opener would as happily run a `file:` URL. A path is
//! shown only if it is inside the checkout it is named in. Each is passed as
//! one argument, never through a shell.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Opens `url`, or says why not.
pub fn open(url: &str) -> Result<(), String> {
    if !argus_protocol::is_web_url(url) {
        return Err("only an http or https link is opened".into());
    }
    #[cfg(target_os = "macos")]
    let mut cmd = Command::new("open");
    #[cfg(windows)]
    let mut cmd = {
        let mut cmd = Command::new("rundll32");
        cmd.arg("url.dll,FileProtocolHandler");
        cmd
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = Command::new("xdg-open");
    let mut child = cmd
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not open the link: {e}"))?;
    // `xdg-open` can take a while to hand over; reaped off the caller's
    // thread so it neither blocks it nor stays a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Shows `relative` of the checkout at `root` in the system's file
/// manager — a file selected in its folder, a folder opened — or the
/// checkout's own folder with none, or says why not.
pub fn reveal(root: &Path, relative: Option<&str>) -> Result<(), String> {
    let (target, dir) = revealed(root, relative)?;
    let (program, args) = reveal_command(&target, dir);
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not open the file manager: {e}"))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// The path to show and whether it is a folder, once it is shown to be in
/// the checkout: the text check refuses `..` and absolute paths, and where
/// the path really leads, links followed, must be under the checkout's own.
fn revealed(root: &Path, relative: Option<&str>) -> Result<(PathBuf, bool), String> {
    let Some(relative) = relative.filter(|r| !r.is_empty()) else {
        return Ok((root.to_path_buf(), true));
    };
    if relative.contains('\\') || !argus_protocol::is_checkout_relative(relative) {
        return Err(format!("{relative} is not inside the checkout"));
    }
    let target = relative.split('/').filter(|p| !p.is_empty() && *p != ".").fold(root.to_path_buf(), |p, part| p.join(part));
    let real = target.canonicalize().map_err(|_| format!("{relative} is not on disk"))?;
    let base = root.canonicalize().map_err(|_| "the checkout is not on disk".to_owned())?;
    if !real.starts_with(&base) {
        return Err(format!("{relative} leads out of the checkout"));
    }
    let dir = real.is_dir();
    Ok((target, dir))
}

/// The file manager's program and its one-argument-each command line.
fn reveal_command(target: &Path, dir: bool) -> (&'static str, Vec<OsString>) {
    #[cfg(windows)]
    {
        if dir {
            ("explorer.exe", vec![target.as_os_str().to_owned()])
        } else {
            let mut arg = OsString::from("/select,");
            arg.push(target.as_os_str());
            ("explorer.exe", vec![arg])
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = dir;
        ("open", vec!["-R".into(), target.as_os_str().to_owned()])
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // `xdg-open` opens a folder; a file's own is the nearest to showing it.
        let folder = if dir { target } else { target.parent().unwrap_or(target) };
        ("xdg-open", vec![folder.as_os_str().to_owned()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkout() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src").join("a.rs"), "x").unwrap();
        dir
    }

    #[test]
    fn a_path_in_the_checkout_is_shown_and_none_names_the_checkout_itself() {
        let dir = checkout();
        assert_eq!(revealed(dir.path(), None), Ok((dir.path().to_path_buf(), true)));
        assert_eq!(revealed(dir.path(), Some("src")), Ok((dir.path().join("src"), true)));
        assert_eq!(revealed(dir.path(), Some("src/a.rs")), Ok((dir.path().join("src").join("a.rs"), false)));
    }

    #[test]
    fn a_path_that_leaves_the_checkout_or_is_not_there_is_not_shown() {
        let dir = checkout();
        for path in ["..", "src/../..", "/etc", "src\\..\\..", "nope"] {
            assert!(revealed(dir.path(), Some(path)).is_err(), "{path}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_checkout_is_not_shown() {
        let outside = tempfile::tempdir().unwrap();
        let dir = checkout();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("out")).unwrap();
        assert!(revealed(dir.path(), Some("out")).is_err());
    }

    #[test]
    fn the_file_manager_gets_the_path_as_one_argument() {
        let target = Path::new("work").join("my files").join("a b.rs");
        let (_, args) = reveal_command(&target, false);
        assert!(args.iter().any(|a| a.to_string_lossy().contains("my files")));
        #[cfg(windows)]
        assert_eq!(args.len(), 1, "/select, and the path are one argument");
    }

    #[test]
    fn only_a_web_link_reaches_the_opener() {
        assert_eq!(open("file:///etc/passwd").unwrap_err(), "only an http or https link is opened");
        assert!(open("javascript:alert(1)").is_err());
    }
}
