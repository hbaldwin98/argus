//! What each verb on a row of a repository's checkouts column asks of the
//! daemon, or the words it is refused in, said once for every client: the
//! primary checkout and the main branch stay, a remote-only branch is never
//! deleted, and a row with no directory acts through the primary checkout.
//! `column` says which rows there are; this says what one of them allows.

use crate::tree::column::{local_name, CheckoutRow};
use crate::{CheckoutId, CheckoutInfo, ClientMsg, RepositoryInfo};

/// A gesture a client offers on a column row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb<'a> {
    /// A worktree removed, or a branch with no directory deleted. Removing
    /// a worktree ends its panes, which is why both clients ask first; a
    /// branch git refuses to drop comes back as [`force_delete`]'s question.
    Remove,
    /// A worktree for the row. A branch row brings its own branch, and a
    /// checkout row branches `name` off its HEAD, so only that one needs it.
    NewWorktree { name: Option<&'a str>, request_id: u64 },
    /// The primary checkout put on the row's branch, which is what a row
    /// with no directory offers instead of somewhere to be.
    Switch,
    Fetch,
    Pull,
}

impl RepositoryInfo {
    /// The checkout the repository was found as: never Argus's to remove,
    /// and where a branch with no directory of its own is acted on.
    pub fn primary(&self) -> Option<&CheckoutInfo> {
        self.checkouts.iter().find(|c| c.primary)
    }

    /// The checkout git runs in for `row`: its own, or the primary one for
    /// a branch, which has none.
    pub fn git_runs_in(&self, row: CheckoutRow) -> Option<CheckoutId> {
        match row {
            CheckoutRow::Checkout(i) => self.checkouts.get(i).map(|c| c.id),
            CheckoutRow::Branch(_) | CheckoutRow::Remote(_) => self.primary().map(|c| c.id),
        }
    }

    /// The branch a row with no directory stands for, as it would be
    /// locally: a remote-only row answers to the name git would make of it.
    pub fn row_branch(&self, row: CheckoutRow) -> Option<&str> {
        match row {
            CheckoutRow::Checkout(_) => None,
            CheckoutRow::Branch(i) => self.branches.get(i).map(String::as_str),
            CheckoutRow::Remote(i) => self.remote_branches.get(i).and_then(|b| local_name(b)),
        }
    }

    /// What `verb` on `row` sends, or why it sends nothing.
    pub fn act(&self, row: CheckoutRow, verb: Verb) -> Result<ClientMsg, String> {
        match verb {
            Verb::Remove => self.remove(row),
            Verb::NewWorktree { name, request_id } => {
                let (checkout, branch) = match row {
                    CheckoutRow::Checkout(i) => {
                        let c = self.checkouts.get(i).ok_or(GONE)?;
                        let name = name.map(str::trim).filter(|n| !n.is_empty());
                        (c.id, name.ok_or("a new worktree needs a branch name")?.to_owned())
                    }
                    _ => (self.through_primary()?, self.branch_of(row)?.to_owned()),
                };
                Ok(ClientMsg::CreateWorktree { checkout, branch, request_id })
            }
            Verb::Switch => match row {
                CheckoutRow::Checkout(i) => {
                    Err(format!("{} has a directory of its own", self.checkouts.get(i).ok_or(GONE)?.name))
                }
                _ => Ok(ClientMsg::SwitchBranch { checkout: self.through_primary()?, branch: self.branch_of(row)?.to_owned() }),
            },
            Verb::Fetch => Ok(ClientMsg::Fetch { checkout: self.git_runs_in(row).ok_or("no checkout to fetch in")? }),
            Verb::Pull => Ok(ClientMsg::Pull { checkout: self.git_runs_in(row).ok_or("no checkout to pull into")? }),
        }
    }

    /// Deleting the main branch would leave the column nothing to lead with
    /// and every count against it nothing to count from, and deleting a
    /// remote-only one would be a push, which nothing here does.
    fn remove(&self, row: CheckoutRow) -> Result<ClientMsg, String> {
        match row {
            CheckoutRow::Checkout(i) => {
                let c = self.checkouts.get(i).ok_or(GONE)?;
                if c.primary {
                    return Err(format!("{} is the primary checkout and stays", c.name));
                }
                Ok(ClientMsg::RemoveCheckout { checkout: c.id })
            }
            CheckoutRow::Branch(i) => {
                let branch = self.branches.get(i).ok_or(GONE)?;
                if self.default_branch.as_ref() == Some(branch) {
                    return Err(format!("{branch} is the main branch and stays"));
                }
                Ok(ClientMsg::DeleteBranch { checkout: self.through_primary()?, branch: branch.clone(), force: false })
            }
            CheckoutRow::Remote(i) => {
                Err(format!("{} is remote-only; nothing is pushed from here", self.remote_branches.get(i).ok_or(GONE)?))
            }
        }
    }

    fn through_primary(&self) -> Result<CheckoutId, String> {
        self.primary().map(|c| c.id).ok_or_else(|| "no primary checkout".to_owned())
    }

    fn branch_of(&self, row: CheckoutRow) -> Result<&str, String> {
        self.row_branch(row).ok_or_else(|| match row {
            CheckoutRow::Remote(i) => match self.remote_branches.get(i) {
                Some(remote) => format!("{remote} has no local name"),
                None => GONE.to_owned(),
            },
            _ => GONE.to_owned(),
        })
    }
}

/// A row index the repository no longer has: the tree moved under a
/// gesture already on its way.
const GONE: &str = "that row is gone";

/// What a yes sends once git has refused to delete `branch` as unmerged
/// ([`crate::ServerMsg::BranchNotMerged`]): the same deletion, forced, in
/// the checkout the refusal names.
pub fn force_delete(checkout: CheckoutId, branch: String) -> ClientMsg {
    ClientMsg::DeleteBranch { checkout, branch, force: true }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GitStatus, RepositoryId};

    fn checkout(id: u64, branch: &str, primary: bool) -> CheckoutInfo {
        CheckoutInfo {
            id: CheckoutId(id),
            name: branch.into(),
            path: format!("/src/{branch}"),
            panes: Vec::new(),
            git: Some(GitStatus {
                branch: Some(branch.into()),
                dirty: false,
                changed_files: 0,
                staged: 0,
                unstaged: 0,
                ahead: 0,
                behind: 0,
                upstream: None,
                last_commit: None,
            }),
            primary,
        }
    }

    /// The primary checkout sits on `feature`, so `main` is a branch row
    /// beside `old`, and `origin/transcripts` came with a fetch.
    fn repository() -> RepositoryInfo {
        RepositoryInfo {
            id: RepositoryId(1),
            name: "argus".into(),
            checkouts: vec![checkout(100, "feature", true), checkout(101, "desktop", false)],
            branches: vec!["main".into(), "old".into()],
            default_branch: Some("main".into()),
            remote_branches: vec!["origin/transcripts".into()],
            unmerged: Default::default(),
            last_used: None,
        }
    }

    const PRIMARY: CheckoutRow = CheckoutRow::Checkout(0);
    const WORKTREE: CheckoutRow = CheckoutRow::Checkout(1);
    const MAIN: CheckoutRow = CheckoutRow::Branch(0);
    const BRANCH: CheckoutRow = CheckoutRow::Branch(1);
    const REMOTE: CheckoutRow = CheckoutRow::Remote(0);

    fn said(answer: Result<ClientMsg, String>) -> String {
        match answer {
            Ok(msg) => format!("{msg:?}"),
            Err(refusal) => format!("refused: {refusal}"),
        }
    }

    #[test]
    fn each_verb_on_each_kind_of_row_sends_one_thing_or_says_why_not() {
        let named = Verb::NewWorktree { name: Some(" spike "), request_id: 7 };
        let unnamed = Verb::NewWorktree { name: None, request_id: 7 };
        let table: [(CheckoutRow, Verb, &str); 20] = [
            (PRIMARY, Verb::Remove, "refused: feature is the primary checkout and stays"),
            (WORKTREE, Verb::Remove, "RemoveCheckout { checkout: CheckoutId(101) }"),
            (MAIN, Verb::Remove, "refused: main is the main branch and stays"),
            (BRANCH, Verb::Remove, r#"DeleteBranch { checkout: CheckoutId(100), branch: "old", force: false }"#),
            (REMOTE, Verb::Remove, "refused: origin/transcripts is remote-only; nothing is pushed from here"),
            (WORKTREE, named, r#"CreateWorktree { checkout: CheckoutId(101), branch: "spike", request_id: 7 }"#),
            (WORKTREE, unnamed, "refused: a new worktree needs a branch name"),
            (BRANCH, unnamed, r#"CreateWorktree { checkout: CheckoutId(100), branch: "old", request_id: 7 }"#),
            (BRANCH, named, r#"CreateWorktree { checkout: CheckoutId(100), branch: "old", request_id: 7 }"#),
            (REMOTE, unnamed, r#"CreateWorktree { checkout: CheckoutId(100), branch: "transcripts", request_id: 7 }"#),
            (PRIMARY, Verb::Switch, "refused: feature has a directory of its own"),
            (WORKTREE, Verb::Switch, "refused: desktop has a directory of its own"),
            (MAIN, Verb::Switch, r#"SwitchBranch { checkout: CheckoutId(100), branch: "main" }"#),
            (BRANCH, Verb::Switch, r#"SwitchBranch { checkout: CheckoutId(100), branch: "old" }"#),
            (REMOTE, Verb::Switch, r#"SwitchBranch { checkout: CheckoutId(100), branch: "transcripts" }"#),
            (WORKTREE, Verb::Fetch, "Fetch { checkout: CheckoutId(101) }"),
            (BRANCH, Verb::Fetch, "Fetch { checkout: CheckoutId(100) }"),
            (REMOTE, Verb::Fetch, "Fetch { checkout: CheckoutId(100) }"),
            (WORKTREE, Verb::Pull, "Pull { checkout: CheckoutId(101) }"),
            (BRANCH, Verb::Pull, "Pull { checkout: CheckoutId(100) }"),
        ];
        let repo = repository();
        for (row, verb, expected) in table {
            assert_eq!(said(repo.act(row, verb)), expected, "{verb:?} on {row:?}");
        }
    }

    #[test]
    fn a_branch_row_without_a_primary_checkout_has_nowhere_to_act() {
        let mut repo = repository();
        repo.checkouts[0].primary = false;
        for verb in [Verb::Remove, Verb::Switch, Verb::NewWorktree { name: None, request_id: 0 }] {
            assert_eq!(said(repo.act(BRANCH, verb)), "refused: no primary checkout", "{verb:?}");
        }
        assert_eq!(said(repo.act(BRANCH, Verb::Fetch)), "refused: no checkout to fetch in");
        assert_eq!(said(repo.act(BRANCH, Verb::Pull)), "refused: no checkout to pull into");
    }

    #[test]
    fn a_row_the_tree_no_longer_has_is_refused_rather_than_guessed_at() {
        let repo = repository();
        assert_eq!(said(repo.act(CheckoutRow::Branch(9), Verb::Remove)), "refused: that row is gone");
        for verb in [Verb::Switch, Verb::NewWorktree { name: None, request_id: 0 }] {
            assert_eq!(said(repo.act(CheckoutRow::Remote(9), verb)), "refused: that row is gone", "{verb:?}");
        }
    }

    #[test]
    fn git_runs_in_a_rows_own_checkout_or_the_primary_for_a_branch() {
        let repo = repository();
        assert_eq!(repo.git_runs_in(WORKTREE), Some(CheckoutId(101)));
        assert_eq!(repo.git_runs_in(REMOTE), Some(CheckoutId(100)), "a branch has no directory of its own");
        assert_eq!(repo.row_branch(REMOTE), Some("transcripts"));
        assert_eq!(repo.row_branch(WORKTREE), None);
    }

    #[test]
    fn an_unmerged_refusal_is_asked_again_as_the_forced_deletion() {
        assert_eq!(
            format!("{:?}", force_delete(CheckoutId(100), "old".into())),
            r#"DeleteBranch { checkout: CheckoutId(100), branch: "old", force: true }"#
        );
    }
}
