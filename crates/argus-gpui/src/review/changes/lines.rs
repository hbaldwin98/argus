//! The lines the Changes pane's lists are drawn as, one row of the long list each: which lists
//! show, each list's head, its files flat or as a tree of folders, how tall each line is, and the
//! id that keeps a file's row apart from every other list's.
//!
//! The tree is the real one (`project/GitLists.dc.html`): folders nest, a chain of folders that
//! hold nothing but one folder is a single row (`crates/argus-gpui/src`), and a folder shows how
//! many files are under it. Which folders are closed is the window's own, kept by the list and the
//! folder's whole path.

use std::collections::{BTreeMap, HashSet};

use argus_desktop_core::review::staging::ReviewSection;

use crate::review::changes::flat::FILE_ROW;
use crate::theme::Tokens;

/// The folders closed in the lists: the list's index among the page's, and the folder's path.
pub type Closed = HashSet<(usize, String)>;

/// One line of the lists as drawn: a list's head, the box an empty list shows, a folder over its
/// files, or a file, by its list and its place in it. `depth` is how many folders it is inside
/// when the list is a tree, and none when it is flat, where the row says its own folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Head(usize),
    Empty(usize),
    Folder { list: usize, label: String, path: String, depth: usize, open: bool, files: usize },
    File { list: usize, at: usize, depth: Option<usize> },
}

impl Line {
    /// The list the line is in, as its index among the page's.
    pub fn list(&self) -> usize {
        match self {
            Line::Head(k) | Line::Empty(k) => *k,
            Line::Folder { list, .. } | Line::File { list, .. } => *list,
        }
    }
}

/// The lists shown: staged always (a file is dropped on it), the others while they hold a file,
/// and while finding only those holding a match.
pub fn shown(found: &[ReviewSection], finding: bool) -> Vec<usize> {
    found.iter().enumerate().filter(|(_, s)| (s.side == "staged" && !finding) || !s.files.is_empty()).map(|(k, _)| k).collect()
}

/// A folder of the tree being built: its folders by name, and the files directly in it.
#[derive(Default)]
struct Node {
    dirs: BTreeMap<String, Node>,
    files: Vec<usize>,
}

impl Node {
    fn count(&self) -> usize {
        self.files.len() + self.dirs.values().map(Node::count).sum::<usize>()
    }
}

/// The tree of a list's files, by the folders their `dir` names.
fn grow(section: &ReviewSection) -> Node {
    let mut root = Node::default();
    for (at, file) in section.files.iter().enumerate() {
        let mut node = &mut root;
        for part in file.file.dir.split('/').filter(|part| !part.is_empty()) {
            node = node.dirs.entry(part.to_owned()).or_default();
        }
        node.files.push(at);
    }
    root
}

/// The lines of one folder's insides: its folders in name order, each chain of single folders as
/// one row, then its files in the daemon's order.
fn walk(lines: &mut Vec<Line>, list: usize, node: &Node, at: &str, depth: usize, closed: &Closed) {
    for (name, child) in &node.dirs {
        let (mut label, mut path, mut node) = (name.clone(), format!("{at}{name}/"), child);
        while node.files.is_empty() && node.dirs.len() == 1 {
            let (name, only) = node.dirs.iter().next().expect("one folder");
            label = format!("{label}/{name}");
            path = format!("{path}{name}/");
            node = only;
        }
        let open = !closed.contains(&(list, path.clone()));
        lines.push(Line::Folder { list, label, path: path.clone(), depth, open, files: node.count() });
        if open {
            walk(lines, list, node, &path, depth + 1, closed);
        }
    }
    lines.extend(node.files.iter().map(|&at| Line::File { list, at, depth: Some(depth) }));
}

/// The lines of the lists `shown` of `found`, in order: each list's head, then its files, as a
/// tree of folders when `tree` (less the `closed` ones' insides).
pub fn lines(found: &[ReviewSection], shown: &[usize], tree: bool, closed: &Closed) -> Vec<Line> {
    let mut lines = Vec::new();
    for &k in shown {
        let section = &found[k];
        lines.push(Line::Head(k));
        if section.files.is_empty() {
            lines.push(Line::Empty(k));
        } else if tree {
            walk(&mut lines, k, &grow(section), "", 0, closed);
        } else {
            lines.extend((0..section.files.len()).map(|at| Line::File { list: k, at, depth: None }));
        }
    }
    lines
}

/// A file row's id: its list and its place in it, apart for any length of list.
pub fn change_id(list: usize, at: usize) -> usize {
    (list << 32) | at
}

pub const HEAD: f32 = 36.0;
pub const EMPTY_BOX: f32 = 40.0;

/// How tall each line is, in the page's pixels.
pub fn height(line: &Line, tokens: &Tokens) -> f32 {
    match line {
        Line::Head(_) => HEAD,
        Line::Empty(_) => EMPTY_BOX + tokens.sp_2 + tokens.sp_6,
        Line::Folder { .. } | Line::File { depth: Some(_), .. } => tokens.row_h_sm,
        Line::File { depth: None, .. } => FILE_ROW,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_desktop_core::review::staging::SectionFile;

    fn section(side: &'static str, paths: &[&str]) -> ReviewSection {
        use argus_desktop_core::connection::page::ReviewFile;
        let file = |path: &str| {
            let (dir, name) = path.rsplit_once('/').map_or((String::new(), path), |(dir, name)| (format!("{dir}/"), name));
            SectionFile { file: ReviewFile { path: path.into(), dir, name: name.into(), added: 1, removed: 0, kind: "modified" }, partly: false, on: false }
        };
        ReviewSection { side, files: paths.iter().map(|p| file(p)).collect() }
    }

    fn folder(list: usize, label: &str, path: &str, depth: usize, files: usize) -> Line {
        Line::Folder { list, label: label.into(), path: path.into(), depth, open: true, files }
    }

    #[test]
    fn the_lines_are_each_lists_head_then_its_files_and_an_empty_staged_list_shows_its_box() {
        let found = vec![section("staged", &[]), section("unstaged", &["b/x.rs", "a.rs", "b/y.rs"]), section("untracked", &[])];
        let shown = shown(&found, false);
        assert_eq!(shown, [0, 1], "the staged list always, the others while they hold a file");
        let flat = lines(&found, &shown, false, &Closed::new());
        let file = |at, depth| Line::File { list: 1, at, depth };
        assert_eq!(flat, [Line::Head(0), Line::Empty(0), Line::Head(1), file(0, None), file(1, None), file(2, None)]);
        let by_folder = lines(&found, &shown, true, &Closed::new());
        assert_eq!(
            by_folder,
            [Line::Head(0), Line::Empty(0), Line::Head(1), folder(1, "b", "b/", 0, 2), file(0, Some(1)), file(2, Some(1)), file(1, Some(0))],
            "folders in name order over their files, then the files with no folder"
        );
    }

    /// `GitLists.dc.html`: `crates/argus-desktop-core/src` is one row, with `chrome` and
    /// `connection` nested under it, and a folder says how many files are under it.
    #[test]
    fn a_chain_of_single_folders_is_one_row_and_the_tree_nests_below_it() {
        let found = vec![section("unstaged", &["crates/core/src/chrome/rail.rs", "crates/core/src/chrome/add/tests.rs", "crates/core/src/connection/page.rs"])];
        let tree = lines(&found, &[0], true, &Closed::new());
        assert_eq!(
            tree,
            [
                Line::Head(0),
                folder(0, "crates/core/src", "crates/core/src/", 0, 3),
                folder(0, "chrome", "crates/core/src/chrome/", 1, 2),
                folder(0, "add", "crates/core/src/chrome/add/", 2, 1),
                Line::File { list: 0, at: 1, depth: Some(3) },
                Line::File { list: 0, at: 0, depth: Some(2) },
                folder(0, "connection", "crates/core/src/connection/", 1, 1),
                Line::File { list: 0, at: 2, depth: Some(2) },
            ]
        );
    }

    #[test]
    fn a_closed_folder_hides_its_insides_and_says_so() {
        let found = vec![section("unstaged", &["a/b.rs", "a/c/d.rs"])];
        let closed: Closed = [(0, "a/".to_owned())].into();
        let tree = lines(&found, &[0], true, &closed);
        assert_eq!(tree, [Line::Head(0), Line::Folder { list: 0, label: "a".into(), path: "a/".into(), depth: 0, open: false, files: 2 }]);
    }

    #[test]
    fn no_two_files_of_any_lists_share_a_row_id() {
        assert_ne!(change_id(0, 10_000), change_id(1, 0), "a list past ten thousand files");
        assert_ne!(change_id(1, 0), change_id(0, 1));
    }
}
