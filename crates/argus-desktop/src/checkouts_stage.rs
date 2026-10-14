//! What the Checkouts stage holds — whether it is open, whether branch rows
//! show, the filter, the selected row — and what its gestures ask of the
//! daemon. Like everything in the window it never predicts: a switched
//! branch or a pull shows once the tree says so.

use argus_protocol::column::{local_name, CheckoutRow};
use argus_protocol::{CheckoutId, DecisionBoard, PaneId, RepositoryInfo};

use crate::checkouts::{self, last_commit, View};
use crate::page::{self, Confirm};

#[derive(Default)]
pub struct CheckoutsStage {
    open: bool,
    branches_shown: bool,
    filter: String,
    /// The row the page chose, by key.
    selected: Option<String>,
    /// What the page was last sent, so an unchanged table is not sent again.
    sent: Option<page::Checkouts>,
    /// The question waiting on the person, and what a yes does.
    confirm: Option<(Confirm, Deleting)>,
}

/// What deleting the selected row comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deleting {
    /// `git branch -d`, or `-D` when `force`, run in the primary checkout.
    Branch { checkout: CheckoutId, branch: String, force: bool },
    /// The worktree removed: its panes, directory and branch.
    Checkout(CheckoutId),
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

    #[cfg(test)]
    pub fn selected_key(&self) -> Option<&str> {
        self.selected.as_deref()
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
                    None => Opened::Say(format!("nothing running in {}", c.name)),
                }
            }
            CheckoutRow::Branch(i) => switch(repo, repo.branches.get(i)?)?,
            CheckoutRow::Remote(i) => switch(repo, local_name(repo.remote_branches.get(i)?)?)?,
        })
    }

    /// `D`: a branch row is deleted at once, since git refuses one that
    /// holds commits nothing else has and that refusal is asked about next;
    /// a worktree is asked about first, because removing it ends its panes.
    /// The primary checkout, the main branch and a remote-only branch are
    /// refused: deleting the last would be a push, which nothing here does.
    pub fn delete(&mut self, repo: &RepositoryInfo) -> Result<Option<Deleting>, String> {
        let main = repo.default_branch.as_deref();
        match self.row(repo).ok_or("nothing is selected")? {
            CheckoutRow::Checkout(i) => {
                let c = repo.checkouts.get(i).ok_or("nothing is selected")?;
                if c.primary {
                    return Err(format!("{} is the primary checkout and stays", c.name));
                }
                let branch = c.git.as_ref().and_then(|g| g.branch.clone()).unwrap_or_else(|| c.name.clone());
                self.confirm = Some((Confirm::Remove { name: c.name.clone(), branch }, Deleting::Checkout(c.id)));
                Ok(None)
            }
            CheckoutRow::Branch(i) => {
                let branch = repo.branches.get(i).ok_or("nothing is selected")?;
                if Some(branch.as_str()) == main {
                    return Err(format!("{branch} is the main branch and stays"));
                }
                let checkout = primary(repo).ok_or("no primary checkout")?;
                Ok(Some(Deleting::Branch { checkout, branch: branch.clone(), force: false }))
            }
            CheckoutRow::Remote(i) => Err(format!(
                "{} is remote-only; nothing is pushed from here",
                repo.remote_branches.get(i).map_or("it", String::as_str)
            )),
        }
    }

    /// Git refused to delete `branch`: ask whether to force it, naming the
    /// commits a yes gives up.
    pub fn refused(&mut self, checkout: CheckoutId, branch: String, commits: &[argus_protocol::CommitInfo], now: u64) {
        let commits = commits.iter().map(|c| last_commit(c, now)).collect();
        self.confirm = Some((
            Confirm::Force { branch: branch.clone(), commits },
            Deleting::Branch { checkout, branch, force: true },
        ));
    }

    /// The person answered the question: a yes is what it asked about.
    pub fn answer(&mut self, yes: bool) -> Option<Deleting> {
        let (_, deleting) = self.confirm.take()?;
        yes.then_some(deleting)
    }

    /// `N`: a branch row gets a worktree of its own, the remote's for a
    /// remote-only one; a checkout row branches `name` off its HEAD into a
    /// new worktree.
    pub fn new_worktree(&self, repo: &RepositoryInfo, name: Option<String>) -> Result<(CheckoutId, String), String> {
        let name = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
        match self.row(repo).ok_or("nothing is selected")? {
            CheckoutRow::Checkout(i) => {
                let c = repo.checkouts.get(i).ok_or("nothing is selected")?;
                Ok((c.id, name.ok_or("a new worktree needs a branch name")?))
            }
            CheckoutRow::Branch(i) => {
                let branch = repo.branches.get(i).ok_or("nothing is selected")?;
                Ok((primary(repo).ok_or("no primary checkout")?, branch.clone()))
            }
            CheckoutRow::Remote(i) => {
                let remote = repo.remote_branches.get(i).ok_or("nothing is selected")?;
                let local = local_name(remote).ok_or("no local name for that branch")?;
                Ok((primary(repo).ok_or("no primary checkout")?, local.to_owned()))
            }
        }
    }

    /// `git fetch --all --prune` and `git pull --ff-only` run in the selected
    /// checkout, or in the primary one for a branch row, which has none.
    pub fn where_git_runs(&self, repo: &RepositoryInfo) -> Option<CheckoutId> {
        self.checkout(repo).or_else(|| primary(repo))
    }

    /// The table, if it differs from what the page was last sent.
    pub fn take(&mut self, repo: Option<&RepositoryInfo>, board: Option<&DecisionBoard>, viewed: Option<&str>, now: u64) -> Option<page::Checkouts> {
        let table = self.table(repo, board, viewed, now);
        (self.sent.as_ref() != Some(&table)).then(|| {
            self.sent = Some(table.clone());
            table
        })
    }

    /// The table whatever was sent before: the page reloaded.
    pub fn everything(&mut self, repo: Option<&RepositoryInfo>, board: Option<&DecisionBoard>, viewed: Option<&str>, now: u64) -> page::Checkouts {
        let table = self.table(repo, board, viewed, now);
        self.sent = Some(table.clone());
        table
    }

    fn table(&self, repo: Option<&RepositoryInfo>, board: Option<&DecisionBoard>, viewed: Option<&str>, now: u64) -> page::Checkouts {
        let Some(repo) = repo else {
            return page::Checkouts {
                branches_shown: self.branches_shown,
                filter: self.filter.clone(),
                confirm: self.confirm.as_ref().map(|(question, _)| question.clone()),
                ..Default::default()
            };
        };
        let mut table = checkouts::build(repo, &View {
            branches_shown: self.branches_shown,
            filter: &self.filter,
            selected: self.selected.as_deref(),
            board,
            viewed,
            now,
        });
        table.confirm = self.confirm.as_ref().map(|(question, _)| question.clone());
        table
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
    fn a_branch_is_deleted_at_once_and_its_refusal_is_asked_about() {
        let mut stage = CheckoutsStage::default();
        stage.toggle_branches();
        stage.select("bold".into());
        let first = stage.delete(&repo()).unwrap();
        assert_eq!(first, Some(Deleting::Branch { checkout: CheckoutId(100), branch: "old".into(), force: false }));

        stage.refused(CheckoutId(100), "old".into(), &[], 0);
        assert_eq!(
            stage.take(Some(&repo()), None, None, 0).unwrap().confirm,
            Some(Confirm::Force { branch: "old".into(), commits: Vec::new() })
        );
        assert_eq!(stage.answer(true), Some(Deleting::Branch { checkout: CheckoutId(100), branch: "old".into(), force: true }));
        assert_eq!(stage.answer(true), None, "a question is answered once");
    }

    #[test]
    fn a_worktree_is_asked_about_before_it_goes_and_the_primary_never_goes() {
        let mut stage = CheckoutsStage::default();
        stage.select("c101".into());
        assert_eq!(stage.delete(&repo()), Ok(None));
        assert_eq!(stage.answer(false), None, "a no does nothing");

        stage.select("c100".into());
        assert!(stage.delete(&repo()).is_err());
    }

    #[test]
    fn the_main_and_a_remote_branch_are_not_deleted_here() {
        let mut stage = CheckoutsStage::default();
        stage.toggle_branches();
        stage.select("rorigin/transcripts".into());
        assert!(stage.delete(&repo()).is_err());
        let mut main_only = repo();
        main_only.branches = vec!["main".into()];
        main_only.checkouts.retain(|c| !c.primary || c.name != "main");
        main_only.checkouts[0].primary = true;
        main_only.checkouts[0].git.as_mut().unwrap().branch = Some("desktop".into());
        stage.select("bmain".into());
        assert!(stage.delete(&main_only).unwrap_err().contains("main branch"));
    }

    #[test]
    fn a_new_worktree_takes_a_branch_rows_name_or_asks_a_checkout_for_one() {
        let mut stage = CheckoutsStage::default();
        stage.toggle_branches();
        stage.select("rorigin/transcripts".into());
        assert_eq!(stage.new_worktree(&repo(), None), Ok((CheckoutId(100), "transcripts".into())));
        stage.select("c101".into());
        assert!(stage.new_worktree(&repo(), Some("  ".into())).is_err());
        assert_eq!(stage.new_worktree(&repo(), Some("spike".into())), Ok((CheckoutId(101), "spike".into())));
    }

    #[test]
    fn an_unchanged_table_is_not_sent_again() {
        let mut stage = CheckoutsStage::default();
        assert!(stage.take(Some(&repo()), None, None, 0).is_some());
        assert!(stage.take(Some(&repo()), None, None, 0).is_none());
        stage.toggle_branches();
        assert!(stage.take(Some(&repo()), None, None, 0).is_some());
    }
}
