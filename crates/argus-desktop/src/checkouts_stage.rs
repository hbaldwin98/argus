//! What the Checkouts stage holds — whether branch rows show, the filter,
//! the selected row — what its gestures ask of the daemon, and the table
//! the page last heard. Whether the stage is up is the session's. Like
//! everything in the window it never predicts: a switched branch or a pull
//! shows once the tree says so.

use argus_protocol::tree::column::CheckoutRow;
use argus_protocol::tree::column::verbs::{force_delete, Verb};
use argus_protocol::{CheckoutId, ClientMsg, DecisionBoard, PaneId, RepositoryInfo};
use serde::Serialize;

use crate::checkouts::{self, last_commit, Checkouts, LastCommit, View};
use crate::page::Event;
use crate::sent::Sent;

/// What the stage asks before doing something that cannot be taken back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Confirm {
    /// Removing a worktree: its panes end, its directory and its branch go.
    Remove { name: String, branch: String },
    /// Git refused `branch -d`: the branch holds commits nothing else has,
    /// which are what a yes gives up.
    Force { branch: String, commits: Vec<LastCommit> },
}

#[derive(Default)]
pub struct CheckoutsStage {
    branches_shown: bool,
    filter: String,
    /// The row the page chose, by key.
    selected: Option<String>,
    sent: Sent<Checkouts>,
    /// The question waiting on the person, and what a yes sends.
    confirm: Option<(Confirm, ClientMsg)>,
}

/// What Enter on a row comes to.
#[derive(Debug)]
pub enum Opened {
    /// The checkout's first pane, to show in the workspace.
    Pane(PaneId),
    /// The primary checkout switched to the row's branch.
    Send(Box<ClientMsg>),
    /// Nothing to open, and why.
    Say(String),
}

impl CheckoutsStage {
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
        let row = self.row(repo)?;
        Some(match row {
            CheckoutRow::Checkout(i) => {
                let c = repo.checkouts.get(i)?;
                match c.listed_panes().next() {
                    Some(pane) => Opened::Pane(pane.id),
                    None => Opened::Say(format!("nothing running in {}", c.name)),
                }
            }
            _ => match repo.act(row, Verb::Switch) {
                Ok(msg) => Opened::Send(Box::new(msg)),
                Err(text) => Opened::Say(text),
            },
        })
    }

    /// `verb` on the selected row, as `tree::column::verbs` decides it.
    pub fn act(&self, repo: &RepositoryInfo, verb: Verb) -> Result<ClientMsg, String> {
        repo.act(self.row(repo).ok_or("nothing is selected")?, verb)
    }

    /// `D`: a branch row is deleted at once, since git refuses one that
    /// holds commits nothing else has and that refusal is asked about next;
    /// a worktree is asked about first, because removing it ends its panes.
    pub fn delete(&mut self, repo: &RepositoryInfo) -> Result<Option<ClientMsg>, String> {
        match self.act(repo, Verb::Remove)? {
            ClientMsg::RemoveCheckout { checkout } => {
                self.ask_remove(repo, checkout)?;
                Ok(None)
            }
            msg => Ok(Some(msg)),
        }
    }

    /// Asks whether to remove the worktree `checkout`, whichever row is
    /// selected: `D` on its row, or `x` on it in the rail.
    pub fn ask_remove(&mut self, repo: &RepositoryInfo, checkout: CheckoutId) -> Result<(), String> {
        let i = repo.checkouts.iter().position(|c| c.id == checkout).ok_or("that checkout is gone")?;
        let msg = repo.act(CheckoutRow::Checkout(i), Verb::Remove)?;
        let c = &repo.checkouts[i];
        let branch = c.git.as_ref().and_then(|g| g.branch.clone()).unwrap_or_else(|| c.name.clone());
        self.confirm = Some((Confirm::Remove { name: c.name.clone(), branch }, msg));
        Ok(())
    }

    /// Git refused to delete `branch`: ask whether to force it, naming the
    /// commits a yes gives up.
    pub fn refused(&mut self, checkout: CheckoutId, branch: String, commits: &[argus_protocol::CommitInfo], now: u64) {
        let commits = commits.iter().map(|c| last_commit(c, now)).collect();
        self.confirm = Some((Confirm::Force { branch: branch.clone(), commits }, force_delete(checkout, branch)));
    }

    /// The person answered the question: a yes is what it asked about.
    pub fn answer(&mut self, yes: bool) -> Option<ClientMsg> {
        let (_, msg) = self.confirm.take()?;
        yes.then_some(msg)
    }

    /// The table, if it differs from what the page was last sent.
    pub fn take(&mut self, repo: Option<&RepositoryInfo>, board: Option<&DecisionBoard>, viewed: Option<&str>, now: u64) -> Option<Event> {
        let table = self.table(repo, board, viewed, now);
        self.sent.take(Some(table)).map(|table| Event::Checkouts(Box::new(table)))
    }

    /// The page reloaded and holds nothing: the table is sent again.
    pub fn reload(&mut self) {
        self.sent.forget();
    }

    fn table(&self, repo: Option<&RepositoryInfo>, board: Option<&DecisionBoard>, viewed: Option<&str>, now: u64) -> Checkouts {
        let Some(repo) = repo else {
            return Checkouts {
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

    fn sent(msg: Option<ClientMsg>) -> String {
        format!("{msg:?}")
    }

    fn table(stage: &mut CheckoutsStage) -> Option<Checkouts> {
        match stage.take(Some(&repo()), None, None, 0)? {
            Event::Checkouts(table) => Some(*table),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn enter_on_a_branch_sends_what_the_column_says_a_switch_is() {
        let mut stage = CheckoutsStage::default();
        stage.toggle_branches();
        stage.select("rorigin/transcripts".into());
        let Some(Opened::Send(msg)) = stage.open_row(&repo()) else {
            panic!("a branch row switches");
        };
        assert_eq!(sent(Some(*msg)), r#"Some(SwitchBranch { checkout: CheckoutId(100), branch: "transcripts" })"#);
    }

    #[test]
    fn a_branch_is_deleted_at_once_and_its_refusal_is_asked_about() {
        let mut stage = CheckoutsStage::default();
        stage.toggle_branches();
        stage.select("bold".into());
        let first = stage.delete(&repo()).unwrap();
        assert_eq!(sent(first), r#"Some(DeleteBranch { checkout: CheckoutId(100), branch: "old", force: false })"#);

        stage.refused(CheckoutId(100), "old".into(), &[], 0);
        assert_eq!(
            table(&mut stage).unwrap().confirm,
            Some(Confirm::Force { branch: "old".into(), commits: Vec::new() })
        );
        assert_eq!(sent(stage.answer(true)), r#"Some(DeleteBranch { checkout: CheckoutId(100), branch: "old", force: true })"#);
        assert!(stage.answer(true).is_none(), "a question is answered once");
    }

    #[test]
    fn a_worktree_is_asked_about_before_it_goes() {
        let mut stage = CheckoutsStage::default();
        stage.select("c101".into());
        assert!(matches!(stage.delete(&repo()), Ok(None)));
        assert_eq!(
            table(&mut stage).unwrap().confirm,
            Some(Confirm::Remove { name: "desktop".into(), branch: "desktop".into() })
        );
        assert!(stage.answer(false).is_none(), "a no does nothing");

        stage.select("c100".into());
        assert_eq!(stage.delete(&repo()).unwrap_err(), "main is the primary checkout and stays");
    }

    #[test]
    fn an_unchanged_table_is_not_sent_again() {
        let mut stage = CheckoutsStage::default();
        assert!(table(&mut stage).is_some());
        assert!(table(&mut stage).is_none());
        stage.toggle_branches();
        assert!(table(&mut stage).is_some());
    }
}
