//! Where a model's file is kept in the folder the window gives, and whether it is there: whole, part
//! downloaded, or not at all.
//!
//! A download writes `<file>.partial` and renames it once it is whole and its hash is checked, so a
//! file under its own name is one that was checked; its size is checked again on each look, since a
//! disk can lose the end of a file.

use std::path::{Path, PathBuf};

use super::catalog::Entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnDisk {
    Missing,
    /// This many bytes of a download that stopped.
    Partial(u64),
    Ready,
}

pub fn path(dir: &Path, entry: &Entry) -> PathBuf {
    dir.join(entry.file)
}

pub fn partial(dir: &Path, entry: &Entry) -> PathBuf {
    dir.join(format!("{}.partial", entry.file))
}

pub fn on_disk(dir: &Path, entry: &Entry) -> OnDisk {
    let size = |path: PathBuf| std::fs::metadata(path).ok().filter(|meta| meta.is_file()).map(|meta| meta.len());
    if size(path(dir, entry)) == Some(entry.size) {
        return OnDisk::Ready;
    }
    match size(partial(dir, entry)) {
        Some(done) if done > 0 && done < entry.size => OnDisk::Partial(done),
        _ => OnDisk::Missing,
    }
}

/// Removes the file and any part of one; a file that is not there is already deleted.
pub fn delete(dir: &Path, entry: &Entry) -> std::io::Result<()> {
    for path in [path(dir, entry), partial(dir, entry)] {
        match std::fs::remove_file(&path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: Entry = Entry { id: "tiny", repo: "a/b", file: "tiny.gguf", size: 4, sha256: "" };

    #[test]
    fn a_file_is_ready_only_whole_and_a_partial_one_says_how_far_it_got() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(on_disk(dir.path(), &ENTRY), OnDisk::Missing);
        std::fs::write(partial(dir.path(), &ENTRY), b"ab").unwrap();
        assert_eq!(on_disk(dir.path(), &ENTRY), OnDisk::Partial(2));
        std::fs::write(path(dir.path(), &ENTRY), b"abc").unwrap();
        assert_eq!(on_disk(dir.path(), &ENTRY), OnDisk::Partial(2), "a short file is not the model");
        std::fs::write(path(dir.path(), &ENTRY), b"abcd").unwrap();
        assert_eq!(on_disk(dir.path(), &ENTRY), OnDisk::Ready);
        delete(dir.path(), &ENTRY).unwrap();
        assert_eq!(on_disk(dir.path(), &ENTRY), OnDisk::Missing);
        delete(dir.path(), &ENTRY).unwrap();
    }
}
