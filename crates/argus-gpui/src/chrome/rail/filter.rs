//! What the filter chips keep of the tree: all of it, the panes that need the person, or the panes
//! at work, and the repositories that fold into the quiet row.
//!
//! The chip is the window's own state (it is a way of looking, not something the daemon holds), so
//! a press changes the view at once and sends nothing. A repository or checkout is kept when a pane
//! under it is; a shut one lists no panes, so its loudest status word speaks for them, as the
//! rail's dot does. Quiet repositories have no panes, so every filter but All puts them away.

use argus_desktop_core::connection::page::{Checkout, Pane, Project, Repository};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Filter {
    #[default]
    All,
    NeedsYou,
    Working,
}

impl Filter {
    /// Whether a pane of `status` stays: what the daemon calls needing the person is waiting,
    /// review and failed (`PaneStatus::needs_you`).
    pub fn keeps_status(self, status: &str) -> bool {
        match self {
            Filter::All => true,
            Filter::NeedsYou => matches!(status, "waiting" | "needs-review" | "failed"),
            Filter::Working => status == "working",
        }
    }

    pub fn keeps_pane(self, pane: &Pane) -> bool {
        self.keeps_status(pane.status)
    }

    pub fn keeps_checkout(self, checkout: &Checkout) -> bool {
        self == Filter::All
            || if checkout.open { checkout.panes.iter().any(|pane| self.keeps_pane(pane)) } else { checkout.status.is_some_and(|status| self.keeps_status(status)) }
    }

    pub fn keeps_repository(self, repo: &Repository) -> bool {
        self == Filter::All
            || if repo.open { repo.checkouts.iter().any(|checkout| self.keeps_checkout(checkout)) } else { repo.status.is_some_and(|status| self.keeps_status(status)) }
    }
}

/// The repositories the tree draws, and the quiet ones it folds into one row.
pub(super) struct Listed<'a> {
    pub shown: Vec<&'a Repository>,
    pub quiet: Vec<&'a Repository>,
}

/// A quiet repository folds away unless it is the chosen one: the repository the person is working
/// in must not vanish from the list because nothing runs in it.
pub(super) fn listed(project: &Project, chosen: Option<u64>, filter: Filter) -> Listed<'_> {
    let mut out = Listed { shown: Vec::new(), quiet: Vec::new() };
    for repo in &project.repositories {
        let folds = repo.quiet && Some(repo.id) != chosen;
        if folds && filter == Filter::All {
            out.quiet.push(repo);
        } else if !folds && filter.keeps_repository(repo) {
            out.shown.push(repo);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chrome::rail::tests::{busy, checkout_row, pane};

    #[test]
    fn a_filter_keeps_the_panes_of_its_status_and_the_checkouts_and_repositories_holding_them() {
        let rail = busy();
        let argus = &rail.project.as_ref().unwrap().repositories[0];
        let working = argus.checkouts[0].panes.iter().filter(|pane| Filter::Working.keeps_pane(pane)).count();
        assert_eq!(working, 1);
        assert!(Filter::NeedsYou.keeps_checkout(&argus.checkouts[0]), "its review pane needs a look");
        assert!(Filter::Working.keeps_repository(argus));
        assert!(!Filter::Working.keeps_checkout(&checkout_row(1, "main", true, vec![pane(1, false, "zsh", "idle", false)])));
    }

    #[test]
    fn a_shut_checkout_speaks_by_its_loudest_status() {
        let argus = &busy().project.unwrap().repositories[0];
        let folded = &argus.checkouts[1];
        assert!(!folded.open);
        assert!(Filter::NeedsYou.keeps_checkout(folded), "waiting");
        assert!(!Filter::Working.keeps_checkout(folded));
    }

    #[test]
    fn quiet_repositories_fold_unless_chosen_and_every_filter_but_all_puts_them_away() {
        let rail = busy();
        let project = rail.project.as_ref().unwrap();
        let all = listed(project, Some(11), Filter::All);
        assert_eq!((all.shown.len(), all.quiet.len()), (1, 1));
        let chosen = listed(project, Some(10), Filter::All);
        assert_eq!((chosen.shown.len(), chosen.quiet.len()), (2, 0), "the chosen repository stays in the list");
        let working = listed(project, Some(11), Filter::Working);
        assert_eq!((working.shown.len(), working.quiet.len()), (1, 0));
    }
}
