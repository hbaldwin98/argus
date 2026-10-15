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

/// Whether `rel`, joined to `root`, still lands inside `root` once every
/// symlink on the way is followed. The text of `rel` having no `..` says
/// nothing about a link inside the checkout that points out of it.
///
/// A path that does not exist yet (an editor may be creating it) is judged by
/// its nearest ancestor that does, since that is the last place a link could
/// redirect it from.
pub fn stays_inside(root: &Path, rel: &str) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let mut at = root.join(rel);
    loop {
        if let Ok(real) = at.canonicalize() {
            return real.starts_with(&root);
        }
        if !at.pop() {
            return false;
        }
    }
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

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_root_is_outside_it_and_a_missing_file_is_judged_by_its_directory() {
        let base = std::env::temp_dir().join(format!("argus-paths-{}", std::process::id()));
        let (root, elsewhere) = (base.join("root"), base.join("elsewhere"));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("out")).unwrap();
        assert!(stays_inside(&root, "src"));
        assert!(stays_inside(&root, "src/new.rs"), "not made yet, but its directory is inside");
        assert!(!stays_inside(&root, "out"));
        assert!(!stays_inside(&root, "out/new.rs"), "a file made through a link lands outside");
        assert!(!stays_inside(&base.join("absent"), "x"));
        std::fs::remove_dir_all(&base).unwrap();
    }
}
