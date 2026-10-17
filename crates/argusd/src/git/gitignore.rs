//! Keeps Argus-managed checkout files out of Git status without changing the repository.
//!
//! The repository-local `info/exclude` is the right boundary: it is private to
//! that repository, never enters a commit, and applies to every linked
//! worktree that shares the repository's Git directory.

use std::path::{Component, Path, PathBuf};

const HEADER: &str = "# argus: managed checkout files";

/// Adds each relative managed file to the repository's local exclude file.
///
/// A plain directory is a valid Argus checkout, so non-repositories are a
/// no-op. Existing text is preserved and each path is added at most once.
/// Invalid paths are skipped because a user-configured harness must not turn
/// an optional status convenience into a failed agent start.
pub(crate) fn ensure(checkout: &Path, paths: impl IntoIterator<Item = PathBuf>) -> anyhow::Result<()> {
    let repo = match git2::Repository::open(checkout) {
        Ok(repo) => repo,
        Err(_) => return Ok(()),
    };
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    let checkout = checkout.canonicalize().unwrap_or_else(|_| checkout.to_path_buf());
    let workdir = workdir
        .canonicalize()
        .unwrap_or_else(|_| workdir.to_path_buf());
    let Ok(relative_checkout) = checkout.strip_prefix(&workdir) else {
        return Ok(());
    };
    let paths: Vec<String> = paths
        .into_iter()
        .filter_map(|path| pattern_for(relative_checkout, &path))
        .collect();
    if paths.is_empty() {
        return Ok(());
    }

    let common = crate::git::resolve_commondir(repo.path());
    let info = common.join("info");
    ensure_directory(&info)?;
    let exclude = info.join("exclude");
    ensure_regular_file(&exclude)?;

    let mut contents = match std::fs::read_to_string(&exclude) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    let mut missing: Vec<String> = paths
        .into_iter()
        .filter(|path| !contains_pattern(&contents, path))
        .collect();
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        return Ok(());
    }

    if !contents.is_empty() && !contents.ends_with('\n') {
        contents.push('\n');
    }
    if !contents.lines().any(|line| line.trim() == HEADER) {
        if !contents.is_empty() && !contents.ends_with("\n\n") {
            contents.push('\n');
        }
        contents.push_str(HEADER);
        contents.push('\n');
    }
    for path in missing {
        contents.push_str(&path);
        contents.push('\n');
    }

    std::fs::write(exclude, contents)?;
    Ok(())
}

fn pattern_for(relative_checkout: &Path, path: &Path) -> Option<String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if normalized.as_os_str().is_empty() {
        return None;
    }

    let relative = relative_checkout.join(normalized);
    let text = relative.to_string_lossy().replace('\\', "/");
    (!text.is_empty()).then(|| format!("/{text}"))
}

fn contains_pattern(contents: &str, pattern: &str) -> bool {
    let unanchored = pattern.strip_prefix('/').unwrap_or(pattern);
    contents.lines().any(|line| {
        let line = line.trim();
        line == pattern || line == unanchored
    })
}

fn ensure_directory(path: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => anyhow::ensure!(meta.is_dir(), "{} is not a directory", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(path)?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn ensure_regular_file(path: &Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            anyhow::ensure!(meta.is_file(), "{} is not a regular file", path.display());
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_exclude(dir: &Path) -> PathBuf {
        dir.join(".git/info/exclude")
    }

    #[test]
    fn adds_managed_files_to_the_repository_exclude_once() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        drop(repo);

        ensure(
            dir.path(),
            [
                PathBuf::from(".pi/extensions/argus-status.ts"),
                PathBuf::from(".claude/settings.local.json"),
            ],
        )
        .unwrap();
        let first = std::fs::read_to_string(local_exclude(dir.path())).unwrap();
        assert!(first.contains(HEADER));
        assert!(first.contains("/.claude/settings.local.json"));
        assert!(first.contains("/.pi/extensions/argus-status.ts"));

        ensure(
            dir.path(),
            [
                PathBuf::from(".pi/extensions/argus-status.ts"),
                PathBuf::from(".claude/settings.local.json"),
            ],
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(local_exclude(dir.path())).unwrap(),
            first
        );
    }

    #[test]
    fn preserves_existing_excludes_and_ignores_generated_files() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        drop(repo);
        let exclude = local_exclude(dir.path());
        std::fs::write(&exclude, "# mine\n/custom\n").unwrap();

        ensure(dir.path(), [PathBuf::from(".claude/settings.local.json")]).unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(
            dir.path().join(".claude/settings.local.json"),
            "generated",
        )
        .unwrap();

        let repo = git2::Repository::open(dir.path()).unwrap();
        let mut options = git2::StatusOptions::new();
        options.include_untracked(true);
        let statuses = repo.statuses(Some(&mut options)).unwrap();
        let count = statuses.iter().count();
        assert_eq!(count, 0, "generated file was not ignored");
        let contents = std::fs::read_to_string(exclude).unwrap();
        assert!(contents.starts_with("# mine\n/custom\n"));
    }

    #[test]
    fn ignores_invalid_paths_and_plain_directories() {
        let dir = tempfile::tempdir().unwrap();
        ensure(
            dir.path(),
            [PathBuf::from("../outside"), PathBuf::from("/absolute")],
        )
        .unwrap();
        assert!(!dir.path().join(".git").exists());
    }
}
