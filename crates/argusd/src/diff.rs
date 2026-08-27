//! Stable review snapshots and diffs. Capturing writes blobs and trees to the
//! Git object database, but never changes HEAD, branches, the real index, or
//! the working directory.

use std::cell::{Cell as ValueCell, RefCell};
use std::collections::HashSet;
use std::path::Path;

use anyhow::Context;
use argus_protocol::{ChangeKind, DiffLine, FileDiff, Hunk, LineKind, ReviewBase};

const MAX_LINES_PER_FILE: usize = 5_000;
const MAX_TOTAL_LINES: usize = 20_000;
const MAX_DIFF_BYTES: i64 = 1024 * 1024;
const BINARY_NOTE: &str = "binary file";
const TOO_LARGE_NOTE: &str = "too large to display";

pub struct GeneratedReview {
    pub files: Vec<FileDiff>,
}

struct Snapshot {
    tree: git2::Oid,
    untracked: HashSet<String>,
}

pub fn generate(path: &Path, base: ReviewBase) -> anyhow::Result<GeneratedReview> {
    let repo = git2::Repository::open(path)
        .with_context(|| format!("could not open Git repository at {}", path.display()))?;
    let index = index_tree(&repo)?;
    // The two sides Git keeps apart. Staged work is already in the index, so
    // nothing untracked can be in it; unstaged work is everything the index
    // has not been told about yet, untracked files included.
    let (old, target) = match base {
        ReviewBase::Staged => (head_tree(&repo), Snapshot { tree: index, untracked: HashSet::new() }),
        ReviewBase::Unstaged => (Some(index), capture(&repo)?),
    };
    let files = render_diff(&repo, old, target.tree, &target.untracked)?;
    Ok(GeneratedReview { files })
}

/// The index exactly as it stands, with no working-tree content laid over it.
/// Writes a tree object and nothing else: the on-disk index is never rewritten.
fn index_tree(repo: &git2::Repository) -> anyhow::Result<git2::Oid> {
    let mut index = repo.index().context("could not read Git index")?;
    index
        .write_tree_to(repo)
        .context("could not capture the Git index")
}

fn capture(repo: &git2::Repository) -> anyhow::Result<Snapshot> {
    let workdir = repo
        .workdir()
        .context("bare repositories cannot be reviewed")?;
    let source = repo.index().context("could not read Git index")?;
    let mut synthetic = git2::Index::new()?;
    for entry in source.iter() {
        synthetic.add(&entry)?;
    }

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut opts))
        .context("could not inspect working tree")?;
    let mut untracked = HashSet::new();
    for status in statuses.iter() {
        let path = status.path().context("review path is not valid UTF-8")?;
        let flags = status.status();
        if flags.contains(git2::Status::WT_DELETED) {
            synthetic.remove_path(Path::new(path))?;
        }
        if flags.intersects(
            git2::Status::WT_NEW | git2::Status::WT_MODIFIED | git2::Status::WT_TYPECHANGE,
        ) {
            let full = workdir.join(path);
            if full.is_dir() {
                // A dirty submodule retains its indexed gitlink. Its internal
                // worktree is a separate repository and review boundary.
                continue;
            }
            let (id, file_size) = capture_blob(repo, &full)
                .with_context(|| format!("could not capture {}", full.display()))?;
            let mut entry = source
                .get_path(Path::new(path), 0)
                .unwrap_or_else(|| new_entry(path, &full));
            entry.path = path.as_bytes().to_vec();
            entry.mode = worktree_mode(&full, entry.mode);
            entry.id = id;
            entry.file_size = file_size;
            synthetic.add(&entry)?;
            if flags.contains(git2::Status::WT_NEW) {
                untracked.insert(path.replace('\\', "/"));
            }
        }
    }
    let tree = synthetic
        .write_tree_to(repo)
        .context("could not write review snapshot")?;
    Ok(Snapshot { tree, untracked })
}

fn capture_blob(repo: &git2::Repository, path: &Path) -> anyhow::Result<(git2::Oid, u32)> {
    #[cfg(unix)]
    if path.symlink_metadata()?.file_type().is_symlink() {
        let data = file_bytes(path)?;
        return Ok((repo.blob(&data)?, data.len().try_into().unwrap_or(u32::MAX)));
    }
    let size = path.metadata()?.len().try_into().unwrap_or(u32::MAX);
    Ok((repo.blob_path(path)?, size))
}

#[cfg(unix)]
fn file_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    Ok(std::fs::read_link(path)?
        .as_os_str()
        .as_encoded_bytes()
        .to_vec())
}

fn new_entry(path: &str, _full: &Path) -> git2::IndexEntry {
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        if _full
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            0o120000
        } else if _full
            .metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
        {
            0o100755
        } else {
            0o100644
        }
    };
    #[cfg(not(unix))]
    let mode = 0o100644;
    git2::IndexEntry {
        ctime: git2::IndexTime::new(0, 0),
        mtime: git2::IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode,
        uid: 0,
        gid: 0,
        file_size: 0,
        id: git2::Oid::zero(),
        flags: 0,
        flags_extended: 0,
        path: path.as_bytes().to_vec(),
    }
}

fn worktree_mode(_path: &Path, indexed: u32) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let Ok(metadata) = _path.symlink_metadata() else {
            return indexed;
        };
        if metadata.file_type().is_symlink() {
            0o120000
        } else if metadata.permissions().mode() & 0o111 != 0 {
            0o100755
        } else {
            0o100644
        }
    }
    #[cfg(not(unix))]
    indexed
}

fn head_tree(repo: &git2::Repository) -> Option<git2::Oid> {
    repo.head().ok()?.peel_to_tree().ok().map(|tree| tree.id())
}

fn render_diff(
    repo: &git2::Repository,
    old: Option<git2::Oid>,
    target: git2::Oid,
    untracked: &HashSet<String>,
) -> anyhow::Result<Vec<FileDiff>> {
    let old_tree = old.map(|oid| repo.find_tree(oid)).transpose()?;
    let target_tree = repo.find_tree(target)?;
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(3).max_size(MAX_DIFF_BYTES);
    let mut diff =
        repo.diff_tree_to_tree(old_tree.as_ref(), Some(&target_tree), Some(&mut opts))?;
    let mut find = git2::DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))
        .context("could not detect review renames")?;

    let files: RefCell<Vec<FileDiff>> = RefCell::new(Vec::new());
    let rendered_lines = ValueCell::new(0usize);
    diff.foreach(
        &mut |delta, _| {
            let mut file = new_file(&delta, untracked);
            if rendered_lines.get() >= MAX_TOTAL_LINES && file.note.is_none() {
                file.note = Some(TOO_LARGE_NOTE.to_string());
            }
            files.borrow_mut().push(file);
            true
        },
        None,
        Some(&mut |_, hunk| {
            if let Some(file) = files.borrow_mut().last_mut() {
                if file.note.is_none() {
                    file.hunks.push(Hunk {
                        header: String::from_utf8_lossy(hunk.header())
                            .trim_end()
                            .to_string(),
                        lines: Vec::new(),
                    });
                }
            }
            true
        }),
        Some(&mut |_, _, line| {
            let mut files = files.borrow_mut();
            let Some(file) = files.last_mut() else {
                return true;
            };
            if file.note.is_some() {
                return true;
            }
            if rendered_lines.get() >= MAX_TOTAL_LINES {
                file.hunks.clear();
                file.note = Some(TOO_LARGE_NOTE.to_string());
                return true;
            }
            let kind = match line.origin() {
                '+' => LineKind::Added,
                '-' => LineKind::Removed,
                ' ' => LineKind::Context,
                _ => return true,
            };
            if let Some(hunk) = file.hunks.last_mut() {
                hunk.lines.push(DiffLine {
                    kind,
                    old_lineno: line.old_lineno(),
                    new_lineno: line.new_lineno(),
                    text: String::from_utf8_lossy(line.content())
                        .trim_end_matches(['\n', '\r'])
                        .to_string(),
                });
                rendered_lines.set(rendered_lines.get() + 1);
            }
            if total_lines(file) > MAX_LINES_PER_FILE {
                file.hunks.clear();
                file.note = Some(TOO_LARGE_NOTE.to_string());
            }
            true
        }),
    )?;
    Ok(files.into_inner())
}

fn new_file(delta: &git2::DiffDelta, untracked: &HashSet<String>) -> FileDiff {
    let path = delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(slashed)
        .unwrap_or_default();
    let kind = match delta.status() {
        git2::Delta::Added if untracked.contains(&path) => ChangeKind::Untracked,
        git2::Delta::Added => ChangeKind::Added,
        git2::Delta::Deleted => ChangeKind::Deleted,
        git2::Delta::Renamed => ChangeKind::Renamed,
        _ => ChangeKind::Modified,
    };
    let too_large = delta.old_file().size().max(delta.new_file().size()) > MAX_DIFF_BYTES as u64;
    FileDiff {
        old_path: (kind == ChangeKind::Renamed)
            .then(|| delta.old_file().path().map(slashed))
            .flatten(),
        path,
        kind,
        hunks: Vec::new(),
        note: too_large
            .then(|| TOO_LARGE_NOTE.to_string())
            .or_else(|| delta.flags().is_binary().then(|| BINARY_NOTE.to_string())),
    }
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
fn total_lines(file: &FileDiff) -> usize {
    file.hunks.iter().map(|h| h.lines.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_with(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let repo = git2::Repository::init(&path).unwrap();
        for (name, body) in files {
            write(&path, name, body);
        }
        commit(&repo, "first");
        drop(repo);
        (dir, path)
    }

    fn write(root: &Path, name: &str, body: &str) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn commit(repo: &git2::Repository, message: &str) {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap();
    }

    fn find<'a>(files: &'a [FileDiff], path: &str) -> &'a FileDiff {
        files.iter().find(|f| f.path == path).unwrap()
    }

    #[test]
    fn each_side_shows_only_its_own_half_of_the_work() {
        let (_dir, path) = repo_with(&[
            ("staged.txt", "old
"),
            ("unstaged.txt", "old
"),
            ("gone.txt", "old
"),
        ]);
        write(&path, "staged.txt", "staged
");
        let repo = git2::Repository::open(&path).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("staged.txt")).unwrap();
        index.write().unwrap();
        write(&path, "unstaged.txt", "working
");
        write(&path, "new.txt", "new
");
        std::fs::remove_file(path.join("gone.txt")).unwrap();

        let staged = generate(&path, ReviewBase::Staged).unwrap();
        assert_eq!(find(&staged.files, "staged.txt").added_lines(), 1);
        assert!(
            staged.files.iter().all(|f| f.path == "staged.txt"),
            "only the indexed edit is staged: {:?}",
            staged.files.iter().map(|f| &f.path).collect::<Vec<_>>()
        );

        let unstaged = generate(&path, ReviewBase::Unstaged).unwrap();
        assert_eq!(find(&unstaged.files, "unstaged.txt").added_lines(), 1);
        assert_eq!(find(&unstaged.files, "new.txt").kind, ChangeKind::Untracked);
        assert_eq!(find(&unstaged.files, "gone.txt").kind, ChangeKind::Deleted);
        assert!(
            unstaged.files.iter().all(|f| f.path != "staged.txt"),
            "an edit already in the index is not still unstaged"
        );
    }

    /// The case the single collapsed endpoint could not express at all: one
    /// file with a different diff on each side.
    #[test]
    fn a_partly_staged_file_shows_a_different_diff_on_each_side() {
        let (_dir, path) = repo_with(&[("a.txt", "one
")]);
        write(&path, "a.txt", "two
");
        let repo = git2::Repository::open(&path).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        drop(index);
        drop(repo);
        write(&path, "a.txt", "three
");

        let added = |review: &GeneratedReview| {
            find(&review.files, "a.txt")
                .hunks
                .iter()
                .flat_map(|hunk| &hunk.lines)
                .filter(|line| line.kind == LineKind::Added)
                .map(|line| line.text.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(added(&generate(&path, ReviewBase::Staged).unwrap()), ["two"]);
        assert_eq!(
            added(&generate(&path, ReviewBase::Unstaged).unwrap()),
            ["three"]
        );
    }

    #[test]
    fn rendered_lines_keep_numbers_without_markers_or_newlines() {
        let (_dir, path) = repo_with(&[("a.txt", "one\ntwo\nthree\n")]);
        write(&path, "a.txt", "one\nTWO\nthree\n");

        let review = generate(&path, ReviewBase::Unstaged).unwrap();
        let file = find(&review.files, "a.txt");
        let removed = file.hunks[0]
            .lines
            .iter()
            .find(|line| line.kind == LineKind::Removed)
            .unwrap();
        let added = file.hunks[0]
            .lines
            .iter()
            .find(|line| line.kind == LineKind::Added)
            .unwrap();
        assert_eq!((removed.old_lineno, removed.text.as_str()), (Some(2), "two"));
        assert_eq!((added.new_lineno, added.text.as_str()), (Some(2), "TWO"));
        assert!(file.hunks[0]
            .lines
            .iter()
            .all(|line| !line.text.ends_with('\n')));
    }

    #[test]
    fn untracked_directories_are_captured_as_files() {
        let (_dir, path) = repo_with(&[("a.txt", "one\n")]);
        write(&path, "sub/deep/new.txt", "new\n");
        let review = generate(&path, ReviewBase::Unstaged).unwrap();
        assert_eq!(find(&review.files, "sub/deep/new.txt").kind, ChangeKind::Untracked);
    }

    #[test]
    fn binary_and_oversized_files_are_listed_without_content() {
        let (_dir, path) = repo_with(&[("a.txt", "one\n")]);
        std::fs::write(path.join("blob.bin"), [0u8, 159, 146, 150, 0]).unwrap();
        let big: String = (0..MAX_LINES_PER_FILE + 10)
            .map(|line| format!("line {line}\n"))
            .collect();
        write(&path, "big.txt", &big);

        let review = generate(&path, ReviewBase::Unstaged).unwrap();
        let binary = find(&review.files, "blob.bin");
        assert_eq!(binary.note.as_deref(), Some(BINARY_NOTE));
        assert!(binary.hunks.is_empty());
        let big = find(&review.files, "big.txt");
        assert_eq!(big.note.as_deref(), Some(TOO_LARGE_NOTE));
        assert!(big.hunks.is_empty());
    }

    #[test]
    fn an_unborn_repository_treats_everything_as_new() {
        let dir = tempfile::tempdir().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        write(dir.path(), "a.txt", "one\n");
        let review = generate(dir.path(), ReviewBase::Unstaged).unwrap();
        assert_eq!(find(&review.files, "a.txt").added_lines(), 1);
    }

    #[test]
    fn a_review_has_a_global_rendered_line_limit() {
        let (_dir, path) = repo_with(&[("base.txt", "base\n")]);
        for file in 0..5 {
            let body: String = (0..MAX_LINES_PER_FILE)
                .map(|line| format!("line {line}\n"))
                .collect();
            write(&path, &format!("large-{file}.txt"), &body);
        }

        let review = generate(&path, ReviewBase::Unstaged).unwrap();
        let rendered: usize = review.files.iter().map(total_lines).sum();
        assert!(rendered <= MAX_TOTAL_LINES, "rendered {rendered} lines");
        assert!(review
            .files
            .iter()
            .any(|file| file.note.as_deref() == Some(TOO_LARGE_NOTE)));
    }

    #[test]
    fn rename_detection_reports_both_paths() {
        let (_dir, path) = repo_with(&[("old.txt", "same content\n")]);
        std::fs::rename(path.join("old.txt"), path.join("new.txt")).unwrap();
        let review = generate(&path, ReviewBase::Unstaged).unwrap();
        let file = find(&review.files, "new.txt");
        assert_eq!(file.kind, ChangeKind::Renamed);
        assert_eq!(file.old_path.as_deref(), Some("old.txt"));
    }

    #[test]
    fn a_staged_rename_is_detected_on_the_staged_side() {
        let (_dir, path) = repo_with(&[("old.txt", "same content\n")]);
        std::fs::rename(path.join("old.txt"), path.join("new.txt")).unwrap();
        let repo = git2::Repository::open(&path).unwrap();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new("old.txt")).unwrap();
        index.add_path(Path::new("new.txt")).unwrap();
        index.write().unwrap();

        let review = generate(&path, ReviewBase::Staged).unwrap();
        assert_eq!(find(&review.files, "new.txt").kind, ChangeKind::Renamed);
    }

    #[test]
    fn ignored_untracked_content_is_not_captured() {
        let (_dir, path) = repo_with(&[(".gitignore", "ignored.txt\n")]);
        write(&path, "ignored.txt", "secret\n");
        assert!(generate(&path, ReviewBase::Unstaged)
            .unwrap()
            .files
            .is_empty());
    }

    #[test]
    fn capture_errors_are_not_successful_empty_reviews() {
        let dir = tempfile::tempdir().unwrap();
        assert!(generate(dir.path(), ReviewBase::Unstaged).is_err());
    }
}
