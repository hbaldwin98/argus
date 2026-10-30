//! Which rows of the files tree are drawn as one: a chain of open folders that each hold nothing but
//! the next folder (`argus-gpui/src/chrome`) is one row, as the board has it, and the rows under it
//! come back out by the levels that were folded.
//!
//! Core has ordered the rows and opened the folders; this only reads their depths. The output stays
//! flat, one line per drawn row, so the list can scroll to a row by its place among them.

use argus_desktop_core::code::stage::CodeTreeRow;

/// One drawn row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The rows it stands for, outermost first: one for most, several for a chain of folders.
    pub chain: Vec<usize>,
    /// How many levels in it is drawn, once the folded levels above it are taken out.
    pub depth: usize,
}

impl Line {
    /// The row a press and the marks belong to: the innermost folder of a chain.
    pub fn at(&self) -> usize {
        *self.chain.last().expect("a line stands for a row")
    }
}

/// The rows named by `shown` (an increasing run of indices into `rows`), chained.
pub fn lines(rows: &[CodeTreeRow], shown: &[usize]) -> Vec<Line> {
    let mut out = Vec::with_capacity(shown.len());
    // The chains still above the row being read: where each ends and how many levels it took out.
    let mut above: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < shown.len() {
        let first = &rows[shown[i]];
        while above.last().is_some_and(|&(last, _)| last >= first.depth) {
            above.pop();
        }
        let depth = first.depth - above.iter().map(|&(_, removed)| removed).sum::<usize>();
        let mut chain = vec![shown[i]];
        // Follow it in while its one child is a folder.
        while let Some(next) = only_folder(rows, shown, i) {
            chain.push(shown[next]);
            i = next;
        }
        let last = rows[*chain.last().expect("a chain has its first row")].depth;
        if chain.len() > 1 {
            above.push((last, chain.len() - 1));
        }
        out.push(Line { chain, depth });
        i += 1;
    }
    out
}

/// Where in `shown` the only child of the open folder at `i` is, when the folder holds that child
/// and it is a folder.
fn only_folder(rows: &[CodeTreeRow], shown: &[usize], i: usize) -> Option<usize> {
    let parent = &rows[shown[i]];
    if !parent.folder || !parent.open || parent.ignored {
        return None;
    }
    let child = shown.get(i + 1).map(|&at| &rows[at]).filter(|child| child.depth == parent.depth + 1 && child.folder)?;
    // Another row at the child's level before the parent's level comes back is a second child.
    let second = shown[i + 2..].iter().map(|&at| rows[at].depth).take_while(|&depth| depth > parent.depth).any(|depth| depth == child.depth);
    (!second).then_some(i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, depth: usize, folder: bool, open: bool) -> CodeTreeRow {
        CodeTreeRow { path: name.into(), name: name.into(), depth, folder, open, changed: false, added: 0, removed: 0, on: false, ignored: false }
    }

    fn all(rows: &[CodeTreeRow]) -> Vec<usize> {
        (0..rows.len()).collect()
    }

    /// Code.dc.html: `argus-gpui/src/chrome` is one row, and what is under it is as deep as it
    /// would be under a single folder.
    #[test]
    fn a_folder_holding_only_a_folder_is_one_row_with_it() {
        let rows = [row("argus-gpui", 1, true, true), row("src", 2, true, true), row("chrome", 3, true, true), row("mod.rs", 4, false, false), row("docs", 1, true, false)];
        let lines = lines(&rows, &all(&rows));
        assert_eq!(lines, [Line { chain: vec![0, 1, 2], depth: 1 }, Line { chain: vec![3], depth: 2 }, Line { chain: vec![4], depth: 1 }]);
        assert_eq!(lines[0].at(), 2);
    }

    #[test]
    fn a_folder_with_two_children_or_a_file_is_not_chained() {
        let rows = [row("a", 0, true, true), row("b", 1, true, false), row("c", 1, true, false), row("d", 0, true, true), row("e", 1, true, true), row("f.rs", 1, false, false)];
        let lines = lines(&rows, &all(&rows));
        assert_eq!(lines.iter().map(|l| l.chain.len()).collect::<Vec<_>>(), [1, 1, 1, 1, 1, 1]);
        assert_eq!(lines.iter().map(|l| l.depth).collect::<Vec<_>>(), [0, 1, 1, 0, 1, 1]);
    }

    /// A closed folder lists nothing under it, so its looking empty is not a reason to chain it.
    #[test]
    fn a_closed_folder_is_never_chained_into() {
        let rows = [row("a", 0, true, false), row("b", 1, true, true)];
        assert_eq!(lines(&rows, &all(&rows)).len(), 2);
    }

    /// Levels taken out above a row come back to the rows after the chain's own ones, and not to a
    /// sibling of the chain.
    #[test]
    fn the_levels_taken_out_do_not_reach_a_sibling_of_the_chain() {
        let rows = [row("a", 0, true, true), row("b", 1, true, true), row("c", 2, false, false), row("z", 0, true, false), row("y", 0, false, false)];
        let lines = lines(&rows, &all(&rows));
        assert_eq!(lines.iter().map(|l| l.depth).collect::<Vec<_>>(), [0, 1, 0, 0]);
    }
}
