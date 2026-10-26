use std::path::PathBuf;

use super::*;
use crate::review::diff::generate;

fn repo_with(files: &[(&str, &[u8])]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let repo = git2::Repository::init(&path).unwrap();
    for (name, body) in files {
        write(&path, name, body);
    }
    stage_all(&repo);
    let tree = repo.find_tree(repo.index().unwrap().write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "first", &tree, &[]).unwrap();
    (dir, path)
}

fn write(root: &Path, name: &str, body: &[u8]) {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn stage_all(repo: &git2::Repository) {
    let mut index = repo.index().unwrap();
    index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
    index.update_all(["*"], None).unwrap();
    index.write().unwrap();
}

fn stage(repo_path: &Path, paths: &[&str]) {
    let repo = git2::Repository::open(repo_path).unwrap();
    let mut index = repo.index().unwrap();
    for path in paths {
        if repo_path.join(path).exists() {
            index.add_path(Path::new(path)).unwrap();
        } else {
            index.remove_path(Path::new(path)).unwrap();
        }
    }
    index.write().unwrap();
}

/// Each file of `base` listed and then read, as a lazy viewer comes to it.
fn listed_then_read(path: &Path, base: ReviewBase) -> Vec<FileDiff> {
    let listed = list(path, base).unwrap().files;
    assert!(listed.iter().all(|f| f.pending && f.hunks.is_empty()), "a listing reads nothing");
    listed.iter().map(|f| read(path, base, &f.path, f.old_path.as_deref(), f.kind).unwrap()).collect()
}

/// A checkout with one of everything on both sides: edits, an added, a
/// deleted, an untracked, a moved and a binary file, some staged and some
/// edited again after.
fn every_kind() -> (tempfile::TempDir, PathBuf) {
    let moved: String = (0..40).map(|i| format!("fn line_{i}() {{}}\n")).collect();
    let (dir, path) = repo_with(&[
        ("src/lib.rs", b"fn a() {}\nfn b() {}\nfn c() {}\n"),
        ("src/gone.rs", b"fn gone() {}\n"),
        ("old_name.rs", moved.as_bytes()),
        ("staged.rs", b"one\ntwo\n"),
        ("logo.png", b"\x89PNG\r\n\x1a\n\0\0\0"),
    ]);
    write(&path, "src/lib.rs", b"fn a() {}\nfn b2() {}\nfn c() {}\n");
    std::fs::remove_file(path.join("src/gone.rs")).unwrap();
    std::fs::rename(path.join("old_name.rs"), path.join("new_name.rs")).unwrap();
    write(&path, "notes/new.md", b"# new\n");
    write(&path, "logo.png", b"\x89PNG\r\n\x1a\n\0\0\x01");
    write(&path, "staged.rs", b"one\ntwo\nthree\n");
    write(&path, "added.rs", b"fn added() {}\n");
    stage(&path, &["staged.rs", "added.rs"]);
    write(&path, "staged.rs", b"zero\none\ntwo\nthree\n");
    (dir, path)
}

#[test]
fn each_file_listed_then_read_is_the_file_the_full_review_reads() {
    let (_dir, path) = every_kind();
    for base in [ReviewBase::Unstaged, ReviewBase::Staged] {
        let full = generate(&path, base).unwrap().files;
        assert!(!full.is_empty(), "{base:?} has changes");
        assert_eq!(listed_then_read(&path, base), full, "{base:?}");
    }
}

#[test]
fn a_listing_names_each_change_and_how_it_changed() {
    let (_dir, path) = every_kind();
    let said = |base| {
        let mut said: Vec<(String, ChangeKind, Option<String>)> =
            list(&path, base).unwrap().files.into_iter().map(|f| (f.path, f.kind, f.old_path)).collect();
        said.sort_by(|a, b| a.0.cmp(&b.0));
        said
    };
    let unstaged = said(ReviewBase::Unstaged);
    assert!(unstaged.contains(&("new_name.rs".into(), ChangeKind::Renamed, Some("old_name.rs".into()))), "{unstaged:?}");
    assert!(unstaged.contains(&("notes/new.md".into(), ChangeKind::Untracked, None)), "{unstaged:?}");
    assert!(unstaged.contains(&("src/gone.rs".into(), ChangeKind::Deleted, None)), "{unstaged:?}");
    assert!(unstaged.contains(&("staged.rs".into(), ChangeKind::Modified, None)), "{unstaged:?}");
    let staged = said(ReviewBase::Staged);
    assert_eq!(staged, [("added.rs".into(), ChangeKind::Added, None), ("staged.rs".into(), ChangeKind::Modified, None)]);
}

#[test]
fn listing_an_unstaged_side_writes_nothing_to_the_object_database() {
    let (_dir, path) = every_kind();
    let objects = || walkdir_count(&path.join(".git/objects"));
    let before = objects();
    list(&path, ReviewBase::Unstaged).unwrap();
    assert_eq!(objects(), before, "the full review writes every changed file; a listing writes none");
}

fn walkdir_count(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|e| if e.path().is_dir() { walkdir_count(&e.path()) } else { 1 }).sum())
        .unwrap_or(0)
}

#[test]
fn past_the_rename_limit_a_moved_file_is_listed_as_gone_and_new() {
    let (_dir, path) = every_kind();
    let kinds = |pairs| {
        let files = list_within(&path, ReviewBase::Unstaged, pairs).unwrap().files;
        ["old_name.rs", "new_name.rs"].map(|p| files.iter().find(|f| f.path == p).map(|f| f.kind))
    };
    assert_eq!(kinds(RENAME_PAIRS), [None, Some(ChangeKind::Renamed)], "paired while the pairs are few");
    // Two gone (`src/gone.rs`, `old_name.rs`) and two new (`new_name.rs`, `notes/new.md`).
    assert_eq!(kinds(3), [Some(ChangeKind::Deleted), Some(ChangeKind::Untracked)]);
}

#[test]
fn a_file_gone_since_it_was_listed_reads_as_deleted() {
    let (_dir, path) = repo_with(&[("a.rs", b"fn a() {}\n")]);
    write(&path, "a.rs", b"fn b() {}\n");
    let listed = list(&path, ReviewBase::Unstaged).unwrap().files;
    std::fs::remove_file(path.join("a.rs")).unwrap();
    let read = read(&path, ReviewBase::Unstaged, &listed[0].path, None, listed[0].kind).unwrap();
    let removed: Vec<&str> = read.hunks.iter().flat_map(|h| &h.lines).map(|l| l.text.as_str()).collect();
    assert_eq!(removed, ["fn a() {}"], "what the index holds, against nothing");
    assert_eq!(read.new_blob, None);
}
