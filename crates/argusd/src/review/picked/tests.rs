use std::path::{Path, PathBuf};

use super::*;

/// A repository whose HEAD, index and working tree hold `f.txt` as given;
/// `None` leaves the file out of that one.
fn repo(head: Option<&str>, index: Option<&str>, disk: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let repo = git2::Repository::init(&root).unwrap();
    let file = root.join("f.txt");
    let sig = git2::Signature::now("t", "t@example.com").unwrap();
    let mut idx = repo.index().unwrap();
    if let Some(body) = head {
        std::fs::write(&file, body).unwrap();
        idx.add_path(Path::new("f.txt")).unwrap();
    }
    std::fs::write(root.join("keep.txt"), "k\n").unwrap();
    idx.add_path(Path::new("keep.txt")).unwrap();
    let tree = repo.find_tree(idx.write_tree().unwrap()).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "first", &tree, &[]).unwrap();
    match index {
        Some(body) => {
            std::fs::write(&file, body).unwrap();
            idx.add_path(Path::new("f.txt")).unwrap();
        }
        None => {
            let _ = idx.remove_path(Path::new("f.txt"));
        }
    }
    idx.write().unwrap();
    match disk {
        Some(body) => std::fs::write(&file, body).unwrap(),
        None => {
            let _ = std::fs::remove_file(&file);
        }
    }
    (dir, root)
}

fn added(lineno: u32, text: &str) -> PickedLine {
    PickedLine { kind: LineKind::Added, lineno, text: text.into() }
}

fn removed(lineno: u32, text: &str) -> PickedLine {
    PickedLine { kind: LineKind::Removed, lineno, text: text.into() }
}

/// What the index would hold for `f.txt` after the pick.
fn staged(root: &Path, from: ReviewBase, lines: &[PickedLine]) -> Option<String> {
    let picked = pick(root, from, "f.txt", lines).unwrap();
    let repo = git2::Repository::open(root).unwrap();
    picked
        .entry
        .map(|(_, oid)| String::from_utf8(repo.find_blob(oid).unwrap().content().to_vec()).unwrap())
}

#[test]
fn one_added_line_of_two_is_all_the_index_takes() {
    let (_dir, root) = repo(Some("a\nb\n"), Some("a\nb\n"), Some("a\nx\nb\ny\n"));
    assert_eq!(staged(&root, ReviewBase::Unstaged, &[added(2, "x")]).as_deref(), Some("a\nx\nb\n"));
}

#[test]
fn a_picked_removal_leaves_the_index_without_the_line() {
    let (_dir, root) = repo(Some("a\nb\nc\n"), Some("a\nb\nc\n"), Some("a\nc\n"));
    assert_eq!(staged(&root, ReviewBase::Unstaged, &[removed(2, "b")]).as_deref(), Some("a\nc\n"));
}

#[test]
fn the_new_half_of_a_replacement_alone_keeps_the_old_line_beside_it() {
    // What `git add -p` makes of the same pick: the old line is still
    // there, as nobody took its removal.
    let (_dir, root) = repo(Some("a\nb\nc\n"), Some("a\nb\nc\n"), Some("a\nB\nc\n"));
    assert_eq!(staged(&root, ReviewBase::Unstaged, &[added(2, "B")]).as_deref(), Some("a\nb\nB\nc\n"));
}

#[test]
fn lines_between_far_apart_hunks_stay_as_the_index_had_them() {
    let old: String = (1..=30).map(|n| format!("{n}\n")).collect();
    let new = old.replace("2\n3\n", "2\nTWO\n3\n").replace("28\n", "28\nEIGHT\n");
    let (_dir, root) = repo(Some(&old), Some(&old), Some(&new));
    let want = old.replace("28\n", "28\nEIGHT\n");
    assert_eq!(staged(&root, ReviewBase::Unstaged, &[added(30, "EIGHT")]), Some(want));
}

#[test]
fn a_picked_line_off_the_staged_side_is_given_back() {
    let (_dir, root) = repo(Some("a\n"), Some("a\nx\ny\n"), Some("a\nx\ny\n"));
    assert_eq!(staged(&root, ReviewBase::Staged, &[added(2, "x")]).as_deref(), Some("a\ny\n"));
}

#[test]
fn a_removal_given_back_puts_the_committed_line_back_in_the_index() {
    let (_dir, root) = repo(Some("a\nb\nc\n"), Some("a\nc\n"), Some("a\nc\n"));
    assert_eq!(staged(&root, ReviewBase::Staged, &[removed(2, "b")]).as_deref(), Some("a\nb\nc\n"));
}

#[test]
fn a_line_that_reads_otherwise_now_is_refused() {
    let (_dir, root) = repo(Some("a\n"), Some("a\n"), Some("a\nx\n"));
    let error = pick(&root, ReviewBase::Unstaged, "f.txt", &[added(2, "was here")]).unwrap_err();
    assert!(error.to_string().contains("changed since it was shown"), "{error}");
}

#[test]
fn a_line_the_diff_no_longer_has_is_refused() {
    let (_dir, root) = repo(Some("a\n"), Some("a\n"), Some("a\nx\n"));
    let error = pick(&root, ReviewBase::Unstaged, "f.txt", &[added(7, "x")]).unwrap_err();
    assert!(error.to_string().contains("changed since it was shown"), "{error}");
}

#[test]
fn an_unchanged_line_and_no_lines_at_all_are_refused() {
    let (_dir, root) = repo(Some("a\n"), Some("a\n"), Some("a\nx\n"));
    let context = PickedLine { kind: LineKind::Context, lineno: 1, text: "a".into() };
    assert!(pick(&root, ReviewBase::Unstaged, "f.txt", &[context]).is_err());
    assert!(pick(&root, ReviewBase::Unstaged, "f.txt", &[]).is_err());
}

#[test]
fn part_of_an_untracked_file_enters_the_index_as_a_new_file() {
    let (_dir, root) = repo(None, None, Some("x\ny\n"));
    let picked = pick(&root, ReviewBase::Unstaged, "f.txt", &[added(1, "x")]).unwrap();
    assert_eq!(picked.entry.map(|(mode, _)| mode), Some(0o100644));
    assert_eq!(staged(&root, ReviewBase::Unstaged, &[added(1, "x")]).as_deref(), Some("x\n"));
}

#[test]
fn every_line_of_a_deleted_file_taken_leaves_no_entry() {
    let (_dir, root) = repo(Some("a\nb\n"), Some("a\nb\n"), None);
    assert_eq!(staged(&root, ReviewBase::Unstaged, &[removed(1, "a"), removed(2, "b")]), None);
    assert_eq!(staged(&root, ReviewBase::Unstaged, &[removed(1, "a")]).as_deref(), Some("b\n"));
}

#[test]
fn a_line_added_after_an_unterminated_last_line_starts_on_its_own() {
    let (_dir, root) = repo(Some("a"), Some("a"), Some("a\nb\n"));
    assert_eq!(staged(&root, ReviewBase::Unstaged, &[added(2, "b")]).as_deref(), Some("a\nb\n"));
}

#[test]
fn a_binary_file_is_staged_whole() {
    let (_dir, root) = repo(Some("a\n"), Some("a\n"), Some("a\0\nb\n"));
    let error = pick(&root, ReviewBase::Unstaged, "f.txt", &[added(2, "b")]).unwrap_err();
    assert!(error.to_string().contains("binary"), "{error}");
}

mod properties {
    use proptest::prelude::*;

    use super::*;

    /// Every changed line `old` → `new`'s diff shows, as a pick names it.
    fn changes(old: &[u8], new: &[u8]) -> Vec<PickedLine> {
        let mut opts = git2::DiffOptions::new();
        opts.context_lines(3);
        let patch = git2::Patch::from_buffers(old, None, new, None, Some(&mut opts)).unwrap();
        let mut lines = Vec::new();
        for h in 0..patch.num_hunks() {
            for l in 0..patch.num_lines_in_hunk(h).unwrap() {
                let line = patch.line_in_hunk(h, l).unwrap();
                let text = String::from_utf8_lossy(line.content()).trim_end_matches(['\n', '\r']).to_string();
                match line.origin() {
                    '+' => lines.push(PickedLine { kind: LineKind::Added, lineno: line.new_lineno().unwrap(), text }),
                    '-' => lines.push(PickedLine { kind: LineKind::Removed, lineno: line.old_lineno().unwrap(), text }),
                    _ => {}
                }
            }
        }
        lines
    }

    fn file() -> impl Strategy<Value = String> {
        proptest::collection::vec(prop_oneof!["[a-c]", Just(String::new())], 0..12)
            .prop_map(|lines| lines.iter().map(|l| format!("{l}\n")).collect())
    }

    proptest! {
        /// `IndexChange::Lines` says the picked lines move across and the
        /// rest stay: taking every change staged makes the index the file
        /// on disk, and giving every change back makes it `HEAD`'s.
        #[test]
        fn taking_every_line_is_taking_the_whole_file(old in file(), new in file()) {
            let lines = changes(old.as_bytes(), new.as_bytes());
            prop_assume!(!lines.is_empty());
            let forward = merge(old.as_bytes(), new.as_bytes(), false, "f", &lines).unwrap();
            prop_assert_eq!(String::from_utf8(forward).unwrap(), new.clone());
            let back = merge(old.as_bytes(), new.as_bytes(), true, "f", &lines).unwrap();
            prop_assert_eq!(String::from_utf8(back).unwrap(), old);
        }

        /// Staging some lines and then the rest lands where staging them
        /// all at once does: no pick is lost or made twice.
        #[test]
        fn staging_in_two_picks_is_staging_in_one(old in file(), new in file(), split in any::<proptest::sample::Index>()) {
            let lines = changes(old.as_bytes(), new.as_bytes());
            prop_assume!(lines.len() >= 2);
            let at = 1 + split.index(lines.len() - 1);
            let first = merge(old.as_bytes(), new.as_bytes(), false, "f", &lines[..at]).unwrap();
            let rest = changes(&first, new.as_bytes());
            let second = if rest.is_empty() {
                first
            } else {
                merge(&first, new.as_bytes(), false, "f", &rest).unwrap()
            };
            prop_assert_eq!(String::from_utf8(second).unwrap(), new);
        }
    }
}

/// Git records an executable file's index entry as 100755, so part of a new
/// one staged keeps that mode, as `git add` would give it.
#[cfg(unix)]
#[test]
fn part_of_an_untracked_executable_enters_the_index_executable() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, root) = repo(None, None, Some("x\ny\n"));
    std::fs::set_permissions(root.join("f.txt"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let picked = pick(&root, ReviewBase::Unstaged, "f.txt", &[added(1, "x")]).unwrap();
    assert_eq!(picked.entry.map(|(mode, _)| mode), Some(0o100755));
}
