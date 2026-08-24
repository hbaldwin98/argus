//! Stable review snapshots and diffs. Capturing writes blobs and trees to the
//! Git object database, but never changes HEAD, branches, the real index, or
//! the working directory.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use argus_protocol::{ChangeKind, DiffLine, FileDiff, Hunk, LineKind, ReviewBase};

const MAX_LINES_PER_FILE: usize = 5_000;
const BINARY_NOTE: &str = "binary file";
const TOO_LARGE_NOTE: &str = "too large to display";

pub struct GeneratedReview {
    pub target_snapshot: String,
    pub baseline_snapshot: Option<String>,
    pub files: Vec<FileDiff>,
}

struct Snapshot {
    tree: git2::Oid,
    untracked: HashSet<String>,
}

pub fn generate(path: &Path, base: ReviewBase) -> anyhow::Result<GeneratedReview> {
    let repo = git2::Repository::open(path)
        .with_context(|| format!("could not open Git repository at {}", path.display()))?;
    let target = capture(&repo)?;
    let baseline = baseline(&repo)?;
    let old = match base {
        ReviewBase::WorkingTree => head_tree(&repo),
        ReviewBase::BranchPoint => branch_point_tree(&repo),
        // First use deliberately falls back to uncommitted work. It does not
        // establish a baseline until the client explicitly acknowledges it.
        ReviewBase::SinceLastLooked => baseline.or_else(|| head_tree(&repo)),
    };
    let files = render_diff(&repo, old, target.tree, &target.untracked)?;
    Ok(GeneratedReview {
        target_snapshot: target.tree.to_string(),
        baseline_snapshot: baseline.map(|oid| oid.to_string()),
        files,
    })
}

/// Move the hidden per-worktree ref only if its current value is exactly what
/// the displayed review said it was.
pub fn acknowledge(path: &Path, target: &str, expected: Option<&str>) -> anyhow::Result<()> {
    let repo = git2::Repository::open(path)?;
    let target = git2::Oid::from_str(target).context("invalid review snapshot")?;
    repo.find_tree(target)
        .context("review snapshot is no longer available")?;
    let expected = expected
        .map(git2::Oid::from_str)
        .transpose()
        .context("invalid expected review baseline")?;
    let name = baseline_ref(&repo)?;
    let mut tx = repo.transaction()?;
    tx.lock_ref(&name)?;
    let current = repo.find_reference(&name).ok().and_then(|r| r.target());
    if current != expected {
        return Err(anyhow!(
            "review baseline changed; refresh before acknowledging"
        ));
    }
    let sig = git2::Signature::now("Argus", "argus@localhost")?;
    tx.set_target(&name, target, Some(&sig), "review acknowledged")?;
    tx.commit()?;
    Ok(())
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
            let data = file_bytes(&full)
                .with_context(|| format!("could not capture {}", full.display()))?;
            let mut entry = source
                .get_path(Path::new(path), 0)
                .unwrap_or_else(|| new_entry(path, &full));
            entry.path = path.as_bytes().to_vec();
            entry.mode = worktree_mode(&full, entry.mode);
            entry.id = repo.blob(&data)?;
            entry.file_size = data.len().try_into().unwrap_or(u32::MAX);
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

fn file_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    #[cfg(unix)]
    if path.symlink_metadata()?.file_type().is_symlink() {
        return Ok(std::fs::read_link(path)?
            .as_os_str()
            .as_encoded_bytes()
            .to_vec());
    }
    std::fs::read(path)
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

fn baseline(repo: &git2::Repository) -> anyhow::Result<Option<git2::Oid>> {
    let name = baseline_ref(repo)?;
    match repo.find_reference(&name) {
        Ok(reference) => {
            let oid = reference.target().context("review baseline is symbolic")?;
            repo.find_tree(oid)
                .context("review baseline does not name a tree")?;
            Ok(Some(oid))
        }
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn baseline_ref(repo: &git2::Repository) -> anyhow::Result<String> {
    let git_dir: PathBuf = repo
        .path()
        .canonicalize()
        .unwrap_or_else(|_| repo.path().to_path_buf());
    // Hashing the private worktree Git directory as a blob gives a stable,
    // ref-safe identity and keeps linked worktrees in the common ref store apart.
    let identity = repo.blob(git_dir.to_string_lossy().as_bytes())?;
    Ok(format!("refs/argus/review/{identity}"))
}

fn head_tree(repo: &git2::Repository) -> Option<git2::Oid> {
    repo.head().ok()?.peel_to_tree().ok().map(|tree| tree.id())
}

fn branch_point_tree(repo: &git2::Repository) -> Option<git2::Oid> {
    let head = repo.head().ok()?;
    let fork = || {
        let mine = head.peel_to_commit().ok()?.id();
        let other = fork_candidate(repo, &head)?;
        let base = repo.merge_base(mine, other).ok()?;
        repo.find_commit(base)
            .ok()?
            .tree()
            .ok()
            .map(|tree| tree.id())
    };
    fork().or_else(|| head.peel_to_tree().ok().map(|tree| tree.id()))
}

fn fork_candidate(repo: &git2::Repository, head: &git2::Reference) -> Option<git2::Oid> {
    let name = head.shorthand().unwrap_or_default().to_string();
    if let Some(upstream) = repo
        .find_branch(&name, git2::BranchType::Local)
        .ok()
        .and_then(|b| b.upstream().ok())
    {
        return upstream.get().peel_to_commit().ok().map(|c| c.id());
    }
    ["main", "master", "develop", "trunk"]
        .iter()
        .filter(|d| **d != name)
        .find_map(|d| {
            repo.find_branch(d, git2::BranchType::Local)
                .ok()?
                .get()
                .peel_to_commit()
                .ok()
                .map(|c| c.id())
        })
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
    opts.context_lines(3);
    let mut diff =
        repo.diff_tree_to_tree(old_tree.as_ref(), Some(&target_tree), Some(&mut opts))?;
    let mut find = git2::DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))
        .context("could not detect review renames")?;

    let files: RefCell<Vec<FileDiff>> = RefCell::new(Vec::new());
    diff.foreach(
        &mut |delta, _| {
            files.borrow_mut().push(new_file(&delta, untracked));
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
    FileDiff {
        old_path: (kind == ChangeKind::Renamed)
            .then(|| delta.old_file().path().map(slashed))
            .flatten(),
        path,
        kind,
        hunks: Vec::new(),
        note: delta.flags().is_binary().then(|| BINARY_NOTE.to_string()),
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

    fn commit_on_branch(path: &Path, branch: &str, files: &[(&str, &str)]) {
        let repo = git2::Repository::open(path).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch(branch, &head, false).unwrap();
        repo.set_head(&format!("refs/heads/{branch}")).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        for (name, body) in files {
            write(path, name, body);
        }
        commit(&repo, "branch work");
    }

    #[test]
    fn snapshot_contains_staged_unstaged_untracked_and_deletions() {
        let (_dir, path) = repo_with(&[
            ("staged.txt", "old\n"),
            ("unstaged.txt", "old\n"),
            ("gone.txt", "old\n"),
        ]);
        write(&path, "staged.txt", "staged\n");
        let repo = git2::Repository::open(&path).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("staged.txt")).unwrap();
        index.write().unwrap();
        write(&path, "unstaged.txt", "working\n");
        write(&path, "new.txt", "new\n");
        std::fs::remove_file(path.join("gone.txt")).unwrap();

        let review = generate(&path, ReviewBase::WorkingTree).unwrap();
        assert_eq!(find(&review.files, "staged.txt").added_lines(), 1);
        assert_eq!(find(&review.files, "unstaged.txt").added_lines(), 1);
        assert_eq!(find(&review.files, "new.txt").kind, ChangeKind::Untracked);
        assert_eq!(find(&review.files, "gone.txt").kind, ChangeKind::Deleted);
    }

    #[test]
    fn rendered_lines_keep_numbers_without_markers_or_newlines() {
        let (_dir, path) = repo_with(&[("a.txt", "one\ntwo\nthree\n")]);
        write(&path, "a.txt", "one\nTWO\nthree\n");

        let review = generate(&path, ReviewBase::WorkingTree).unwrap();
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
        let review = generate(&path, ReviewBase::WorkingTree).unwrap();
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

        let review = generate(&path, ReviewBase::WorkingTree).unwrap();
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
        let review = generate(dir.path(), ReviewBase::WorkingTree).unwrap();
        assert_eq!(find(&review.files, "a.txt").added_lines(), 1);
    }

    #[test]
    fn branch_bases_keep_committed_work_out_of_uncommitted_review() {
        let (_dir, path) = repo_with(&[("a.txt", "one\n")]);
        commit_on_branch(&path, "feature", &[("b.txt", "committed\n")]);
        write(&path, "c.txt", "working\n");

        let uncommitted = generate(&path, ReviewBase::WorkingTree).unwrap();
        assert!(uncommitted.files.iter().all(|file| file.path != "b.txt"));
        let branch = generate(&path, ReviewBase::BranchPoint).unwrap();
        assert_eq!(find(&branch.files, "b.txt").added_lines(), 1);
        assert_eq!(find(&branch.files, "c.txt").added_lines(), 1);
    }

    #[test]
    fn rename_detection_reports_both_paths() {
        let (_dir, path) = repo_with(&[("old.txt", "same content\n")]);
        std::fs::rename(path.join("old.txt"), path.join("new.txt")).unwrap();
        let review = generate(&path, ReviewBase::WorkingTree).unwrap();
        let file = find(&review.files, "new.txt");
        assert_eq!(file.kind, ChangeKind::Renamed);
        assert_eq!(file.old_path.as_deref(), Some("old.txt"));
    }

    #[test]
    fn a_staged_rename_is_detected_from_the_synthetic_tree() {
        let (_dir, path) = repo_with(&[("old.txt", "same content\n")]);
        std::fs::rename(path.join("old.txt"), path.join("new.txt")).unwrap();
        let repo = git2::Repository::open(&path).unwrap();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new("old.txt")).unwrap();
        index.add_path(Path::new("new.txt")).unwrap();
        index.write().unwrap();

        let review = generate(&path, ReviewBase::WorkingTree).unwrap();
        assert_eq!(find(&review.files, "new.txt").kind, ChangeKind::Renamed);
    }

    #[test]
    fn ignored_untracked_content_is_not_captured() {
        let (_dir, path) = repo_with(&[(".gitignore", "ignored.txt\n")]);
        write(&path, "ignored.txt", "secret\n");
        assert!(generate(&path, ReviewBase::WorkingTree)
            .unwrap()
            .files
            .is_empty());
    }

    #[test]
    fn first_since_last_looked_falls_back_without_creating_a_baseline() {
        let (_dir, path) = repo_with(&[("a.txt", "old\n")]);
        write(&path, "a.txt", "new\n");
        let review = generate(&path, ReviewBase::SinceLastLooked).unwrap();
        assert!(review.baseline_snapshot.is_none());
        assert_eq!(review.files.len(), 1);
        assert!(generate(&path, ReviewBase::SinceLastLooked)
            .unwrap()
            .baseline_snapshot
            .is_none());
    }

    #[test]
    fn acknowledged_baseline_is_durable_and_cas_protected() {
        let (_dir, path) = repo_with(&[("a.txt", "old\n")]);
        write(&path, "a.txt", "one\n");
        let first = generate(&path, ReviewBase::SinceLastLooked).unwrap();
        acknowledge(&path, &first.target_snapshot, None).unwrap();
        drop(git2::Repository::open(&path).unwrap());
        assert!(generate(&path, ReviewBase::SinceLastLooked)
            .unwrap()
            .files
            .is_empty());

        write(&path, "a.txt", "two\n");
        let second = generate(&path, ReviewBase::SinceLastLooked).unwrap();
        assert!(acknowledge(&path, &second.target_snapshot, None).is_err());
        acknowledge(
            &path,
            &second.target_snapshot,
            second.baseline_snapshot.as_deref(),
        )
            .unwrap();
    }

    #[test]
    fn acknowledging_a_displayed_snapshot_does_not_hide_later_edits() {
        let (_dir, path) = repo_with(&[("a.txt", "old\n")]);
        write(&path, "a.txt", "displayed\n");
        let displayed = generate(&path, ReviewBase::SinceLastLooked).unwrap();
        write(&path, "a.txt", "later\n");

        acknowledge(&path, &displayed.target_snapshot, None).unwrap();
        let remaining = generate(&path, ReviewBase::SinceLastLooked).unwrap();
        let file = find(&remaining.files, "a.txt");
        assert!(file.hunks.iter().flat_map(|hunk| &hunk.lines).any(|line| {
            line.kind == LineKind::Added && line.text == "later"
        }));
    }

    #[test]
    fn linked_worktrees_use_distinct_baseline_refs() {
        let (_dir, path) = repo_with(&[("a.txt", "old\n")]);
        let linked_root = tempfile::tempdir().unwrap();
        let linked = linked_root.path().join("worktree");
        let repo = git2::Repository::open(&path).unwrap();
        repo.worktree("linked", &linked, None).unwrap();
        let other = git2::Repository::open(&linked).unwrap();
        assert_ne!(baseline_ref(&repo).unwrap(), baseline_ref(&other).unwrap());
    }

    #[test]
    fn capture_errors_are_not_successful_empty_reviews() {
        let dir = tempfile::tempdir().unwrap();
        assert!(generate(dir.path(), ReviewBase::WorkingTree).is_err());
    }
}
