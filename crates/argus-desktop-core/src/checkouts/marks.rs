//! The mark a file tree's row carries at its right, and says on hover, in the Files panel and the
//! Code stage: `M` for a changed file, `?` for an untracked one, and on a Code folder the count of
//! changed files under it. The page holds these in `markOf` (`ck/files.ts`) and `entryOf`
//! (`code/stage.ts`).

use std::collections::HashMap;

use super::files_page::{FileMark, FileRow};
use crate::code::stage::CodeTreeRow;

/// Which colour the mark is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Changed,
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    /// `M`, `?`, or a count on a folder.
    pub text: String,
    pub tone: Tone,
    /// What hovering it says, and what a reader that cannot see it hears.
    pub title: String,
}

/// A Files row's mark: git's word, and nothing for an ignored entry, which is drawn faint instead.
pub fn of_file(row: &FileRow) -> Option<Mark> {
    match row.mark? {
        FileMark::Changed => Some(Mark { text: "M".into(), tone: Tone::Changed, title: "Changed".into() }),
        FileMark::Untracked => Some(Mark { text: "?".into(), tone: Tone::Untracked, title: "Untracked".into() }),
        FileMark::Ignored => None,
    }
}

/// How many changed files are under each folder, for the count a folder carries.
pub fn changed_under<'a>(changed: impl IntoIterator<Item = &'a str>) -> HashMap<&'a str, usize> {
    let mut under = HashMap::new();
    for path in changed {
        for folder in crate::code::tree::folders_of(path) {
            *under.entry(folder).or_insert(0) += 1;
        }
    }
    under
}

/// A Code row's mark: a folder with a change under it says how many files, a changed file `M`
/// with how much.
pub fn of_code(row: &CodeTreeRow, under: &HashMap<&str, usize>) -> Option<Mark> {
    if !row.changed {
        return None;
    }
    if row.folder {
        let inside = under.get(row.path.as_str()).copied().unwrap_or(0);
        let files = if inside == 1 { "file" } else { "files" };
        return Some(Mark { text: inside.to_string(), tone: Tone::Changed, title: format!("{inside} changed {files} under it") });
    }
    Some(Mark { text: "M".into(), tone: Tone::Changed, title: format!("Changed: +{} \u{2212}{}", row.added, row.removed) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::stage::CodeChangedFile;

    fn file(path: &str) -> CodeChangedFile {
        CodeChangedFile { path: path.into(), name: path.rsplit('/').next().unwrap().into(), dir: String::new(), added: 1, removed: 0 }
    }

    fn row(path: &str, folder: bool, changed: bool) -> CodeTreeRow {
        CodeTreeRow { path: path.into(), name: path.into(), depth: 0, folder, open: false, changed, added: 3, removed: 1, on: false, ignored: false }
    }

    #[test]
    fn a_files_row_is_marked_as_git_says_and_an_ignored_one_is_not() {
        let mut row = FileRow { path: "a".into(), name: "a".into(), depth: 0, dir: false, open: false, mark: Some(FileMark::Changed) };
        assert_eq!(of_file(&row).map(|m| (m.text, m.tone)), Some(("M".into(), Tone::Changed)));
        row.mark = Some(FileMark::Untracked);
        assert_eq!(of_file(&row).map(|m| (m.text, m.title)), Some(("?".into(), "Untracked".into())));
        row.mark = Some(FileMark::Ignored);
        assert_eq!(of_file(&row), None);
        row.mark = None;
        assert_eq!(of_file(&row), None);
    }

    #[test]
    fn a_folder_counts_the_changed_files_under_it_at_every_depth() {
        let changed = [file("crates/a/x.rs"), file("crates/a/y.rs"), file("crates/b.rs"), file("top.md")];
        let under = changed_under(changed.iter().map(|f| f.path.as_str()));
        assert_eq!(under.get("crates"), Some(&3));
        assert_eq!(under.get("crates/a"), Some(&2));
        assert_eq!(under.get("top.md"), None);
    }

    #[test]
    fn a_code_folder_says_how_many_files_and_a_file_how_much() {
        let changed = [file("crates/a.rs"), file("crates/b.rs"), file("docs/c.md")];
        let under = changed_under(changed.iter().map(|f| f.path.as_str()));
        let many = of_code(&row("crates", true, true), &under).unwrap();
        assert_eq!((many.text.as_str(), many.title.as_str()), ("2", "2 changed files under it"));
        assert_eq!(of_code(&row("docs", true, true), &under).unwrap().title, "1 changed file under it");
        assert_eq!(of_code(&row("crates/a.rs", false, true), &under).unwrap().title, "Changed: +3 \u{2212}1");
        assert_eq!(of_code(&row("clean.rs", false, false), &under), None);
    }
}
