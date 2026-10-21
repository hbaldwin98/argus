//! A version's files, which the daemon sends as a flat sorted list, as the
//! tree the Code stage draws: the rows that show given the folders open,
//! each folder marked when something under it changed, and each changed
//! file with how much.

use std::collections::{HashMap, HashSet};

use argus_protocol::CodeChange;

/// One row of the tree as drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    /// A file's path, or a folder's with no trailing slash.
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub folder: bool,
    /// For a folder, whether its rows show.
    pub open: bool,
    /// Something in it changed — the file, or anything under the folder.
    pub changed: bool,
    pub added: u32,
    pub removed: u32,
}

/// The folders of `path`, outermost first: `a`, `a/b` for `a/b/c.rs`.
pub fn folders_of(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/').map(move |(at, _)| &path[..at])
}

/// Folders before files at every level, each in name order whatever the
/// case: the order a person reads a tree in, which the daemon's flat byte
/// order is not (`README.md` sorts before `src/`).
fn folders_first(files: &[String]) -> Vec<&String> {
    let key = |path: &str| -> Vec<(bool, String)> {
        let last = path.matches('/').count();
        path.split('/').enumerate().map(|(i, part)| (i == last, part.to_lowercase())).collect()
    };
    let mut ordered: Vec<&String> = files.iter().collect();
    ordered.sort_by_cached_key(|path| key(path));
    ordered
}

/// The rows that show of `files` with `open` folders open. With `changed`
/// only, the files a version changed are listed under their folders, every
/// one of which shows open: a short list is read whole.
pub fn rows(files: &[String], changes: &[CodeChange], open: &HashSet<String>, changed_only: bool) -> Vec<TreeRow> {
    let by_path: HashMap<&str, &CodeChange> = changes.iter().map(|c| (c.path.as_str(), c)).collect();
    let changed_folders: HashSet<&str> = changes.iter().flat_map(|c| folders_of(&c.path)).collect();
    let mut rows = Vec::new();
    let mut listed: HashSet<&str> = HashSet::new();
    for file in folders_first(files) {
        let change = by_path.get(file.as_str());
        if changed_only && change.is_none() {
            continue;
        }
        // A file shows when every folder above it is open.
        let mut shown = true;
        for (depth, folder) in folders_of(file).enumerate() {
            if !shown {
                break;
            }
            let is_open = changed_only || open.contains(folder);
            if listed.insert(folder) {
                rows.push(TreeRow {
                    path: folder.to_owned(),
                    name: folder.rsplit('/').next().unwrap_or(folder).to_owned(),
                    depth,
                    folder: true,
                    open: is_open,
                    changed: changed_folders.contains(folder),
                    added: 0,
                    removed: 0,
                });
            }
            shown = is_open;
        }
        if shown {
            rows.push(TreeRow {
                path: file.to_string(),
                name: file.rsplit('/').next().unwrap_or(file).to_owned(),
                depth: folders_of(file).count(),
                folder: false,
                open: false,
                changed: change.is_some(),
                added: change.map_or(0, |c| c.added),
                removed: change.map_or(0, |c| c.removed),
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    fn change(path: &str, added: u32, removed: u32) -> CodeChange {
        CodeChange { path: path.into(), added, removed }
    }

    fn shown(rows: &[TreeRow]) -> Vec<String> {
        rows.iter()
            .map(|r| format!("{}{}{}", "  ".repeat(r.depth), r.name, if r.folder { "/" } else { "" }))
            .collect()
    }

    #[test]
    fn closed_folders_hide_what_is_under_them() {
        let all = files(&["README.md", "src/a.rs", "src/deep/b.rs"]);
        let open = HashSet::new();
        assert_eq!(shown(&rows(&all, &[], &open, false)), ["src/", "README.md"]);
        let open: HashSet<String> = ["src".to_string()].into();
        assert_eq!(shown(&rows(&all, &[], &open, false)), ["src/", "  deep/", "  a.rs", "README.md"]);
    }

    #[test]
    fn folders_come_before_files_at_every_level_whatever_the_case() {
        let all = files(&["Zed.md", "alpha.rs", "src/b.rs", "src/Z/c.rs", "Src2/x"]);
        let open: HashSet<String> = ["src".to_string(), "src/Z".to_string()].into();
        assert_eq!(
            shown(&rows(&all, &[], &open, false)),
            ["src/", "  Z/", "    c.rs", "  b.rs", "Src2/", "alpha.rs", "Zed.md"]
        );
    }

    #[test]
    fn a_folder_with_a_change_under_it_says_so_and_a_file_says_how_much() {
        let all = files(&["src/deep/b.rs", "src/z.rs"]);
        let open: HashSet<String> = ["src".to_string(), "src/deep".to_string()].into();
        let rows = rows(&all, &[change("src/deep/b.rs", 3, 1)], &open, false);
        let changed: Vec<(&str, bool, u32)> = rows.iter().map(|r| (r.path.as_str(), r.changed, r.added)).collect();
        assert_eq!(
            changed,
            [("src", true, 0), ("src/deep", true, 0), ("src/deep/b.rs", true, 3), ("src/z.rs", false, 0)]
        );
    }

    #[test]
    fn changed_only_lists_the_changed_files_under_open_folders() {
        let all = files(&["a.rs", "src/b.rs", "src/c.rs"]);
        let rows = rows(&all, &[change("src/c.rs", 1, 0)], &HashSet::new(), true);
        assert_eq!(shown(&rows), ["src/", "  c.rs"]);
        assert!(rows[0].open);
    }

    #[test]
    fn a_files_folders_are_outermost_first() {
        assert_eq!(folders_of("a/b/c.rs").collect::<Vec<_>>(), ["a", "a/b"]);
        assert_eq!(folders_of("c.rs").count(), 0);
    }
}
