//! Where a file pasted into a pane is kept on the pane's host, and how its
//! path is spelled into the pane.
//!
//! A client's clipboard and files are on the client's machine; a pane's
//! process reads the daemon's. A screenshot pasted over ssh has to cross,
//! and the one form every harness takes an image in is a path it can open
//! — the same thing a terminal types when a file is dropped on it. So the
//! bytes land under Argus's own config directory, never the checkout, and
//! the pane is pasted the path the way the host's terminal would drop it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use argus_protocol::PaneId;

/// How much of a name is kept. Enough to say what the file was; a name a
/// browser made of a whole page title is not worth a path that long.
const NAME_MAX: usize = 64;

/// Writes `bytes` under `root` for `pane` and says where they went. The
/// name is kept for its extension, which is how a harness tells an image
/// from text, and as a reminder of what was pasted; it is numbered so two
/// pastes of the same name both survive.
pub fn keep(root: &Path, pane: PaneId, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let dir = root.join(pane.to_string());
    fs::create_dir_all(&dir)?;
    let name = safe_name(name);
    let taken = fs::read_dir(&dir)?.count();
    for n in taken + 1.. {
        let path = dir.join(format!("{n}-{name}"));
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                io::Write::write_all(&mut file, bytes)?;
                return Ok(path);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("the numbers do not run out")
}

/// Takes out what `pane` was pasted; nothing to do when it was nothing.
pub fn forget(root: &Path, pane: PaneId) {
    let _ = fs::remove_dir_all(root.join(pane.to_string()));
}

/// Takes out every pane's files. For a daemon starting: every pane it
/// restores starts its process over, and what the old one was pasted it
/// has already read.
pub fn forget_all(root: &Path) {
    let _ = fs::remove_dir_all(root);
}

/// The path as the host's terminal would type it on a drop, which is the
/// spelling a harness already parses: spaces escaped on Unix, the whole
/// path quoted on Windows when it has any.
pub fn dropped(path: &Path) -> String {
    let path = path.to_string_lossy();
    if cfg!(windows) {
        if path.contains(char::is_whitespace) {
            format!("\"{path}\"")
        } else {
            path.into_owned()
        }
    } else {
        path.replace(' ', "\\ ")
    }
}

/// A name a shell, a path or a harness cannot trip over: one path
/// component, of characters that need no quoting, with its extension.
fn safe_name(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut safe: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    if safe.trim_matches(|c| c == '.' || c == '_').is_empty() {
        safe = "pasted".to_string();
    }
    if safe.chars().count() <= NAME_MAX {
        return safe;
    }
    // Cut the front, not the back: the extension is the part that matters.
    let keep_from = safe.chars().count() - NAME_MAX;
    safe.chars().skip(keep_from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pasted_file_lands_under_its_pane_with_its_extension() {
        let root = tempfile::tempdir().unwrap();
        let path = keep(root.path(), PaneId(7), "shot.png", b"png").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"png");
        assert_eq!(path.parent().unwrap(), root.path().join("7"));
        assert_eq!(path.extension().unwrap(), "png");
    }

    #[test]
    fn two_pastes_of_the_same_name_both_survive() {
        let root = tempfile::tempdir().unwrap();
        let first = keep(root.path(), PaneId(7), "shot.png", b"one").unwrap();
        let second = keep(root.path(), PaneId(7), "shot.png", b"two").unwrap();

        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), b"one");
        assert_eq!(fs::read(&second).unwrap(), b"two");
    }

    #[test]
    fn a_name_is_cut_down_to_what_a_path_can_take() {
        // A dropped file keeps its name, minus whatever a shell or a
        // harness's path parser would stop at. Only the last component: a
        // name with a directory in it must not land outside the pane's.
        assert_eq!(safe_name("Screen Shot 2026.png"), "Screen_Shot_2026.png");
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\Users\\me\\a b.jpg"), "C__Users_me_a_b.jpg");
        assert_eq!(safe_name(""), "pasted");
        assert_eq!(safe_name("..."), "pasted");
        let long = format!("{}.png", "x".repeat(100));
        let cut = safe_name(&long);
        assert_eq!(cut.chars().count(), NAME_MAX);
        assert!(cut.ends_with(".png"));
    }

    #[test]
    fn a_dropped_path_is_spelled_as_the_terminal_would() {
        let spelled = dropped(Path::new("/tmp/a b/1-shot.png"));
        if cfg!(windows) {
            assert_eq!(spelled, "\"/tmp/a b/1-shot.png\"");
        } else {
            assert_eq!(spelled, "/tmp/a\\ b/1-shot.png");
        }
        assert_eq!(dropped(Path::new("/tmp/1-shot.png")), "/tmp/1-shot.png");
    }

    #[test]
    fn forgetting_a_pane_takes_its_files_and_no_others() {
        let root = tempfile::tempdir().unwrap();
        let gone = keep(root.path(), PaneId(7), "a.png", b"a").unwrap();
        let kept = keep(root.path(), PaneId(8), "b.png", b"b").unwrap();

        forget(root.path(), PaneId(7));

        assert!(!gone.exists());
        assert!(kept.exists());
    }
}
