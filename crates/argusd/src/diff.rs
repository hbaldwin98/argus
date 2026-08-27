//! Stable review snapshots and diffs. Capturing writes blobs and trees to the
//! Git object database, but never changes HEAD, branches, the real index, or
//! the working directory.

use std::cell::{Cell as ValueCell, RefCell};
use std::collections::HashSet;
use std::path::Path;

use anyhow::Context;
use argus_protocol::{
    ChangeKind, CommitFile, CommitInfo, DiffLine, FileDiff, HistoryCommit, Hunk, LineKind,
    ReviewBase, MAX_HISTORY_COMMITS,
};

const MAX_LINES_PER_FILE: usize = 5_000;
const MAX_TOTAL_LINES: usize = 20_000;
const MAX_DIFF_BYTES: i64 = 1024 * 1024;
const BINARY_NOTE: &str = "binary file";
const TOO_LARGE_NOTE: &str = "too large to display";

pub struct GeneratedReview {
    pub files: Vec<FileDiff>,
    pub commit: Option<CommitInfo>,
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
        ReviewBase::Staged => (
            head_tree(&repo),
            Snapshot {
                tree: index,
                untracked: HashSet::new(),
            },
        ),
        ReviewBase::Unstaged => (Some(index), capture(&repo)?),
        ReviewBase::Commit => anyhow::bail!("a commit review needs a commit id"),
    };
    let mut files = render_diff(&repo, old, target.tree, &target.untracked)?;
    highlight_files(&repo, old, target.tree, &mut files);
    Ok(GeneratedReview {
        files,
        commit: None,
    })
}

/// Newest first, first-parent diffs, capped. An unborn HEAD is an empty
/// history rather than an error — there is nothing to show yet.
pub fn list_commits(path: &Path) -> anyhow::Result<Vec<HistoryCommit>> {
    let repo = git2::Repository::open(path)
        .with_context(|| format!("could not open Git repository at {}", path.display()))?;
    // `revwalk.push_head` reports GenericError for an unborn branch, not
    // UnbornBranch. `repository.head` is the call that names that case.
    match repo.head() {
        Ok(_) => {}
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => return Ok(Vec::new()),
        Err(error) => return Err(error).context("could not read HEAD"),
    }
    let mut walk = repo.revwalk().context("could not walk commits")?;
    // Newest first, like `git log`. TIME is already that order; REVERSE
    // would turn the cap below into the oldest commits in the repository.
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;
    walk.push_head().context("could not read HEAD")?;
    let mut commits = Vec::new();
    for oid in walk.take(MAX_HISTORY_COMMITS) {
        let oid = oid.context("could not read a commit from history")?;
        let commit = repo
            .find_commit(oid)
            .with_context(|| format!("could not load commit {oid}"))?;
        let files = commit_files(&repo, &commit)?;
        commits.push(HistoryCommit {
            info: commit_info(&commit),
            files,
        });
    }
    Ok(commits)
}

/// Parent of `rev` against `rev` itself. Merge commits use the first parent,
/// matching `git show`.
pub fn generate_commit(path: &Path, rev: &str) -> anyhow::Result<GeneratedReview> {
    let repo = git2::Repository::open(path)
        .with_context(|| format!("could not open Git repository at {}", path.display()))?;
    let obj = repo
        .revparse_single(rev)
        .with_context(|| format!("could not resolve {rev}"))?;
    let commit = obj
        .peel_to_commit()
        .with_context(|| format!("{rev} is not a commit"))?;
    let new_tree = commit.tree().context("could not read the commit's tree")?;
    let old = commit
        .parent(0)
        .ok()
        .and_then(|parent| parent.tree().ok())
        .map(|tree| tree.id());
    let mut files = render_diff(&repo, old, new_tree.id(), &HashSet::new())?;
    highlight_files(&repo, old, new_tree.id(), &mut files);
    Ok(GeneratedReview {
        files,
        commit: Some(commit_info(&commit)),
    })
}

fn commit_info(commit: &git2::Commit<'_>) -> CommitInfo {
    let oid = commit.id().to_string();
    CommitInfo {
        short: oid.chars().take(7).collect(),
        oid,
        summary: commit.summary().unwrap_or("").to_string(),
        author: commit.author().name().unwrap_or("").to_string(),
        time: commit.time().seconds(),
    }
}

fn commit_files(
    repo: &git2::Repository,
    commit: &git2::Commit<'_>,
) -> anyhow::Result<Vec<CommitFile>> {
    let old_tree = commit.parent(0).ok().map(|parent| parent.tree_id());
    with_diff(repo, old_tree, commit.tree_id(), |diff| {
        let files: RefCell<Vec<CommitFile>> = RefCell::new(Vec::new());
        diff.foreach(
            &mut |delta, _| {
                let file = new_file(&delta, &HashSet::new());
                files.borrow_mut().push(CommitFile {
                    path: file.path,
                    old_path: file.old_path,
                    kind: file.kind,
                    added: 0,
                    removed: 0,
                });
                true
            },
            None,
            None,
            // Counted rather than rendered: the list shows a shape, and the
            // hunks themselves wait until the commit is actually opened.
            Some(&mut |_, _, line| {
                if let Some(file) = files.borrow_mut().last_mut() {
                    match line.origin() {
                        '+' => file.added += 1,
                        '-' => file.removed += 1,
                        _ => {}
                    }
                }
                true
            }),
        )?;
        Ok(files.into_inner())
    })
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

/// Both diff walks want the same options and rename detection; only what
/// they build out of the deltas differs.
fn with_diff<T>(
    repo: &git2::Repository,
    old: Option<git2::Oid>,
    target: git2::Oid,
    build: impl FnOnce(&mut git2::Diff<'_>) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
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
    build(&mut diff)
}

fn render_diff(
    repo: &git2::Repository,
    old: Option<git2::Oid>,
    target: git2::Oid,
    untracked: &HashSet<String>,
) -> anyhow::Result<Vec<FileDiff>> {
    with_diff(repo, old, target, |diff| {
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
                        spans: Vec::new(),
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
    })
}

/// Hangs syntax spans on every line of every file, parsing whole blobs rather
/// than hunks. Each side is highlighted from its own tree, so a removed line is
/// read in the file it was removed from and not in the one that replaced it.
///
/// Every step here is allowed to fail quietly. Highlighting is decoration: a
/// file with no grammar, an unreadable blob, or a parse that gives up all mean
/// plain text, and none of them may cost the operator their review.
fn highlight_files(
    repo: &git2::Repository,
    old_tree: Option<git2::Oid>,
    new_tree: git2::Oid,
    files: &mut [FileDiff],
) {
    for file in files {
        if file.note.is_some() || file.hunks.is_empty() {
            continue;
        }
        let lines = || file.hunks.iter().flat_map(|hunk| &hunk.lines);
        // A pure addition has no old side to read, and a deletion no new one.
        // Skipping the parse is worth the two passes over the lines.
        let wants_old = lines().any(|line| line.kind == LineKind::Removed);
        let wants_new = lines().any(|line| line.kind != LineKind::Removed);

        let new_spans = wants_new
            .then(|| blob_spans(repo, Some(new_tree), &file.path, &file.path))
            .flatten();
        let old_path = file.old_path.clone().unwrap_or_else(|| file.path.clone());
        let old_spans = wants_old
            .then(|| blob_spans(repo, old_tree, &old_path, &file.path))
            .flatten();

        for line in file.hunks.iter_mut().flat_map(|hunk| &mut hunk.lines) {
            // Context lines exist on both sides and read the same on both, so
            // the new side answers for everything except an outright removal.
            let (spans, lineno) = match line.kind {
                LineKind::Removed => (old_spans.as_ref(), line.old_lineno),
                _ => (new_spans.as_ref(), line.new_lineno),
            };
            let (Some(spans), Some(lineno)) = (spans, lineno) else {
                continue;
            };
            // Git numbers lines from one. A span reaching past the text the
            // diff carries means the blob and the hunk disagree about this
            // line, and an offset into the wrong line is worse than no colour.
            if let Some(found) = spans.get(lineno.saturating_sub(1) as usize) {
                let width = line.text.len() as u32;
                line.spans = found.iter().copied().filter(|s| s.end <= width).collect();
            }
        }
    }
}

/// Reads one path out of one tree and highlights it. `syntax_path` names the
/// grammar and `blob_path` locates the content, which differ for a rename: the
/// old side sits at the old path but is still the same language.
fn blob_spans(
    repo: &git2::Repository,
    tree: Option<git2::Oid>,
    blob_path: &str,
    syntax_path: &str,
) -> Option<Vec<Vec<argus_protocol::HighlightSpan>>> {
    let tree = repo.find_tree(tree?).ok()?;
    let entry = tree.get_path(Path::new(blob_path)).ok()?;
    let object = entry.to_object(repo).ok()?;
    let blob = object.as_blob()?;
    if blob.is_binary() {
        return None;
    }
    let text = std::str::from_utf8(blob.content()).ok()?;
    crate::highlight::line_spans(syntax_path, text)
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
            (
                "staged.txt",
                "old
",
            ),
            (
                "unstaged.txt",
                "old
",
            ),
            (
                "gone.txt", "old
",
            ),
        ]);
        write(
            &path,
            "staged.txt",
            "staged
",
        );
        let repo = git2::Repository::open(&path).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("staged.txt")).unwrap();
        index.write().unwrap();
        write(
            &path,
            "unstaged.txt",
            "working
",
        );
        write(
            &path, "new.txt", "new
",
        );
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
        let (_dir, path) = repo_with(&[(
            "a.txt", "one
",
        )]);
        write(
            &path, "a.txt", "two
",
        );
        let repo = git2::Repository::open(&path).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        drop(index);
        drop(repo);
        write(
            &path, "a.txt", "three
",
        );

        let added = |review: &GeneratedReview| {
            find(&review.files, "a.txt")
                .hunks
                .iter()
                .flat_map(|hunk| &hunk.lines)
                .filter(|line| line.kind == LineKind::Added)
                .map(|line| line.text.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            added(&generate(&path, ReviewBase::Staged).unwrap()),
            ["two"]
        );
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
        assert_eq!(
            (removed.old_lineno, removed.text.as_str()),
            (Some(2), "two")
        );
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
        assert_eq!(
            find(&review.files, "sub/deep/new.txt").kind,
            ChangeKind::Untracked
        );
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

    /// Highlighting reaches the wire, and each side is read in its own blob.
    /// The removed line here is only valid Rust in the file it was removed
    /// from, so finding its keyword proves the old tree was the one parsed.
    #[test]
    fn diff_lines_carry_syntax_spans_from_their_own_side() {
        let (_dir, path) = repo_with(&[("a.rs", "fn old_name() {}\n")]);
        write(&path, "a.rs", "struct NewThing;\n");

        let review = generate(&path, ReviewBase::Unstaged).unwrap();
        let file = find(&review.files, "a.rs");
        let lines: Vec<&DiffLine> = file.hunks.iter().flat_map(|h| &h.lines).collect();

        let added = lines.iter().find(|l| l.kind == LineKind::Added).unwrap();
        let text = |l: &DiffLine, s: &argus_protocol::HighlightSpan| {
            l.text[s.start as usize..s.end as usize].to_string()
        };
        assert!(
            added.spans.iter().any(|s| text(added, s) == "struct"),
            "added line should be highlighted from the new blob: {:?}",
            added.spans
        );

        let removed = lines.iter().find(|l| l.kind == LineKind::Removed).unwrap();
        assert!(
            removed.spans.iter().any(|s| text(removed, s) == "fn"),
            "removed line should be highlighted from the old blob: {:?}",
            removed.spans
        );

        // Every span has to land inside the text it annotates, or the client
        // will slice a string it was never given.
        for line in lines {
            for span in &line.spans {
                assert!(
                    span.end as usize <= line.text.len(),
                    "span {span:?} overruns {:?}",
                    line.text
                );
            }
        }
    }

    #[test]
    fn a_file_with_no_grammar_is_shipped_without_spans() {
        let (_dir, path) = repo_with(&[("notes.xyz", "one\n")]);
        write(&path, "notes.xyz", "two\n");
        let review = generate(&path, ReviewBase::Unstaged).unwrap();
        let file = find(&review.files, "notes.xyz");
        assert!(file
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .all(|l| l.spans.is_empty()));
    }

    #[test]
    fn history_lists_newest_first_with_the_files_each_commit_touched() {
        let (_dir, path) = repo_with(&[("a.txt", "one\n")]);
        let repo = git2::Repository::open(&path).unwrap();
        write(&path, "a.txt", "two\n");
        write(&path, "b.txt", "new\n");
        commit(&repo, "second");

        let history = list_commits(&path).unwrap();
        assert_eq!(
            history
                .iter()
                .map(|c| c.info.summary.as_str())
                .collect::<Vec<_>>(),
            ["second", "first"]
        );
        assert_eq!(
            history[0]
                .files
                .iter()
                .map(|f| (f.kind, f.path.as_str()))
                .collect::<Vec<_>>(),
            [
                (ChangeKind::Modified, "a.txt"),
                (ChangeKind::Added, "b.txt")
            ]
        );
        assert_eq!(history[1].files[0].path, "a.txt");
        assert_eq!(history[1].files[0].kind, ChangeKind::Added);
    }

    #[test]
    fn a_commit_review_is_that_commit_against_its_parent() {
        let (_dir, path) = repo_with(&[("a.txt", "one\n")]);
        let repo = git2::Repository::open(&path).unwrap();
        write(&path, "a.txt", "two\n");
        commit(&repo, "second");

        let history = list_commits(&path).unwrap();
        let review = generate_commit(&path, &history[0].info.oid).unwrap();
        assert_eq!(review.commit.as_ref().unwrap().summary, "second");
        assert_eq!(find(&review.files, "a.txt").added_lines(), 1);
        assert_eq!(find(&review.files, "a.txt").removed_lines(), 1);
        assert!(
            review.files.iter().all(|f| f.path == "a.txt"),
            "only the second commit's edit: {:?}",
            review.files.iter().map(|f| &f.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_unborn_repository_has_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        assert!(list_commits(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn history_stays_newest_first_past_the_first_two_commits() {
        // Guards the revwalk sorting: a REVERSE in there reads correctly
        // on two commits but hands back the oldest once the cap bites.
        let (_dir, path) = repo_with(&[("a.txt", "0\n")]);
        let repo = git2::Repository::open(&path).unwrap();
        for i in 1..5 {
            write(&path, "a.txt", &format!("{i}\n"));
            commit(&repo, &format!("c{i}"));
        }
        drop(repo);

        let history = list_commits(&path).unwrap();
        assert_eq!(
            history
                .iter()
                .map(|c| c.info.summary.as_str())
                .collect::<Vec<_>>(),
            ["c4", "c3", "c2", "c1", "first"]
        );
    }

    #[test]
    fn a_history_file_carries_the_lines_it_changed() {
        let (_dir, path) = repo_with(&[("a.txt", "one\n")]);
        let repo = git2::Repository::open(&path).unwrap();
        write(&path, "a.txt", "two\nthree\n");
        commit(&repo, "second");
        drop(repo);

        let history = list_commits(&path).unwrap();
        let file = &history[0].files[0];
        assert_eq!(file.path, "a.txt");
        assert_eq!((file.added, file.removed), (2, 1));
    }

    #[test]
    fn a_root_commit_review_lists_its_files_as_added() {
        let (_dir, path) = repo_with(&[("a.txt", "one\n")]);
        let history = list_commits(&path).unwrap();
        let review = generate_commit(&path, &history[0].info.oid).unwrap();
        assert_eq!(find(&review.files, "a.txt").kind, ChangeKind::Added);
    }

    #[test]
    fn history_of_a_non_repository_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_commits(dir.path()).is_err());
    }

    #[test]
    fn a_missing_commit_is_an_error() {
        let (_dir, path) = repo_with(&[("a.txt", "one\n")]);
        assert!(generate_commit(&path, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef").is_err());
    }
}
