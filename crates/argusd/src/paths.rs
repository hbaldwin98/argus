//! Recognizing when two spellings name the same place on disk.
//!
//! Paths reach the daemon from three directions that spell them
//! differently — the user's config, libgit2, and the store — so comparing
//! them textually would make a repository configured as `C:/src/x` and the
//! worktree libgit2 reports at `C:\src\x` two different rows.

use std::path::Path;

/// Compares paths by their canonical form where one is available, falling
/// back to a literal comparison for a path that does not exist yet.
pub fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Whether a string starts `C:` — an absolute Windows path, which
/// `Path::is_absolute` on a Unix build does not recognize.
pub fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_that_do_not_exist_are_compared_as_written() {
        assert!(same_path(
            Path::new("/nowhere/at/all"),
            Path::new("/nowhere/at/all")
        ));
        assert!(!same_path(Path::new("/nowhere/a"), Path::new("/nowhere/b")));
    }

    #[test]
    fn a_drive_letter_is_recognized_as_absolute() {
        assert!(has_windows_drive_prefix("C:\\src\\x"));
        assert!(has_windows_drive_prefix("c:/src/x"));
        assert!(!has_windows_drive_prefix("src/x"));
        assert!(!has_windows_drive_prefix(":"));
    }
}
