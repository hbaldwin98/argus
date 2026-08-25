//! Listing what a checkout contains, for the fuzzy pickers: its branches
//! and its files — and, for the directory browser, what a directory
//! anywhere on disk contains.
//!
//! Both are in-process. Branches come from libgit2; files from `ignore`,
//! the crate ripgrep and fd are built on, so `.gitignore` is honoured with
//! the same rules the user already expects. Shelling out to `fd` or `rg`
//! would mean a console window per invocation on Windows, where the daemon
//! owns no console — the bug `git::list_worktrees` documents — and would
//! make both features depend on tools that may not be installed.

use argus_protocol::{DirEntry, DirListing};
use std::path::{Path, PathBuf};

/// Files bigger than a picker can usefully show. A repo past this is
/// almost certainly one where the user wants a narrower query anyway.
const MAX_FILES: usize = 50_000;

/// Local branches, current one first, then alphabetically. Empty if `path`
/// isn't a repo.
pub fn branches(path: &Path) -> Vec<String> {
    let Ok(repo) = git2::Repository::open(path) else {
        return Vec::new();
    };
    let current = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().map(str::to_string))
        .filter(|s| s != "HEAD");

    let Ok(iter) = repo.branches(Some(git2::BranchType::Local)) else {
        return current.into_iter().collect();
    };
    let mut names: Vec<String> = iter
        .flatten()
        .filter_map(|(b, _)| b.name().ok().flatten().map(str::to_string))
        .filter(|n| Some(n) != current.as_ref())
        .collect();
    names.sort();

    // The branch you are on goes first: it is the one you are most likely
    // to be looking at, and never the one you want to switch to.
    current.into_iter().chain(names).collect()
}

/// Every file in the checkout that git would not ignore, repo-relative and
/// forward-slashed. Directories are omitted — you cannot open one.
pub fn files(path: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let walk = ignore::WalkBuilder::new(path)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .filter_entry(|e| e.file_name() != ".git")
        .build();

    for entry in walk.flatten() {
        if out.len() >= MAX_FILES {
            break;
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if let Ok(rel) = entry.path().strip_prefix(path) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    out.sort();
    out
}

/// Subdirectories of `path`, for the "add project"/"add repository"
/// browser. An empty `path` starts at the user's home directory, which is
/// where checkouts almost always live — starting at the daemon's cwd would
/// drop the user somewhere they have to climb out of.
///
/// Files are left out: neither a project nor a repository can be one, and
/// a browser full of them is a browser you have to filter before you can
/// use it.
pub fn directories(path: &str) -> DirListing {
    let target = resolve(path);
    let listing = |entries, error| DirListing {
        request_id: 0,
        path: display_path(&target),
        parent: target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(display_path),
        entries,
        error,
    };

    let read = match std::fs::read_dir(&target) {
        Ok(read) => read,
        Err(e) => return listing(Vec::new(), Some(e.to_string())),
    };

    let mut entries: Vec<DirEntry> = read
        .flatten()
        // `file_type` on the entry does not follow symlinks, so a symlinked
        // checkout — a normal way to keep repos on another drive — would
        // otherwise vanish from the browser.
        .filter(|e| e.path().is_dir())
        .filter(|e| e.file_name() != ".git")
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let is_repo = e.path().join(".git").exists();
            DirEntry { name, is_repo }
        })
        .collect();
    // Case-insensitive: `Source` sorting away from `src` is an ordering
    // only a byte comparison would choose.
    entries.sort_by_key(|e| (e.name.to_lowercase(), e.name.clone()));
    listing(entries, None)
}

/// Where an empty or `~`-rooted path points. A relative path is taken
/// against home for the same reason the empty one is.
fn resolve(path: &str) -> PathBuf {
    let home = home_dir();
    let trimmed = path.trim();
    let expanded = match trimmed {
        "" => return home,
        "~" => home,
        p if p.starts_with("~/") || p.starts_with("~\\") => home.join(&p[2..]),
        p => PathBuf::from(p),
    };
    // Canonicalizing collapses the `..` a climb to the parent leaves
    // behind, so the breadcrumb never grows a tail of them.
    expanded
        .canonicalize()
        .map(strip_verbatim)
        .unwrap_or(expanded)
}

fn home_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `canonicalize` without the verbatim `\\?\` prefix Windows puts on the
/// front, which is correct as a path and unreadable as a breadcrumb. UNC
/// paths keep theirs: there the prefix is load-bearing.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().to_string();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
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
        (dir, path)
    }

    fn write(root: &Path, name: &str, body: &str) {
        let p = root.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn branch(path: &Path, name: &str) {
        let repo = git2::Repository::open(path).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch(name, &head, false).unwrap();
    }

    // --- branches -----------------------------------------------------------

    #[test]
    fn a_repo_lists_the_branch_it_is_on() {
        let (_d, path) = repo(&[("a.txt", "one\n")]);
        let list = branches(&path);
        assert_eq!(list.len(), 1);
        assert!(list[0] == "master" || list[0] == "main", "{list:?}");
    }

    #[test]
    fn the_current_branch_comes_first_and_the_rest_are_sorted() {
        // You are switching *away* from the current one, so it is never the
        // answer — but it is the label that tells you where you are.
        let (_d, path) = repo(&[("a.txt", "one\n")]);
        branch(&path, "zebra");
        branch(&path, "alpha");

        let list = branches(&path);
        let here = list[0].clone();
        assert!(here == "master" || here == "main", "{list:?}");
        assert_eq!(&list[1..], &["alpha".to_string(), "zebra".to_string()]);
    }

    #[test]
    fn the_current_branch_is_not_listed_twice() {
        let (_d, path) = repo(&[("a.txt", "one\n")]);
        branch(&path, "other");
        let list = branches(&path);
        let here = &list[0];
        assert_eq!(list.iter().filter(|b| *b == here).count(), 1, "{list:?}");
    }

    #[test]
    fn a_directory_that_is_not_a_repo_has_no_branches() {
        let dir = tempfile::tempdir().unwrap();
        assert!(branches(dir.path()).is_empty());
    }

    // --- files --------------------------------------------------------------

    #[test]
    fn files_are_listed_repo_relative_and_forward_slashed() {
        let (_d, path) = repo(&[("src/deep/a.rs", "x\n"), ("b.txt", "y\n")]);
        assert_eq!(files(&path), vec!["b.txt", "src/deep/a.rs"]);
    }

    #[test]
    fn ignored_files_are_left_out() {
        // The whole reason to use git's own ignore rules: a picker full of
        // `target/` is a picker nobody can use.
        let (_d, path) = repo(&[(".gitignore", "target/\n*.log\n"), ("src/a.rs", "x\n")]);
        write(&path, "target/huge.bin", "x");
        write(&path, "noise.log", "x");

        let list = files(&path);
        assert!(list.contains(&"src/a.rs".to_string()), "{list:?}");
        assert!(!list.iter().any(|f| f.starts_with("target/")), "{list:?}");
        assert!(!list.iter().any(|f| f.ends_with(".log")), "{list:?}");
    }

    #[test]
    fn gits_own_directory_is_never_offered() {
        let (_d, path) = repo(&[("a.txt", "x\n")]);
        let list = files(&path);
        assert!(!list.iter().any(|f| f.starts_with(".git/")), "{list:?}");
    }

    #[test]
    fn a_dotfile_the_user_tracks_is_still_offered() {
        // Hidden is not the same as ignored, and `.gitignore` itself is a
        // file people edit.
        let (_d, path) = repo(&[(".gitignore", "target/\n")]);
        assert!(files(&path).contains(&".gitignore".to_string()));
    }

    #[test]
    fn an_untracked_file_is_offered_too() {
        // You want to open the file you just created at least as often as
        // one you already committed.
        let (_d, path) = repo(&[("a.txt", "x\n")]);
        write(&path, "brand-new.rs", "x");
        assert!(files(&path).contains(&"brand-new.rs".to_string()));
    }

    #[test]
    fn directories_are_not_listed_because_you_cannot_open_one() {
        let (_d, path) = repo(&[("src/a.rs", "x\n")]);
        let list = files(&path);
        assert!(!list.contains(&"src".to_string()), "{list:?}");
    }

    // --- directories --------------------------------------------------------

    fn names(listing: &DirListing) -> Vec<String> {
        listing.entries.iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn only_directories_are_offered_and_they_are_sorted() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["Zebra", "alpha", "beta"] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }
        std::fs::write(dir.path().join("a-file.txt"), "x").unwrap();

        let listing = directories(&dir.path().to_string_lossy());
        // Case-insensitive: `Zebra` sorting away from `alpha` is an order
        // only a byte comparison would choose.
        assert_eq!(names(&listing), vec!["alpha", "beta", "Zebra"]);
    }

    #[test]
    fn a_repository_is_marked_as_one() {
        // The difference between a project root and a repository, and the
        // thing you cannot see from the name.
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("checkout");
        std::fs::create_dir(&inner).unwrap();
        git2::Repository::init(&inner).unwrap();
        std::fs::create_dir(dir.path().join("plain")).unwrap();

        let listing = directories(&dir.path().to_string_lossy());
        let by_name = |n: &str| {
            listing
                .entries
                .iter()
                .find(|e| e.name == n)
                .unwrap()
                .is_repo
        };
        assert!(by_name("checkout"));
        assert!(!by_name("plain"));
    }

    #[test]
    fn gits_own_directory_is_never_offered_to_browse_into() {
        let (_d, path) = repo(&[("a.txt", "x\n")]);
        let listing = directories(&path.to_string_lossy());
        assert!(!names(&listing).contains(&".git".to_string()), "{listing:?}");
    }

    #[test]
    fn the_listing_carries_the_parent_to_climb_to() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("child");
        std::fs::create_dir(&inner).unwrap();

        let listing = directories(&inner.to_string_lossy());
        let parent = listing.parent.expect("a child has a parent");
        assert!(
            listing.path.ends_with("child"),
            "{}",
            listing.path
        );
        assert!(parent.len() < listing.path.len(), "{parent} vs {}", listing.path);
    }

    #[test]
    fn an_empty_path_starts_somewhere_that_exists() {
        // The client sends no path on open: only the daemon knows what it
        // can see, and dropping the user nowhere would be worse than a
        // text box.
        let listing = directories("");
        assert!(listing.error.is_none(), "{listing:?}");
        assert!(!listing.path.is_empty());
    }

    #[test]
    fn a_directory_that_is_not_there_says_why_rather_than_looking_empty() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("never-existed");

        let listing = directories(&gone.to_string_lossy());
        assert!(listing.entries.is_empty());
        assert!(listing.error.is_some(), "{listing:?}");
        // The path still comes back, so the browser can say where it is
        // stuck and the user can climb out.
        assert!(listing.path.ends_with("never-existed"), "{}", listing.path);
    }

    #[test]
    fn a_dot_dot_in_the_path_is_collapsed_away() {
        // Climbing leaves them behind, and a breadcrumb growing a tail of
        // `..` reads like a bug even when it resolves.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        let climbed = dir.path().join("a").join("..");

        let listing = directories(&climbed.to_string_lossy());
        assert!(!listing.path.contains(".."), "{}", listing.path);
        assert_eq!(names(&listing), vec!["a"]);
    }

    #[test]
    fn a_tilde_means_the_home_directory() {
        assert_eq!(directories("~").path, directories("").path);
    }

    #[test]
    fn listing_a_directory_that_is_not_a_repo_still_works() {
        // A checkout can point anywhere; there is no reason to fail here.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("loose.txt"), "x").unwrap();
        assert_eq!(files(dir.path()), vec!["loose.txt"]);
    }
}
