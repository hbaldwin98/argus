//! The order a repository's checkouts column lists its rows, said once for
//! every client: the main branch leads, whether a checkout sits on it or a
//! row offers it, then the other checkouts, then — only while branches are
//! shown — the branches no checkout is on and what a fetch turned up.

pub mod verbs;

use crate::{CheckoutInfo, RepositoryInfo};

/// One row of the column, as an index into the repository's own
/// `checkouts`, `branches` or `remote_branches`. The kinds interleave, so
/// the column's order is [`RepositoryInfo::column`] rather than any one
/// list on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutRow {
    Checkout(usize),
    Branch(usize),
    /// A branch that exists on a remote and nowhere else, held as
    /// `origin/feature`.
    Remote(usize),
}

impl RepositoryInfo {
    /// The column's rows, with the branches no checkout is on only when
    /// `branches`: a repository with forty would bury the checkouts that
    /// are the point of the column. The main branch's row leads either way.
    pub fn column(&self, branches: bool) -> Vec<CheckoutRow> {
        let default = self.default_branch.as_deref();
        // `branches` only holds the ones no checkout has, so a hit here
        // means the main branch has no directory and needs a row of its own.
        let pinned = default.and_then(|d| self.branches.iter().position(|b| b == d));
        let leads = |i: &usize| default.is_some_and(|d| self.checkouts[*i].on_branch(d));

        let mut rows: Vec<CheckoutRow> = pinned.map(CheckoutRow::Branch).into_iter().collect();
        rows.extend((0..self.checkouts.len()).filter(leads).map(CheckoutRow::Checkout));
        rows.extend((0..self.checkouts.len()).filter(|i| !leads(i)).map(CheckoutRow::Checkout));
        if branches {
            rows.extend((0..self.branches.len()).filter(|i| Some(*i) != pinned).map(CheckoutRow::Branch));
            // What a fetch turned up comes last: it is the furthest from
            // being somewhere you can work.
            rows.extend((0..self.remote_branches.len()).map(CheckoutRow::Remote));
        }
        rows
    }
}

impl CheckoutInfo {
    /// Whether this checkout is the one sitting on `branch`. Its git status
    /// is the truth; the row's name stands in only until the first poll has
    /// been round.
    pub fn on_branch(&self, branch: &str) -> bool {
        self.git.as_ref().and_then(|g| g.branch.as_deref()).unwrap_or(&self.name) == branch
    }
}

/// The local branch a remote-only one would become: `feature/x` for
/// `origin/feature/x`. Remote names have no slash in them, so the first one
/// is the split. Switching to it, or giving it a worktree, names that and
/// leaves git to take it from the remote.
pub fn local_name(remote_branch: &str) -> Option<&str> {
    remote_branch.split_once('/').map(|(_, rest)| rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CheckoutId, GitStatus, RepositoryId};

    fn checkout(id: u64, branch: &str) -> CheckoutInfo {
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
            primary: id == 1,
        }
    }

    fn repository(checkouts: Vec<CheckoutInfo>, branches: &[&str], default: &str) -> RepositoryInfo {
        RepositoryInfo {
            id: RepositoryId(1),
            name: "argus".into(),
            checkouts,
            branches: branches.iter().map(|b| b.to_string()).collect(),
            default_branch: Some(default.into()),
            remote_branches: vec!["origin/transcripts".into()],
            unmerged: Default::default(),
            last_used: None,
        }
    }

    #[test]
    fn the_main_branchs_checkout_leads_and_branches_wait_to_be_asked_for() {
        let repo = repository(vec![checkout(1, "desktop"), checkout(2, "main")], &["old"], "main");
        assert_eq!(repo.column(false), [CheckoutRow::Checkout(1), CheckoutRow::Checkout(0)]);
        assert_eq!(
            repo.column(true),
            [CheckoutRow::Checkout(1), CheckoutRow::Checkout(0), CheckoutRow::Branch(0), CheckoutRow::Remote(0)]
        );
    }

    #[test]
    fn a_main_branch_no_checkout_is_on_still_leads_as_a_branch_row() {
        let repo = repository(vec![checkout(1, "desktop")], &["main", "old"], "main");
        assert_eq!(repo.column(false), [CheckoutRow::Branch(0), CheckoutRow::Checkout(0)]);
        assert_eq!(repo.column(true)[2], CheckoutRow::Branch(1), "and is not listed twice");
    }

    #[test]
    fn a_remote_branch_answers_to_its_local_name() {
        assert_eq!(local_name("origin/feature/x"), Some("feature/x"));
        assert_eq!(local_name("nothing"), None);
    }
}
