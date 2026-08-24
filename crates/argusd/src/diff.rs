//! Enumerating a checkout's uncommitted work for the review viewer
//! (DESIGN.md §9 M4). Read-only, in-process libgit2 for the same reason
//! `git::list_worktrees` is — see the note there about console windows.

use std::cell::RefCell;
use std::path::Path;

use argus_protocol::{ChangeKind, DiffLine, FileDiff, Hunk, LineKind, ReviewBase};

/// Past this a file is reported as changed but not rendered — nobody reads
/// a 20k-line diff, and shipping it is pure waste.
const MAX_LINES_PER_FILE: usize = 5_000;

const BINARY_NOTE: &str = "binary file";
const TOO_LARGE_NOTE: &str = "too large to display";

/// Every change in the working tree at `path` against `HEAD`, untracked
/// files included. Empty when `path` isn't a repo — not worth an error.
pub fn working_tree(path: &Path, base: ReviewBase) -> Vec<FileDiff> {
    let Ok(repo) = git2::Repository::open(path) else {
        return Vec::new();
    };

    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true)
        // Without this an untracked *directory* collapses to a single entry
        // for the directory itself, which is not something you can review.
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .context_lines(3);

    // Against HEAD's tree, or against nothing at all in a repo whose first
    // commit hasn't happened yet — where every file is legitimately new.
    let base_tree = base_tree(&repo, base);

    let Ok(diff) = repo.diff_tree_to_workdir_with_index(base_tree.as_ref(), Some(&mut opts)) else {
        return Vec::new();
    };

    // `foreach` holds all three callbacks at once, though their borrows
    // never actually overlap.
    let files: RefCell<Vec<FileDiff>> = RefCell::new(Vec::new());
    let _ = diff.foreach(
        &mut |delta, _| {
            files.borrow_mut().push(new_file(&delta));
            true
        },
        None,
        Some(&mut |_, hunk| {
            if let Some(file) = files.borrow_mut().last_mut() {
                if file.note.is_none() {
                    file.hunks.push(Hunk {
                        header: String::from_utf8_lossy(hunk.header()).trim_end().to_string(),
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
                // Headers, and "no newline at end of file".
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
    );

    files.into_inner()
}

/// The tree the working directory is compared against. `None` in a repo
/// whose first commit hasn't happened, where every file is legitimately new.
fn base_tree(repo: &git2::Repository, base: ReviewBase) -> Option<git2::Tree<'_>> {
    let head = repo.head().ok()?;
    match base {
        ReviewBase::WorkingTree => head.peel_to_tree().ok(),
        ReviewBase::BranchPoint => {
            // With no fork point — on the default branch, typically — fall
            // back to HEAD. An empty diff would read as "this branch
            // changed nothing", which is a different claim.
            let fork = || {
                let mine = head.peel_to_commit().ok()?.id();
                let other = fork_candidate(repo, &head)?;
                let base = repo.merge_base(mine, other).ok()?;
                repo.find_commit(base).ok()?.tree().ok()
            };
            fork().or_else(|| head.peel_to_tree().ok())
        }
    }
}

/// What this branch forked from: its upstream if it has one, else whichever
/// of the usual default branches exists and isn't the branch itself.
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

fn new_file(delta: &git2::DiffDelta) -> FileDiff {
    // Only a deletion leaves the new side empty.
    let path = delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(slashed)
        .unwrap_or_default();

    let kind = match delta.status() {
        git2::Delta::Added => ChangeKind::Added,
        git2::Delta::Deleted => ChangeKind::Deleted,
        git2::Delta::Renamed => ChangeKind::Renamed,
        git2::Delta::Untracked => ChangeKind::Untracked,
        // Typechange has no better label, and reads as a modification.
        _ => ChangeKind::Modified,
    };

    let old_path = (kind == ChangeKind::Renamed)
        .then(|| delta.old_file().path().map(slashed))
        .flatten();

    FileDiff {
        path,
        old_path,
        kind,
        hunks: Vec::new(),
        note: delta
            .flags()
            .is_binary()
            .then(|| BINARY_NOTE.to_string()),
    }
}

/// Forward slashes everywhere, so a quoted path is one an agent can use.
fn slashed(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
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
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "first", &tree, &[])
            .unwrap();
        drop(tree);
        drop(index);
        (dir, path)
    }

    fn write(root: &Path, name: &str, body: &str) {
        let p = root.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn find<'a>(files: &'a [FileDiff], path: &str) -> &'a FileDiff {
        files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("no {path:?} in {:?}", paths(files)))
    }

    fn paths(files: &[FileDiff]) -> Vec<&str> {
        files.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn a_clean_checkout_has_nothing_to_review() {
        let (_d, path) = repo_with(&[("a.txt", "one\n")]);
        assert!(working_tree(&path, ReviewBase::WorkingTree).is_empty());
    }

    #[test]
    fn a_directory_that_is_not_a_repo_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(working_tree(dir.path(), ReviewBase::WorkingTree).is_empty());
    }

    #[test]
    fn an_edited_file_reports_the_lines_on_both_sides() {
        let (_d, path) = repo_with(&[("a.txt", "one\ntwo\nthree\n")]);
        write(&path, "a.txt", "one\nTWO\nthree\n");

        let files = working_tree(&path, ReviewBase::WorkingTree);
        let f = find(&files, "a.txt");
        assert_eq!(f.kind, ChangeKind::Modified);
        assert_eq!((f.added_lines(), f.removed_lines()), (1, 1));

        let lines = &f.hunks[0].lines;
        let removed = lines.iter().find(|l| l.kind == LineKind::Removed).unwrap();
        assert_eq!(removed.text, "two");
        assert_eq!(removed.old_lineno, Some(2), "a comment needs the old side");
        let added = lines.iter().find(|l| l.kind == LineKind::Added).unwrap();
        assert_eq!(added.text, "TWO");
        assert_eq!(added.new_lineno, Some(2));
    }

    #[test]
    fn line_text_carries_no_diff_marker_or_newline() {
        // The client draws the marker from `kind`; baking it into the text
        // would double it up and break any comment that quotes the line.
        let (_d, path) = repo_with(&[("a.txt", "one\n")]);
        write(&path, "a.txt", "two\n");
        for line in &working_tree(&path, ReviewBase::WorkingTree)[0].hunks[0].lines {
            assert!(!line.text.starts_with(['+', '-']), "{line:?}");
            assert!(!line.text.ends_with('\n'), "{line:?}");
        }
    }

    #[test]
    fn an_untracked_file_is_reviewable_with_its_contents() {
        // The case most likely to be forgotten before a commit, so it must
        // not be silently absent.
        let (_d, path) = repo_with(&[("a.txt", "one\n")]);
        write(&path, "new.txt", "hello\n");

        let files = working_tree(&path, ReviewBase::WorkingTree);
        let f = find(&files, "new.txt");
        assert_eq!(f.kind, ChangeKind::Untracked);
        assert_eq!(f.added_lines(), 1);
        assert_eq!(f.hunks[0].lines[0].text, "hello");
    }

    #[test]
    fn untracked_directories_are_listed_as_their_files() {
        // libgit2's default collapses these to the directory, which is not
        // something you can review.
        let (_d, path) = repo_with(&[("a.txt", "one\n")]);
        write(&path, "sub/deep/x.txt", "x\n");

        let files = working_tree(&path, ReviewBase::WorkingTree);
        assert_eq!(paths(&files), vec!["sub/deep/x.txt"]);
    }

    #[test]
    fn paths_are_forward_slashed_so_an_agent_can_use_them_verbatim() {
        let (_d, path) = repo_with(&[("sub/a.txt", "one\n")]);
        write(&path, "sub/a.txt", "two\n");
        assert_eq!(paths(&working_tree(&path, ReviewBase::WorkingTree)), vec!["sub/a.txt"]);
    }

    #[test]
    fn a_deleted_file_is_reported_under_the_path_it_had() {
        let (_d, path) = repo_with(&[("a.txt", "one\n"), ("b.txt", "b\n")]);
        std::fs::remove_file(path.join("a.txt")).unwrap();

        let files = working_tree(&path, ReviewBase::WorkingTree);
        let f = find(&files, "a.txt");
        assert_eq!(f.kind, ChangeKind::Deleted);
        assert_eq!(f.removed_lines(), 1);
    }

    #[test]
    fn a_binary_file_is_listed_but_not_rendered() {
        let (_d, path) = repo_with(&[("a.txt", "one\n")]);
        std::fs::write(path.join("blob.bin"), [0u8, 159, 146, 150, 0]).unwrap();

        let files = working_tree(&path, ReviewBase::WorkingTree);
        let f = find(&files, "blob.bin");
        assert_eq!(
            f.note.as_deref(),
            Some(BINARY_NOTE),
            "and not an empty, apparently-unchanged file"
        );
        assert!(f.hunks.is_empty());
    }

    #[test]
    fn a_huge_file_is_listed_with_a_note_instead_of_its_lines() {
        let (_d, path) = repo_with(&[("a.txt", "one\n")]);
        let big: String = (0..MAX_LINES_PER_FILE + 10)
            .map(|i| format!("line {i}\n"))
            .collect();
        write(&path, "big.txt", &big);

        let f = &working_tree(&path, ReviewBase::WorkingTree)[0];
        assert_eq!(f.path, "big.txt");
        assert_eq!(f.note.as_deref(), Some(TOO_LARGE_NOTE));
        assert!(f.hunks.is_empty(), "the point is not to ship it");
    }

    #[test]
    fn several_changed_files_all_appear() {
        let (_d, path) = repo_with(&[("a.txt", "a\n"), ("b.txt", "b\n")]);
        write(&path, "a.txt", "A\n");
        write(&path, "b.txt", "B\n");
        write(&path, "c.txt", "C\n");

        let files = working_tree(&path, ReviewBase::WorkingTree);
        assert_eq!(paths(&files), vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn hunks_keep_gits_own_header() {
        // A comment that quotes it should match what `git diff` would show.
        let (_d, path) = repo_with(&[("a.txt", "one\ntwo\nthree\n")]);
        write(&path, "a.txt", "one\nTWO\nthree\n");
        let header = &working_tree(&path, ReviewBase::WorkingTree)[0].hunks[0].header;
        assert!(header.starts_with("@@"), "{header:?}");
        assert!(!header.ends_with('\n'));
    }

    #[test]
    fn a_repo_with_no_commits_yet_treats_everything_as_new() {
        let dir = tempfile::tempdir().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();

        let files = working_tree(dir.path(), ReviewBase::WorkingTree);
        assert_eq!(paths(&files), vec!["a.txt"]);
        assert_eq!(files[0].added_lines(), 1);
    }
    // --- diff bases ---------------------------------------------------------

    /// Commits `files` on a new branch off the current HEAD.
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
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "work", &tree, &[&head])
            .unwrap();
    }

    #[test]
    fn the_working_tree_base_ignores_what_the_branch_already_committed() {
        let (_d, path) = repo_with(&[("a.txt", "one\n")]);
        commit_on_branch(&path, "feature", &[("b.txt", "committed\n")]);

        assert!(
            working_tree(&path, ReviewBase::WorkingTree).is_empty(),
            "committed work is not uncommitted work"
        );
    }

    #[test]
    fn the_branch_point_base_shows_everything_the_branch_did() {
        let (_d, path) = repo_with(&[("a.txt", "one\n")]);
        commit_on_branch(&path, "feature", &[("b.txt", "committed\n")]);
        write(&path, "c.txt", "uncommitted\n");

        let files = working_tree(&path, ReviewBase::BranchPoint);
        let mut names = paths(&files);
        names.sort();
        assert_eq!(
            names,
            vec!["b.txt", "c.txt"],
            "both what it committed and what it hasn't"
        );
    }

    #[test]
    fn a_branch_with_no_fork_point_falls_back_to_uncommitted_work() {
        // On the default branch there is nothing to have forked from, and
        // an empty diff would read as "this branch changed nothing".
        let (_d, path) = repo_with(&[("a.txt", "one\n")]);
        write(&path, "a.txt", "two\n");

        let files = working_tree(&path, ReviewBase::BranchPoint);
        assert_eq!(paths(&files), vec!["a.txt"]);
    }

}
