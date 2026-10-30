//! What the rail's right-click menus offer: a repository's, a checkout's and a pane's
//! (`chrome.ts`, `repoRow`, `checkoutMenu`, `paneMenu`); the brand's is `switcher`.
//!
//! Each is a list of `MenuItem<Does>` that `kit/menu` draws once. A choice only names the calls it
//! sends; the rail changes when the daemon's tree says it did. A choice that goes to another stage
//! first closes what the session has open of Git, as a nav item does.

use argus_desktop_core::chrome::cursor::starts;
use argus_desktop_core::connection::page::{Call, Checkout, Pane, Repository};

use crate::kit::menu::item::MenuItem;

/// What a choice of a rail menu does.
#[derive(Debug, Clone)]
pub enum Does {
    /// Sends these calls.
    Send(Vec<Call>),
    /// Sends these calls after closing what the session has open of Git: the choice goes to a stage
    /// or brings a pane on screen.
    Go(Vec<Call>),
    /// Opens the rename dialog over the pane, once the menu has let go of the keys.
    Rename { pane: u64, title: String },
    /// Opens the dialog that names a workspace to make.
    NewWorkspace,
}

/// A repository's menu: its name, Features, the recent sessions, a rule, and Collapse or Expand.
/// Features chooses the repository first when it is not the chosen one.
pub fn repository(repo: &Repository, chosen: bool) -> Vec<MenuItem<Does>> {
    let mut features = Vec::new();
    if !chosen {
        features.push(Call::ChooseRepository { repository: repo.id });
    }
    features.push(Call::ShowFeatures { open: true });
    vec![
        MenuItem::heading(repo.name.clone()),
        MenuItem::choice("Features", Does::Go(features)),
        MenuItem::choice("Recent agent sessions", Does::Send(vec![Call::OpenSessions { repository: repo.id }])),
        MenuItem::Rule,
        MenuItem::choice(
            if repo.open { "Collapse" } else { "Expand" },
            Does::Send(vec![Call::ToggleRepository { repository: repo.id }]),
        ),
    ]
}

/// A checkout's menu: the branch, a new shell and a new agent per template, a rule, its review, its
/// history, the Checkouts stage on it, its fold when it has panes, and for a worktree a rule and
/// its removal in danger.
pub fn checkout(checkout: &Checkout, templates: &[String]) -> Vec<MenuItem<Does>> {
    let id = checkout.id;
    let mut items = vec![MenuItem::heading(if checkout.branch.is_empty() { checkout.name.clone() } else { checkout.branch.clone() })];
    items.extend(starts(templates).into_iter().map(|start| {
        let label = match &start.agent {
            Some(agent) => format!("New agent \u{b7} {agent}"),
            None => "New shell".to_owned(),
        };
        MenuItem::choice(label, Does::Send(vec![Call::StartOn { checkout: id, template: start.agent }]))
    }));
    items.push(MenuItem::Rule);
    items.push(MenuItem::choice("Review changes", Does::Send(vec![Call::OpenReview { checkout: id }])));
    items.push(MenuItem::choice("History", Does::Send(vec![Call::OpenHistory { checkout: id }])));
    items.push(MenuItem::choice("Show in Checkouts", Does::Go(vec![Call::GoToCheckout { checkout: id }])));
    if checkout.count > 0 {
        items.push(MenuItem::choice(
            if checkout.open { "Collapse" } else { "Expand" },
            Does::Send(vec![Call::ToggleCheckout { checkout: id }]),
        ));
    }
    if checkout.kind == "worktree" {
        items.push(MenuItem::Rule);
        items.push(MenuItem::choice("Remove worktree\u{2026}", Does::Go(vec![Call::RemoveCheckout { checkout: id }])).danger());
    }
    items
}

/// A pane's menu: its title, Show (which picks it, as a click does), Open beside, Rename, a rule,
/// and Close in danger.
pub fn pane(checkout: u64, pane: &Pane) -> Vec<MenuItem<Does>> {
    vec![
        MenuItem::heading(pane.title.clone()),
        MenuItem::choice(
            "Show",
            Does::Go(vec![Call::RailPick { checkout, pane: Some(pane.id) }, Call::Select { pane: pane.id }]),
        ),
        MenuItem::choice("Open beside", Does::Send(vec![Call::OpenBeside { pane: pane.id }])),
        MenuItem::choice("Rename\u{2026}", Does::Rename { pane: pane.id, title: pane.title.clone() }),
        MenuItem::Rule,
        MenuItem::choice("Close", Does::Send(vec![Call::Close { pane: pane.id }])).danger(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::menu::item::lines;

    fn checkout_of(kind: &'static str, count: usize, open: bool) -> Checkout {
        Checkout {
            id: 110,
            name: "main".into(),
            branch: "feature".into(),
            kind,
            git: None,
            clean: true,
            shared: None,
            open,
            count,
            status: None,
            panes: Vec::new(),
        }
    }

    fn repo(open: bool) -> Repository {
        Repository {
            id: 11,
            name: "argus".into(),
            quiet: false,
            divider: false,
            open,
            panes: 0,
            status: None,
            used: None,
            checkouts: Vec::new(),
        }
    }

    fn calls(item: &MenuItem<Does>) -> Vec<String> {
        match &item.as_choice().expect("a choice").does {
            Does::Send(calls) | Does::Go(calls) => calls.iter().map(|call| format!("{call:?}")).collect(),
            Does::Rename { pane, .. } => vec![format!("rename {pane}")],
            Does::NewWorkspace => vec!["new workspace".into()],
        }
    }

    #[test]
    fn a_repositorys_menu_says_its_name_and_offers_its_features_sessions_and_fold() {
        // SHELL-RAIL-6.
        let items = repository(&repo(true), true);
        assert_eq!(lines(&items), ["# argus", "Features", "Recent agent sessions", "---", "Collapse"]);
        assert_eq!(lines(&repository(&repo(false), true))[4], "Expand");
    }

    #[test]
    fn features_chooses_the_repository_first_unless_it_is_the_chosen_one() {
        let items = repository(&repo(true), false);
        assert_eq!(calls(&items[1]), ["ChooseRepository { repository: 11 }", "ShowFeatures { open: true }"]);
        assert!(matches!(items[1].as_choice().unwrap().does, Does::Go(_)), "it goes to a stage, so it leaves Git");
        assert_eq!(calls(&repository(&repo(true), true)[1]), ["ShowFeatures { open: true }"]);
    }

    #[test]
    fn a_checkouts_menu_starts_a_shell_and_each_agent_then_reviews_and_folds() {
        // SHELL-RAIL-10.
        let items = checkout(&checkout_of("main", 2, true), &["claude".to_owned(), "codex".to_owned()]);
        assert_eq!(
            lines(&items),
            [
                "# feature",
                "New shell",
                "New agent \u{b7} claude",
                "New agent \u{b7} codex",
                "---",
                "Review changes",
                "History",
                "Show in Checkouts",
                "Collapse"
            ]
        );
        assert_eq!(calls(&items[1]), ["StartOn { checkout: 110, template: None }"]);
        assert_eq!(calls(&items[3]), ["StartOn { checkout: 110, template: Some(\"codex\") }"]);
    }

    #[test]
    fn an_empty_checkout_offers_no_fold_and_a_folded_one_offers_expand() {
        assert!(!lines(&checkout(&checkout_of("main", 0, true), &[])).contains(&"Collapse".to_owned()));
        assert_eq!(lines(&checkout(&checkout_of("main", 2, false), &[])).last().map(String::as_str), Some("Expand"));
    }

    #[test]
    fn a_checkout_named_by_its_name_when_it_has_no_branch() {
        let mut bare = checkout_of("main", 0, true);
        bare.branch.clear();
        assert_eq!(lines(&checkout(&bare, &[]))[0], "# main");
    }

    #[test]
    fn a_worktree_alone_offers_its_removal_in_danger_after_a_rule() {
        let main = lines(&checkout(&checkout_of("main", 1, true), &[]));
        assert!(!main.iter().any(|line| line.contains("Remove")));
        let worktree = checkout(&checkout_of("worktree", 1, true), &[]);
        let lines = lines(&worktree);
        assert_eq!(&lines[lines.len() - 2..], ["---", "!Remove worktree\u{2026}"]);
        assert_eq!(calls(worktree.last().unwrap()), ["RemoveCheckout { checkout: 110 }"]);
    }

    #[test]
    fn a_panes_menu_shows_picks_opens_beside_renames_and_closes_in_danger() {
        // SHELL-RAIL-15.
        let pane = Pane { id: 2, agent: false, title: "zsh".into(), status: "idle", note: None, on: false, views: 0 };
        let items = super::pane(110, &pane);
        assert_eq!(lines(&items), ["# zsh", "Show", "Open beside", "Rename\u{2026}", "---", "!Close"]);
        assert_eq!(calls(&items[1]), ["RailPick { checkout: 110, pane: Some(2) }", "Select { pane: 2 }"]);
        assert_eq!(calls(&items[2]), ["OpenBeside { pane: 2 }"]);
        assert!(matches!(&items[3].as_choice().unwrap().does, Does::Rename { pane: 2, title } if title == "zsh"));
        assert_eq!(calls(&items[5]), ["Close { pane: 2 }"]);
    }
}
