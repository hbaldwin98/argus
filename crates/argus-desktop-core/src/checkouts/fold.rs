//! Which rows of a file tree fold into the one "N ignored" row at its foot, which the Files panel
//! and the Code stage share. The page holds the same rule in `kit/ignored_fold.ts`; the window
//! asks it here, so a rule that a person reads as one is written once per client, not once per
//! surface.

use super::files_page::{FileMark, FileRow};
use crate::code::stage::CodeTreeRow;

/// A row of a tree, as far as the fold is concerned.
pub trait Foldable {
    fn depth(&self) -> usize;
    fn ignored(&self) -> bool;
    fn name(&self) -> &str;
}

impl Foldable for FileRow {
    fn depth(&self) -> usize {
        self.depth
    }

    fn ignored(&self) -> bool {
        self.mark == Some(FileMark::Ignored)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl Foldable for CodeTreeRow {
    fn depth(&self) -> usize {
        self.depth
    }

    fn ignored(&self) -> bool {
        self.ignored
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// The rows, by index, split into what stays in the tree and what the fold holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folded {
    /// The rows that stay in the tree, in their order.
    pub shown: Vec<usize>,
    /// The rows the fold holds, in their order: an ignored top-level entry and whatever was
    /// opened under it.
    pub held: Vec<usize>,
    /// The ignored top-level entries, which the fold's row names.
    pub names: Vec<String>,
}

impl Folded {
    /// What the fold's row says: `4 ignored · .argus, .opencode, target`.
    pub fn label(&self) -> String {
        format!("{} ignored \u{b7} {}", self.names.len(), self.names.join(", "))
    }
}

/// Ignored entries at the top of the tree are build output, caches and the agents' own folders:
/// seven of them between a person and the sources they came for. Deeper ones stay where they
/// are, since folding one out of the middle of its folder would lose where it was.
pub fn fold_ignored<T: Foldable>(rows: &[T]) -> Folded {
    let mut folded = Folded::default();
    let mut inside = false;
    for (at, row) in rows.iter().enumerate() {
        if row.depth() == 0 {
            inside = row.ignored();
            if inside {
                folded.names.push(row.name().to_owned());
            }
        }
        if inside { folded.held.push(at) } else { folded.shown.push(at) }
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, depth: usize, ignored: bool) -> FileRow {
        FileRow {
            path: name.into(),
            name: name.into(),
            depth,
            dir: false,
            open: false,
            mark: ignored.then_some(FileMark::Ignored),
        }
    }

    /// `page/test/kit/ignored_fold.test.mjs` holds the same rule for the page.
    #[test]
    fn a_top_level_ignored_entry_and_what_is_opened_under_it_are_held() {
        let rows = [row("src", 0, false), row("a.rs", 1, false), row("target", 0, true), row("debug", 1, false), row("README", 0, false)];
        let folded = fold_ignored(&rows);
        assert_eq!(folded.shown, [0, 1, 4]);
        assert_eq!(folded.held, [2, 3]);
        assert_eq!(folded.names, ["target"]);
    }

    #[test]
    fn a_deeper_ignored_entry_stays_where_it_is() {
        let rows = [row("src", 0, false), row("gen", 1, true), row("b.rs", 1, false)];
        let folded = fold_ignored(&rows);
        assert_eq!(folded.shown, [0, 1, 2]);
        assert!(folded.held.is_empty() && folded.names.is_empty());
    }

    #[test]
    fn the_label_counts_and_names_the_entries() {
        let rows = [row(".argus", 0, true), row("src", 0, false), row("target", 0, true)];
        assert_eq!(fold_ignored(&rows).label(), "2 ignored \u{b7} .argus, target");
    }
}
