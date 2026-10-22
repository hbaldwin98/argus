//! How the table's rows fall into its blocks: the checkouts first, then, only when there are
//! branches no checkout is on, the fold row that stands for them, then the branch rows while they
//! are shown. Rust already gives the rows in the protocol's column order, so this only splits them.

use crate::checkouts::table::{Checkouts, CheckoutsRow};

/// The fold row: the branches without a checkout, a count until it is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fold {
    /// `9 branches without a checkout`.
    pub label: String,
    /// The branch rows below it are listed.
    pub shown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table<'a> {
    pub checkouts: Vec<&'a CheckoutsRow>,
    pub fold: Option<Fold>,
    pub branches: Vec<&'a CheckoutsRow>,
}

/// The fold shows whenever a branch lacks a checkout, even when the filter hides every branch row:
/// its count is of the repository's, not of what is listed.
pub fn partition(table: &Checkouts) -> Table<'_> {
    let (checkouts, branches) = table.rows.iter().partition(|row| row.checkout.is_some());
    let fold = (table.branches > 0).then(|| Fold {
        label: format!("{} branch{} without a checkout", table.branches, if table.branches == 1 { "" } else { "es" }),
        shown: table.branches_shown,
    });
    Table { checkouts, fold, branches }
}

impl Checkouts {
    /// The row the table has selected, by its key.
    pub fn selected_row(&self) -> Option<&CheckoutsRow> {
        let key = self.selected.as_deref()?;
        self.rows.iter().find(|row| row.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkouts::says::tests::{branch_row, row};

    fn table(branches: usize, shown: bool) -> Checkouts {
        Checkouts {
            repository: Some("argus".into()),
            checkouts: 2,
            branches,
            branches_shown: shown,
            rows: vec![row(), CheckoutsRow { key: "c2".into(), checkout: Some(2), ..row() }, branch_row("old")],
            selected: Some("c2".into()),
            ..Checkouts::default()
        }
    }

    #[test]
    fn checkouts_come_first_then_the_fold_then_the_branches() {
        let table = table(9, true);
        let parts = partition(&table);
        assert_eq!(parts.checkouts.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["c1", "c2"]);
        assert_eq!(parts.fold, Some(Fold { label: "9 branches without a checkout".into(), shown: true }));
        assert_eq!(parts.branches.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["bold"]);
    }

    #[test]
    fn the_fold_is_singular_for_one_and_absent_for_none() {
        assert_eq!(partition(&table(1, false)).fold.map(|f| f.label), Some("1 branch without a checkout".into()));
        assert_eq!(partition(&table(0, false)).fold, None);
    }

    #[test]
    fn the_fold_stays_while_the_filter_hides_every_branch_row() {
        let mut table = table(3, false);
        table.rows.truncate(2);
        let parts = partition(&table);
        assert!(parts.fold.is_some() && parts.branches.is_empty());
    }

    #[test]
    fn the_selected_row_is_the_one_the_key_names() {
        assert_eq!(table(0, false).selected_row().map(|r| r.key.as_str()), Some("c2"));
        let mut gone = table(0, false);
        gone.selected = None;
        assert!(gone.selected_row().is_none());
    }
}
