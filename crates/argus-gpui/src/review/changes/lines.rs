//! The lines the Changes pane's lists are drawn as, one row of the long list each: which lists
//! show, each list's head, its files flat or under their folders, how tall each line is, and the id
//! that keeps a file's row apart from every other list's.

use argus_desktop_core::review::staging::ReviewSection;

use crate::review::changes::flat::FILE_ROW;
use crate::review::changes::side::folder;
use crate::theme::Tokens;

/// One line of the lists as drawn: a list's head, the box an empty list shows, a folder's name over
/// its files, or a file, by its list and its place in it (`folders` is whether the row says its
/// folder, which a folder's line says for it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Head(usize),
    Empty(usize),
    Folder(usize, String),
    File { list: usize, at: usize, folders: bool },
}

impl Line {
    /// The list the line is in, as its index among the page's.
    pub fn list(&self) -> usize {
        match self {
            Line::Head(k) | Line::Empty(k) | Line::Folder(k, _) => *k,
            Line::File { list, .. } => *list,
        }
    }
}

/// The lists shown: staged always (a file is dropped on it), the others while they hold a file,
/// and while finding only those holding a match.
pub fn shown(found: &[ReviewSection], finding: bool) -> Vec<usize> {
    found.iter().enumerate().filter(|(_, s)| (s.side == "staged" && !finding) || !s.files.is_empty()).map(|(k, _)| k).collect()
}

/// The lines of the lists `shown` of `found`, in order: each list's head, then its files, by
/// folder under each folder's name when `tree`.
pub fn lines(found: &[ReviewSection], shown: &[usize], tree: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    for &k in shown {
        let section = &found[k];
        lines.push(Line::Head(k));
        if section.files.is_empty() {
            lines.push(Line::Empty(k));
        } else if tree {
            let mut folders: Vec<(&str, Vec<usize>)> = Vec::new();
            for (i, file) in section.files.iter().enumerate() {
                let dir = folder(&file.file.dir);
                match folders.iter_mut().find(|(d, _)| *d == dir) {
                    Some((_, files)) => files.push(i),
                    None => folders.push((dir, vec![i])),
                }
            }
            folders.sort_by(|a, b| a.0.cmp(b.0));
            for (dir, files) in folders {
                lines.push(Line::Folder(k, dir.to_owned()));
                lines.extend(files.into_iter().map(|at| Line::File { list: k, at, folders: false }));
            }
        } else {
            lines.extend((0..section.files.len()).map(|at| Line::File { list: k, at, folders: true }));
        }
    }
    lines
}

/// A file row's id: its list and its place in it, apart for any length of list.
pub fn change_id(list: usize, at: usize) -> usize {
    (list << 32) | at
}

pub const HEAD: f32 = 30.0;
pub const EMPTY_BOX: f32 = 40.0;

/// How tall each line is, in the page's pixels.
pub fn height(line: &Line, tokens: &Tokens) -> f32 {
    match line {
        Line::Head(_) => HEAD,
        Line::Empty(_) => EMPTY_BOX + tokens.sp_2 + tokens.sp_6,
        Line::Folder(..) => tokens.sp_24,
        Line::File { .. } => FILE_ROW,
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

    #[test]
    fn the_lines_are_each_lists_head_then_its_files_and_an_empty_staged_list_shows_its_box() {
        let found = vec![section("staged", &[]), section("unstaged", &["b/x.rs", "a.rs", "b/y.rs"]), section("untracked", &[])];
        let shown = shown(&found, false);
        assert_eq!(shown, [0, 1], "the staged list always, the others while they hold a file");
        let flat = lines(&found, &shown, false);
        let file = |at, folders| Line::File { list: 1, at, folders };
        assert_eq!(flat, [Line::Head(0), Line::Empty(0), Line::Head(1), file(0, true), file(1, true), file(2, true)]);
        let by_folder = lines(&found, &shown, true);
        assert_eq!(
            by_folder,
            [Line::Head(0), Line::Empty(0), Line::Head(1), Line::Folder(1, "b".into()), file(0, false), file(2, false), Line::Folder(1, "repository root".into()), file(1, false)],
            "folders in name order, each over its files"
        );
    }

    #[test]
    fn no_two_files_of_any_lists_share_a_row_id() {
        assert_ne!(change_id(0, 10_000), change_id(1, 0), "a list past ten thousand files");
        assert_ne!(change_id(1, 0), change_id(0, 1));
    }
}
