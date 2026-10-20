//! One folder of a checkout as it is on disk, with what Git makes of each
//! entry. The Code stage reads versions out of Git; this reads the folder,
//! so untracked and ignored files are in it. Read-only, one level at a
//! time: a folder's own contents are a later question, asked when the
//! person opens it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{bail, Context};
use argus_protocol::{is_checkout_relative, CheckoutEntry, EntryGit, MAX_DIR_ENTRIES};

/// The entries of `path` (relative to `root`, `/`-separated, empty for the
/// root itself), folders first and then files, each by name without regard
/// to case, and whether the folder held more than [`MAX_DIR_ENTRIES`].
pub fn list(root: &Path, path: &str) -> anyhow::Result<(Vec<CheckoutEntry>, bool)> {
    let folder = inside(root, path)?;
    let relative = parts(path);
    let marks = git_marks(root, &relative);

    let mut entries = Vec::new();
    let mut truncated = false;
    for item in std::fs::read_dir(&folder).with_context(|| format!("could not read {}", folder.display()))? {
        let Ok(item) = item else { continue };
        let name = item.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        if entries.len() == MAX_DIR_ENTRIES {
            truncated = true;
            break;
        }
        // A link to a folder is a folder to open; a broken one is a file.
        let meta = std::fs::metadata(item.path()).or_else(|_| item.metadata());
        let Ok(meta) = meta else { continue };
        let dir = meta.is_dir();
        entries.push(CheckoutEntry {
            git: marks.as_ref().and_then(|m| m.get(&name).copied()),
            size: (!dir).then_some(meta.len()),
            modified: meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()),
            dir,
            name,
        });
    }
    entries.sort_by_cached_key(|e| (!e.dir, e.name.to_lowercase(), e.name.clone()));
    Ok((entries, truncated))
}

fn parts(path: &str) -> Vec<&str> {
    path.split('/').filter(|p| !p.is_empty() && *p != ".").collect()
}

/// The folder `path` names, once it is shown to be inside `root` after
/// symlinks are followed: the text check refuses `..` and absolute paths,
/// and a link in the way is caught by comparing where it really leads.
fn inside(root: &Path, path: &str) -> anyhow::Result<PathBuf> {
    if path.is_empty() {
        return Ok(root.to_path_buf());
    }
    if path.contains('\\') || !is_checkout_relative(path) {
        bail!("{path} is not inside the checkout");
    }
    let folder = parts(path).iter().fold(root.to_path_buf(), |p, part| p.join(part));
    let real = folder.canonicalize().with_context(|| format!("no such folder: {path}"))?;
    let base = root.canonicalize().context("the checkout is not on disk")?;
    if !real.starts_with(&base) {
        bail!("{path} leads out of the checkout");
    }
    if !real.is_dir() {
        bail!("{path} is not a folder");
    }
    Ok(folder)
}

/// What Git makes of the direct children of `relative`, by name; `None` when
/// the checkout is not a repository. One status pass limited to the folder,
/// without descending into untracked or ignored folders, so a build
/// directory of thousands of files is one entry.
fn git_marks(root: &Path, relative: &[&str]) -> Option<HashMap<String, EntryGit>> {
    let repo = git2::Repository::open(root).ok()?;
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(false)
        .include_ignored(true)
        .recurse_ignored_dirs(false);
    if !relative.is_empty() {
        options.pathspec(relative.join("/"));
    }
    let statuses = repo.statuses(Some(&mut options)).ok()?;

    let mut marks: HashMap<String, EntryGit> = HashMap::new();
    for entry in statuses.iter() {
        let status = entry.status();
        if status.is_empty() || status == git2::Status::CURRENT {
            continue;
        }
        let Ok(full) = entry.path() else { continue };
        let full: Vec<&str> = full.split('/').filter(|p| !p.is_empty()).collect();
        if full.len() <= relative.len() || full[..relative.len()] != *relative {
            continue;
        }
        let name = full[relative.len()].to_owned();
        let direct = full.len() == relative.len() + 1;
        let mark = if status.is_ignored() {
            // Only the entry git ignored itself is faint; something ignored
            // deeper does not make its folder so.
            if !direct {
                continue;
            }
            EntryGit::Ignored
        } else if direct && status == git2::Status::WT_NEW {
            EntryGit::Untracked
        } else {
            EntryGit::Changed
        };
        let slot = marks.entry(name).or_insert(mark);
        // A folder with something changed under it is changed, whatever else
        // is in it.
        if mark == EntryGit::Changed {
            *slot = mark;
        }
    }
    Some(marks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, name: &str, body: &str) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    /// A repository with `committed` on `HEAD`.
    fn repo(committed: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        for (name, body) in committed {
            write(dir.path(), name, body);
        }
        let mut index = repo.index().unwrap();
        index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let who = git2::Signature::now("t", "t@t").unwrap();
        repo.commit(Some("HEAD"), &who, &who, "start", &tree, &[]).unwrap();
        dir
    }

    fn names(entries: &[CheckoutEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    fn git_of(entries: &[CheckoutEntry], name: &str) -> Option<EntryGit> {
        entries.iter().find(|e| e.name == name).unwrap().git
    }

    #[test]
    fn folders_come_first_then_files_each_by_name_without_regard_to_case() {
        let dir = repo(&[("b.txt", ""), ("Zed/x", ""), ("alpha/x", ""), ("A.txt", "")]);
        let (entries, truncated) = list(dir.path(), "").unwrap();
        assert_eq!(names(&entries), ["alpha", "Zed", "A.txt", "b.txt"]);
        assert!(!truncated);
    }

    #[test]
    fn git_is_never_listed() {
        let dir = repo(&[("a", "")]);
        assert_eq!(names(&list(dir.path(), "").unwrap().0), ["a"]);
    }

    #[test]
    fn changed_untracked_and_ignored_entries_are_marked_and_clean_ones_are_not() {
        let dir = repo(&[("clean.rs", "x"), ("edited.rs", "x"), (".gitignore", "target/\n*.log\n")]);
        write(dir.path(), "edited.rs", "y");
        write(dir.path(), "new.rs", "z");
        write(dir.path(), "target/out", "o");
        write(dir.path(), "run.log", "l");
        let (entries, _) = list(dir.path(), "").unwrap();
        assert_eq!(git_of(&entries, "clean.rs"), None);
        assert_eq!(git_of(&entries, "edited.rs"), Some(EntryGit::Changed));
        assert_eq!(git_of(&entries, "new.rs"), Some(EntryGit::Untracked));
        assert_eq!(git_of(&entries, "target"), Some(EntryGit::Ignored));
        assert_eq!(git_of(&entries, "run.log"), Some(EntryGit::Ignored));
    }

    #[test]
    fn a_folder_is_changed_when_anything_under_it_is_changed_or_untracked() {
        let dir = repo(&[("a/deep/f.rs", "x"), ("b/g.rs", "x"), ("c/h.rs", "x")]);
        write(dir.path(), "a/deep/f.rs", "y");
        write(dir.path(), "b/new.rs", "z");
        let (entries, _) = list(dir.path(), "").unwrap();
        assert_eq!(git_of(&entries, "a"), Some(EntryGit::Changed));
        assert_eq!(git_of(&entries, "b"), Some(EntryGit::Changed));
        assert_eq!(git_of(&entries, "c"), None);
    }

    #[test]
    fn an_untracked_folder_is_one_untracked_entry_and_is_read_on_its_own() {
        let dir = repo(&[("a.rs", "x")]);
        for n in 0..50 {
            write(dir.path(), &format!("build/{n}.o"), "o");
        }
        let (entries, _) = list(dir.path(), "").unwrap();
        assert_eq!(git_of(&entries, "build"), Some(EntryGit::Untracked));
        let (inner, _) = list(dir.path(), "build").unwrap();
        assert_eq!(inner.len(), 50);
    }

    #[test]
    fn a_subfolder_lists_its_own_entries_with_sizes() {
        let dir = repo(&[("src/a.rs", "abc"), ("src/mod/b.rs", "")]);
        let (entries, _) = list(dir.path(), "src").unwrap();
        assert_eq!(names(&entries), ["mod", "a.rs"]);
        assert_eq!(entries[1].size, Some(3));
        assert_eq!(entries[0].size, None);
        assert!(entries[1].modified.is_some());
    }

    #[test]
    fn a_folder_outside_git_is_listed_with_no_marks() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.txt", "x");
        let (entries, _) = list(dir.path(), "").unwrap();
        assert_eq!(names(&entries), ["a.txt"]);
        assert_eq!(entries[0].git, None);
    }

    #[test]
    fn a_path_that_leaves_the_checkout_is_refused() {
        let dir = repo(&[("a/x", "")]);
        for path in ["..", "a/../..", "/etc", "C:/Windows", "a\\..\\.."] {
            assert!(list(dir.path(), path).is_err(), "{path}");
        }
        assert!(list(dir.path(), "nope").is_err());
        assert!(list(dir.path(), "a/x").is_err(), "a file is not a folder");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_that_leads_out_of_the_checkout_is_not_followed() {
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "secret", "s");
        let dir = repo(&[("a", "")]);
        std::os::unix::fs::symlink(outside.path(), dir.path().join("out")).unwrap();
        assert!(list(dir.path(), "out").is_err());
        // The link itself is an entry, a folder to the eye.
        assert!(list(dir.path(), "").unwrap().0.iter().any(|e| e.name == "out" && e.dir));
    }
}
