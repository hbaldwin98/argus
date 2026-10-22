//! What the stage says when no pane is on screen: waiting for the daemon, no project to work in,
//! or a project with nothing shown (`drawStage`'s empty branches).
//!
//! The choice and the words that depend on the tree are here, once, so a window draws a card and
//! decides nothing: which state it is, the way out of a connection that does not come, and the
//! checkout a shell is offered on, which is the first of the repository chosen, else of the first
//! repository.

use crate::connection::page::{Project, Rail, RunningPane};

/// The one way out of a connection that has not answered. Nothing is loaded to open the switcher
/// from, so there is always exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WayOut {
    /// A remote host is being reached: go back to this machine.
    ThisMachine,
    Settings,
}

/// Where a shell is offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellHere {
    pub checkout: u64,
    /// The button: `Open a shell on <branch>`, or `Open a shell` outside git.
    pub words: String,
    /// The line under the buttons: the branch (or the checkout's name), and the repository.
    pub branch_or_name: String,
    pub repository: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoPane {
    pub project: String,
    pub shell: Option<ShellHere>,
    /// Every pane in the project, the most recently changed first, however the rail is folded.
    pub running: Vec<RunningPane>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Empty {
    /// The daemon has not sent a tree.
    Connecting {
        /// `None` is this machine.
        host: Option<String>,
        /// What the last failed try said.
        detail: Option<String>,
        way_out: WayOut,
    },
    NoProjects,
    NoPane(NoPane),
}

/// The empty stage for `rail`, or `None` while a pane is on screen.
pub fn of(rail: &Rail, host: Option<&str>, detail: Option<&str>) -> Option<Empty> {
    if rail.shown.is_some() {
        return None;
    }
    if !rail.loaded {
        return Some(Empty::Connecting {
            host: host.map(str::to_owned),
            detail: detail.map(str::to_owned),
            way_out: if host.is_some() { WayOut::ThisMachine } else { WayOut::Settings },
        });
    }
    Some(match &rail.project {
        None => Empty::NoProjects,
        Some(project) => Empty::NoPane(NoPane { project: project.name.clone(), shell: shell_here(project, rail.repository), running: project.running.clone() }),
    })
}

fn shell_here(project: &Project, chosen: Option<u64>) -> Option<ShellHere> {
    let repository = project.repositories.iter().find(|repository| Some(repository.id) == chosen).or(project.repositories.first())?;
    let checkout = repository.checkouts.first()?;
    Some(ShellHere {
        checkout: checkout.id,
        words: if checkout.branch.is_empty() { "Open a shell".to_owned() } else { format!("Open a shell on {}", checkout.branch) },
        branch_or_name: if checkout.branch.is_empty() { checkout.name.clone() } else { checkout.branch.clone() },
        repository: repository.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::page::{Checkout, Repository, Shown};

    fn checkout(id: u64, name: &str, branch: &str) -> Checkout {
        Checkout {
            id,
            name: name.into(),
            branch: branch.into(),
            kind: "main",
            git: None,
            clean: false,
            shared: None,
            open: true,
            count: 0,
            status: None,
            panes: Vec::new(),
        }
    }

    fn repository(id: u64, name: &str, checkouts: Vec<Checkout>) -> Repository {
        Repository { id, name: name.into(), quiet: false, divider: false, open: true, panes: 0, status: None, used: None, checkouts }
    }

    fn loaded(repositories: Vec<Repository>, chosen: Option<u64>) -> Rail {
        let project = Project { id: 1, name: "src".into(), needs_you: 0, repositories, running: Vec::new() };
        Rail { loaded: true, project: Some(project), repository: chosen, ..Rail::default() }
    }

    fn no_pane(rail: &Rail) -> NoPane {
        match of(rail, None, None) {
            Some(Empty::NoPane(no_pane)) => no_pane,
            other => panic!("expected the stage with no pane, got {other:?}"),
        }
    }

    #[test]
    fn a_pane_on_screen_leaves_the_stage_to_it() {
        let mut rail = loaded(Vec::new(), None);
        rail.shown = Some(Shown {
            pane: 1,
            repository: "a".into(),
            checkout: "b".into(),
            branch: String::new(),
            agent: false,
            browser: false,
            title: "t".into(),
            status: "idle",
            resumed: None,
        });
        assert_eq!(of(&rail, None, None), None);
    }

    #[test]
    fn no_tree_heard_is_connecting_with_one_way_out() {
        let rail = Rail::default();
        assert_eq!(of(&rail, None, Some("spawn failed")), Some(Empty::Connecting { host: None, detail: Some("spawn failed".into()), way_out: WayOut::Settings }));
        assert_eq!(
            of(&rail, Some("build-box"), None),
            Some(Empty::Connecting { host: Some("build-box".into()), detail: None, way_out: WayOut::ThisMachine })
        );
    }

    #[test]
    fn a_tree_with_no_project_is_the_stage_that_adds_one() {
        let rail = Rail { loaded: true, ..Rail::default() };
        assert_eq!(of(&rail, None, None), Some(Empty::NoProjects));
    }

    #[test]
    fn a_shell_is_offered_on_the_first_checkout_of_the_repository_chosen() {
        let rail = loaded(
            vec![repository(10, "dotfiles", vec![checkout(100, "main", "trunk")]), repository(11, "argus", vec![checkout(110, "main", "dev"), checkout(111, "spare", "x")])],
            Some(11),
        );
        let shell = no_pane(&rail).shell.expect("a checkout");
        assert_eq!((shell.checkout, shell.words.as_str(), shell.branch_or_name.as_str(), shell.repository.as_str()), (110, "Open a shell on dev", "dev", "argus"));
    }

    #[test]
    fn with_no_repository_chosen_the_first_repository_is_used() {
        let rail = loaded(vec![repository(10, "dotfiles", vec![checkout(100, "main", "trunk")]), repository(11, "argus", vec![checkout(110, "main", "dev")])], None);
        assert_eq!(no_pane(&rail).shell.map(|shell| shell.checkout), Some(100));
        let stale = loaded(vec![repository(10, "dotfiles", vec![checkout(100, "main", "trunk")])], Some(99));
        assert_eq!(no_pane(&stale).shell.map(|shell| shell.checkout), Some(100), "a repository gone is no choice");
    }

    #[test]
    fn a_checkout_outside_git_is_named_by_its_name_and_the_button_names_no_branch() {
        let rail = loaded(vec![repository(10, "notes", vec![checkout(100, "notes", "")])], None);
        let shell = no_pane(&rail).shell.expect("a checkout");
        assert_eq!((shell.words.as_str(), shell.branch_or_name.as_str()), ("Open a shell", "notes"));
    }

    #[test]
    fn a_repository_with_no_checkout_offers_no_shell_and_the_project_still_lists_its_panes() {
        let rail = loaded(vec![repository(10, "notes", Vec::new())], None);
        let page = no_pane(&rail);
        assert_eq!((page.shell, page.project.as_str()), (None, "src"));
    }

    #[test]
    fn every_pane_of_the_project_is_offered_in_the_order_the_rail_sent() {
        let mut rail = loaded(vec![repository(10, "notes", Vec::new())], None);
        let running = |id| RunningPane { id, agent: true, title: format!("p{id}"), status: "idle", note: None, repository: "notes".into(), checkout: "main".into(), used: None };
        rail.project.as_mut().unwrap().running = vec![running(3), running(1)];
        let ids: Vec<_> = no_pane(&rail).running.iter().map(|pane| pane.id).collect();
        assert_eq!(ids, [3, 1]);
    }
}
