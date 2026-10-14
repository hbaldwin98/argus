//! What the Checkouts stage holds — whether it is open, whether branch rows
//! show, the filter, the selected row — and what its gestures ask of the
//! daemon. Like everything in the window it never predicts: a switched
//! branch or a pull shows once the tree says so.

use argus_protocol::column::{local_name, CheckoutRow};
use argus_protocol::{CheckoutId, DecisionBoard, PaneId, RepositoryInfo};

use crate::checkouts::{self, View};
use crate::page;

#[derive(Default)]
pub struct CheckoutsStage {
    open: bool,
    branches_shown: bool,
    filter: String,
    /// The row the page chose, by key.
    selected: Option<String>,
    /// What the page was last sent, so an unchanged table is not sent again.
    sent: Option<page::Checkouts>,
}

/// What Enter on a row comes to.
#[derive(Debug)]
pub enum Opened {
    /// The checkout's first pane, to show in the workspace.
    Pane(PaneId),
    /// The primary checkout, to be switched to the row's branch.
    Switch { checkout: CheckoutId, branch: String },
    /// Nothing to open, and why.
    Say(String),
}

impl CheckoutsStage {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn toggle_branches(&mut self) {
        self.branches_shown = !self.branches_shown;
    }

    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }

    pub fn select(&mut self, key: String) {
        self.selected = Some(key);
    }

    /// Moves the selection `by` rows, held at either end.
    pub fn step(&mut self, repo: &RepositoryInfo, by: isize) {
        let rows = checkouts::rows(repo, self.branches_shown, &self.filter);
        let Some(current) = self.row(repo) else {
            return;
        };
        let at = rows.iter().position(|row| *row == current).unwrap_or(0);
        let next = at.saturating_add_signed(by).min(rows.len().saturating_sub(1));
        self.selected = rows.get(next).and_then(|row| checkouts::key(repo, *row));
    }

    /// The selected row as the table lists it now.
    pub fn row(&self, repo: &RepositoryInfo) -> Option<CheckoutRow> {
        let rows = checkouts::rows(repo, self.branches_shown, &self.filter);
        checkouts::selected(repo, &rows, self.selected.as_deref())
    }

    /// The selected row's checkout, when it has one.
    pub fn checkout(&self, repo: &RepositoryInfo) -> Option<CheckoutId> {
        match self.row(repo)? {
            CheckoutRow::Checkout(i) => repo.checkouts.get(i).map(|c| c.id),
            _ => None,
        }
    }

    /// Enter: a checkout opens on its first pane in the workspace; a branch
    /// row switches the primary checkout to it.
    pub fn open_row(&self, repo: &RepositoryInfo) -> Option<Opened> {
        Some(match self.row(repo)? {
            CheckoutRow::Checkout(i) => {
                let c = repo.checkouts.get(i)?;
                match c.listed_panes().next() {
                    Some(pane) => Opened::Pane(pane.id),
                    None => Opened::Say(format!("nothing runs in {}: its + starts a shell or an agent", c.name)),
                }
            }
            CheckoutRow::Branch(i) => switch(repo, repo.branches.get(i)?)?,
            CheckoutRow::Remote(i) => switch(repo, local_name(repo.remote_branches.get(i)?)?)?,
        })
    }

    /// `git fetch --all --prune` and `git pull --ff-only` run in the selected
    /// checkout, or in the primary one for a branch row, which has none.
    pub fn where_git_runs(&self, repo: &RepositoryInfo) -> Option<CheckoutId> {
        self.checkout(repo).or_else(|| primary(repo))
    }

    /// The table, if it differs from what the page was last sent.
    pub fn take(&mut self, repo: Option<&RepositoryInfo>, board: Option<&DecisionBoard>, viewed: Option<&str>) -> Option<page::Checkouts> {
        let table = self.table(repo, board, viewed);
        (self.sent.as_ref() != Some(&table)).then(|| {
            self.sent = Some(table.clone());
            table
        })
    }

    /// The table whatever was sent before: the page reloaded.
    pub fn everything(&mut self, repo: Option<&RepositoryInfo>, board: Option<&DecisionBoard>, viewed: Option<&str>) -> page::Checkouts {
        let table = self.table(repo, board, viewed);
        self.sent = Some(table.clone());
        table
    }

    fn table(&self, repo: Option<&RepositoryInfo>, board: Option<&DecisionBoard>, viewed: Option<&str>) -> page::Checkouts {
        let Some(repo) = repo else {
            return page::Checkouts { branches_shown: self.branches_shown, filter: self.filter.clone(), ..Default::default() };
        };
        checkouts::build(repo, &View {
            branches_shown: self.branches_shown,
            filter: &self.filter,
            selected: self.selected.as_deref(),
            board,
            viewed,
        })
    }
}

fn primary(repo: &RepositoryInfo) -> Option<CheckoutId> {
    repo.checkouts.iter().find(|c| c.primary).map(|c| c.id)
}

fn switch(repo: &RepositoryInfo, branch: &str) -> Option<Opened> {
    Some(Opened::Switch { checkout: primary(repo)?, branch: branch.to_owned() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane, repository};
    use argus_protocol::{PaneKind, PaneStatus};

    fn repo() -> RepositoryInfo {
        let mut repo = repository(10, "argus", None, vec![
            checkout(100, "main", true, Vec::new()),
            checkout(101, "desktop", false, vec![pane(7, PaneKind::Agent, "claude", PaneStatus::Working)]),
        ]);
        repo.default_branch = Some("main".into());
        repo.branches = vec!["old".into()];
        repo.remote_branches = vec!["origin/transcripts".into()];
        repo
    }

    #[test]
    fn stepping_moves_the_selection_and_holds_at_the_ends() {
        let mut stage = CheckoutsStage::default();
        stage.step(&repo(), -1);
        assert_eq!(stage.checkout(&repo()), Some(CheckoutId(100)));
        stage.step(&repo(), 5);
        assert_eq!(stage.checkout(&repo()), Some(CheckoutId(101)), "branches are not shown");
    }

    #[test]
    fn enter_opens_a_checkouts_pane_or_says_there_is_none() {
        let mut stage = CheckoutsStage::default();
        stage.select("c101".into());
        assert!(matches!(stage.open_row(&repo()), Some(Opened::Pane(PaneId(7)))));
        stage.select("c100".into());
        assert!(matches!(stage.open_row(&repo()), Some(Opened::Say(_))));
    }

    #[test]
    fn enter_on_a_branch_switches_the_primary_checkout_to_its_local_name() {
        let mut stage = CheckoutsStage::default();
        stage.toggle_branches();
        stage.select("rorigin/transcripts".into());
        assert!(matches!(
            stage.open_row(&repo()),
            Some(Opened::Switch { checkout: CheckoutId(100), branch }) if branch == "transcripts"
        ));
        assert_eq!(stage.where_git_runs(&repo()), Some(CheckoutId(100)), "a branch has no directory of its own");
    }

    #[test]
    fn an_unchanged_table_is_not_sent_again() {
        let mut stage = CheckoutsStage::default();
        assert!(stage.take(Some(&repo()), None, None).is_some());
        assert!(stage.take(Some(&repo()), None, None).is_none());
        stage.toggle_branches();
        assert!(stage.take(Some(&repo()), None, None).is_some());
    }
}
