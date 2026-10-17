//! A checkout at one version, as the Code stage reads it: every file and how
//! much each changed against the version's base, and one file's lines with
//! the ones that version changed, what was removed between them, and what
//! the file defines. Read-only, and nothing here writes to the object
//! database: the working tree is read off disk, the rest out of trees Git
//! already holds.

use std::path::Path;

use anyhow::{bail, Context};
use argus_protocol::{is_checkout_relative, CodeChange, CodeLine, CodeSymbol, CodeVersion, Removed};

/// Files larger than this are named but not read, as a review lists them.
const MAX_FILE_BYTES: usize = 1024 * 1024;
/// How far into a file a NUL marks it binary: Git's own heuristic.
const BINARY_SNIFF: usize = 8000;
const BINARY_NOTE: &str = "binary file";
const TOO_LARGE_NOTE: &str = "too large to display";

/// One file at one version.
pub struct ReadFile {
    pub lines: Vec<CodeLine>,
    pub removed: Vec<Removed>,
    pub symbols: Vec<CodeSymbol>,
    pub note: Option<String>,
}

fn open(path: &Path) -> anyhow::Result<git2::Repository> {
    git2::Repository::open(path).with_context(|| format!("could not open Git repository at {}", path.display()))
}

fn head_tree(repo: &git2::Repository) -> Option<git2::Tree<'_>> {
    repo.head().ok()?.peel_to_tree().ok()
}

fn commit<'r>(repo: &'r git2::Repository, oid: &str) -> anyhow::Result<git2::Commit<'r>> {
    repo.revparse_single(oid)
        .with_context(|| format!("could not resolve {oid}"))?
        .peel_to_commit()
        .with_context(|| format!("{oid} is not a commit"))
}

/// What a version is shown against: `HEAD` for the working tree and the
/// index, a commit's first parent for a commit, and nothing for `HEAD`.
fn base_tree<'r>(repo: &'r git2::Repository, version: &CodeVersion) -> anyhow::Result<Option<git2::Tree<'r>>> {
    Ok(match version {
        CodeVersion::WorkingTree | CodeVersion::Index => head_tree(repo),
        CodeVersion::Head => None,
        CodeVersion::Commit(oid) => commit(repo, oid)?.parent(0).ok().and_then(|p| p.tree().ok()),
    })
}

/// Every file of `version`, sorted, and the ones it changed.
pub fn list(root: &Path, version: &CodeVersion) -> anyhow::Result<(Vec<String>, Vec<CodeChange>)> {
    let repo = open(root)?;
    let mut files = match version {
        // Untracked files are part of what is on disk, as review counts them.
        CodeVersion::WorkingTree => crate::git::browse::files(root),
        CodeVersion::Index => {
            let index = repo.index().context("could not read the index")?;
            index.iter().map(|e| String::from_utf8_lossy(&e.path).into_owned()).collect()
        }
        CodeVersion::Head => match head_tree(&repo) {
            Some(tree) => tree_files(&tree),
            None => Vec::new(),
        },
        CodeVersion::Commit(oid) => tree_files(&commit(&repo, oid)?.tree()?),
    };
    files.sort();
    files.dedup();
    Ok((files, changes(&repo, version)?))
}

fn tree_files(tree: &git2::Tree<'_>) -> Vec<String> {
    let mut files = Vec::new();
    let _ = tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            if let Ok(name) = entry.name() {
                files.push(format!("{dir}{name}"));
            }
        }
        git2::TreeWalkResult::Ok
    });
    files
}

fn changes(repo: &git2::Repository, version: &CodeVersion) -> anyhow::Result<Vec<CodeChange>> {
    let base = base_tree(repo, version)?;
    let mut opts = git2::DiffOptions::new();
    opts.context_lines(0)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
    let diff = match version {
        CodeVersion::WorkingTree => repo.diff_tree_to_workdir_with_index(base.as_ref(), Some(&mut opts))?,
        CodeVersion::Index => repo.diff_tree_to_index(base.as_ref(), None, Some(&mut opts))?,
        CodeVersion::Head => return Ok(Vec::new()),
        CodeVersion::Commit(oid) => {
            let tree = commit(repo, oid)?.tree()?;
            repo.diff_tree_to_tree(base.as_ref(), Some(&tree), Some(&mut opts))?
        }
    };
    let mut out = Vec::new();
    for (i, delta) in diff.deltas().enumerate() {
        let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) else {
            continue;
        };
        let (added, removed) = match git2::Patch::from_diff(&diff, i) {
            Ok(Some(patch)) => patch.line_stats().map(|(_, a, r)| (a as u32, r as u32)).unwrap_or((0, 0)),
            _ => (0, 0),
        };
        // Git calls a file modified when only its line endings moved — a
        // checkout with autocrlf, a tool that rewrote it — and the patch it
        // makes of it, read through the same filters, changes no line. That
        // is not a change anyone reading the code would call one.
        if delta.status() == git2::Delta::Modified && added == 0 && removed == 0 {
            continue;
        }
        out.push(CodeChange { path: path.to_string_lossy().replace('\\', "/"), added, removed });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// The bytes of `file` at `version`, or None where that version has no
/// such file.
fn content(repo: &git2::Repository, root: &Path, file: &str, version: &CodeVersion) -> anyhow::Result<Option<Vec<u8>>> {
    let blob = |tree: Option<git2::Tree<'_>>| -> Option<Vec<u8>> {
        let entry = tree?.get_path(Path::new(file)).ok()?;
        Some(repo.find_blob(entry.id()).ok()?.content().to_vec())
    };
    Ok(match version {
        CodeVersion::WorkingTree => match std::fs::read(root.join(file)) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error).with_context(|| format!("could not read {file}")),
        },
        CodeVersion::Index => {
            let index = repo.index().context("could not read the index")?;
            index
                .get_path(Path::new(file), 0)
                .and_then(|entry| repo.find_blob(entry.id).ok())
                .map(|b| b.content().to_vec())
        }
        CodeVersion::Head => blob(head_tree(repo)),
        CodeVersion::Commit(oid) => blob(Some(commit(repo, oid)?.tree()?)),
    })
}

/// `file` at `version`: its lines highlighted, the ones the version added
/// or changed marked, where it removed lines, and what it defines.
pub fn read(root: &Path, file: &str, version: &CodeVersion) -> anyhow::Result<ReadFile> {
    if !is_checkout_relative(file) {
        bail!("{file} is not a path inside the checkout");
    }
    let repo = open(root)?;
    let Some(bytes) = content(&repo, root, file, version)? else {
        bail!("{file} is not in this version");
    };
    let unreadable = |note: &str| ReadFile {
        lines: Vec::new(),
        removed: Vec::new(),
        symbols: Vec::new(),
        note: Some(note.to_owned()),
    };
    if bytes.len() > MAX_FILE_BYTES {
        return Ok(unreadable(TOO_LARGE_NOTE));
    }
    if bytes[..bytes.len().min(BINARY_SNIFF)].contains(&0) {
        return Ok(unreadable(BINARY_NOTE));
    }
    let text = String::from_utf8_lossy(&bytes);
    let spans = crate::syntax::highlight::line_spans(file, &text);
    let mut lines: Vec<CodeLine> = text
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let width = line.len() as u32;
            CodeLine {
                text: line.to_owned(),
                spans: spans
                    .as_ref()
                    .and_then(|s| s.get(i))
                    .map(|s| s.iter().copied().filter(|s| s.end <= width).collect())
                    .unwrap_or_default(),
                added: false,
            }
        })
        .collect();

    let mut removed = Vec::new();
    let base = base_tree(&repo, version)?;
    let had_base = !matches!(version, CodeVersion::Head);
    if had_base {
        let old = base
            .and_then(|tree| tree.get_path(Path::new(file)).ok())
            .and_then(|entry| repo.find_blob(entry.id()).ok())
            .map(|b| b.content().to_vec())
            .unwrap_or_default();
        // A checkout made with autocrlf has CRLF on disk and LF in Git; the
        // lines are read without their endings, so they are compared so too,
        // or every line of an untouched file reads as changed.
        let (old, new) = (unix_endings(&old), unix_endings(&bytes));
        let mut opts = git2::DiffOptions::new();
        opts.context_lines(0);
        let patch = git2::Patch::from_buffers(&old, Some(Path::new(file)), &new, Some(Path::new(file)), Some(&mut opts))
            .context("could not compare the file with its base")?;
        for h in 0..patch.num_hunks() {
            let (hunk, count) = patch.hunk(h)?;
            let mut gone = 0;
            for l in 0..count {
                let line = patch.line_in_hunk(h, l)?;
                match line.origin() {
                    '+' => {
                        if let Some(at) = line.new_lineno().and_then(|n| lines.get_mut(n as usize - 1)) {
                            at.added = true;
                        }
                    }
                    '-' => gone += 1,
                    _ => {}
                }
            }
            if gone > 0 {
                // With no new lines, git numbers the hunk by the line the
                // removal sits after.
                let before = if hunk.new_lines() == 0 { hunk.new_start() + 1 } else { hunk.new_start() };
                removed.push(Removed { before, count: gone });
            }
        }
    }
    Ok(ReadFile { symbols: crate::symbols::of(file, &text), lines, removed, note: None })
}

/// `bytes` with each CRLF a bare LF.
fn unix_endings(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for (i, &b) in bytes.iter().enumerate() {
        if !(b == b'\r' && bytes.get(i + 1) == Some(&b'\n')) {
            out.push(b);
        }
    }
    out
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
        commit_all(&repo, "first");
        (dir, path)
    }

    fn write(root: &Path, name: &str, body: &str) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn stage(root: &Path) {
        let repo = git2::Repository::open(root).unwrap();
        let mut index = repo.index().unwrap();
        index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
    }

    fn commit_all(repo: &git2::Repository, message: &str) -> git2::Oid {
        let mut index = repo.index().unwrap();
        index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents).unwrap()
    }

    fn texts(file: &ReadFile) -> Vec<&str> {
        file.lines.iter().map(|l| l.text.as_str()).collect()
    }

    fn added(file: &ReadFile) -> Vec<usize> {
        file.lines.iter().enumerate().filter(|(_, l)| l.added).map(|(i, _)| i + 1).collect()
    }

    #[test]
    fn the_working_tree_is_read_off_disk_with_its_uncommitted_lines_marked() {
        let (_dir, root) = repo_with(&[("a.rs", "one\ntwo\nthree\n")]);
        write(&root, "a.rs", "one\nTWO\nthree\nfour\n");

        let file = read(&root, "a.rs", &CodeVersion::WorkingTree).unwrap();
        assert_eq!(texts(&file), ["one", "TWO", "three", "four"]);
        assert_eq!(added(&file), [2, 4]);
        assert_eq!(file.removed, [Removed { before: 2, count: 1 }]);
    }

    #[test]
    fn crlf_on_disk_against_lf_in_git_is_not_a_change() {
        // A Windows checkout with autocrlf: every line differs byte for byte
        // and none of them was changed.
        let (_dir, root) = repo_with(&[("a.rs", "one\ntwo\n")]);
        write(&root, "a.rs", "one\r\nTWO\r\n");

        let file = read(&root, "a.rs", &CodeVersion::WorkingTree).unwrap();
        assert_eq!(texts(&file), ["one", "TWO"]);
        assert_eq!(added(&file), [2], "only the line that was edited");
        assert_eq!(file.removed, [Removed { before: 2, count: 1 }]);
    }

    #[test]
    fn a_file_whose_only_change_is_its_line_endings_is_not_listed_as_changed() {
        let (_dir, root) = repo_with(&[("a.rs", "one\ntwo\n"), ("b.rs", "x\n")]);
        let repo = git2::Repository::open(&root).unwrap();
        repo.config().unwrap().set_bool("core.autocrlf", true).unwrap();
        write(&root, "a.rs", "one\r\ntwo\r\n");
        write(&root, "b.rs", "y\r\n");

        let (_, changes) = list(&root, &CodeVersion::WorkingTree).unwrap();
        let paths: Vec<&str> = changes.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["b.rs"], "{changes:?}");
    }

    #[test]
    fn head_has_no_base_so_nothing_in_it_is_marked() {
        let (_dir, root) = repo_with(&[("a.rs", "one\n")]);
        write(&root, "a.rs", "changed\n");

        let file = read(&root, "a.rs", &CodeVersion::Head).unwrap();
        assert_eq!(texts(&file), ["one"], "HEAD, not the working tree");
        assert!(added(&file).is_empty() && file.removed.is_empty());
    }

    #[test]
    fn the_index_is_what_was_staged_not_what_was_typed_since() {
        let (_dir, root) = repo_with(&[("a.rs", "one\n")]);
        write(&root, "a.rs", "staged\n");
        stage(&root);
        write(&root, "a.rs", "typed after\n");

        let file = read(&root, "a.rs", &CodeVersion::Index).unwrap();
        assert_eq!(texts(&file), ["staged"]);
        assert_eq!(added(&file), [1]);
    }

    #[test]
    fn a_commit_is_read_against_its_parent() {
        let (_dir, root) = repo_with(&[("a.rs", "one\ntwo\n")]);
        write(&root, "a.rs", "one\n");
        let repo = git2::Repository::open(&root).unwrap();
        let oid = commit_all(&repo, "drop two").to_string();
        write(&root, "a.rs", "since\n");

        let file = read(&root, "a.rs", &CodeVersion::Commit(oid)).unwrap();
        assert_eq!(texts(&file), ["one"]);
        assert_eq!(file.removed, [Removed { before: 2, count: 1 }], "after the last line");
    }

    #[test]
    fn a_file_is_highlighted_and_outlined() {
        let (_dir, root) = repo_with(&[("a.rs", "fn main() {}\n")]);
        let file = read(&root, "a.rs", &CodeVersion::WorkingTree).unwrap();
        assert!(!file.lines[0].spans.is_empty());
        assert_eq!(file.symbols.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["main"]);
    }

    #[test]
    fn a_binary_or_missing_file_is_said_rather_than_drawn() {
        let (_dir, root) = repo_with(&[("a.bin", "x\0y")]);
        let file = read(&root, "a.bin", &CodeVersion::WorkingTree).unwrap();
        assert_eq!(file.note.as_deref(), Some(BINARY_NOTE));
        assert!(read(&root, "gone.rs", &CodeVersion::WorkingTree).is_err());
    }

    #[test]
    fn a_path_out_of_the_checkout_is_refused_before_anything_is_read() {
        let (_dir, root) = repo_with(&[("a.rs", "x\n")]);
        assert!(read(&root, "../outside", &CodeVersion::WorkingTree).is_err());
        assert!(read(&root, "/etc/passwd", &CodeVersion::WorkingTree).is_err());
    }

    #[test]
    fn a_version_lists_its_files_and_how_much_each_changed() {
        let (_dir, root) = repo_with(&[("a.rs", "one\ntwo\n"), ("src/b.rs", "b\n")]);
        write(&root, "a.rs", "one\nTWO\nthree\n");
        write(&root, "new.rs", "fresh\n");

        let (files, changes) = list(&root, &CodeVersion::WorkingTree).unwrap();
        assert_eq!(files, ["a.rs", "new.rs", "src/b.rs"]);
        assert_eq!(
            changes,
            [
                CodeChange { path: "a.rs".into(), added: 2, removed: 1 },
                CodeChange { path: "new.rs".into(), added: 1, removed: 0 },
            ]
        );

        let (files, changes) = list(&root, &CodeVersion::Head).unwrap();
        assert_eq!(files, ["a.rs", "src/b.rs"], "untracked files are not in HEAD");
        assert!(changes.is_empty());
    }
}
