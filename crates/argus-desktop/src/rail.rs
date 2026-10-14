//! What the rail lists from the tree, and what a row says.
//!
//! The same rail as the terminal client's: the project heading, its
//! repositories with the active ones first and the quiet ones after a rule
//! (`ProjectInfo::repositories_by_activity`), each open repository's
//! checkouts with their git state (`GitStatus::summary`), and their panes.
//! Features are not in it; the Feature stage opens on their index.

use std::collections::HashSet;

use argus_protocol::{
    CheckoutInfo, PaneId, PaneInfo, PaneKind, PaneStatus, ProjectId, ProjectInfo, RepositoryId,
};

use crate::page::{self, Rail};

/// What the rail is drawn from besides the tree.
pub struct Choices<'a> {
    pub project: Option<ProjectId>,
    pub shown: Option<PaneId>,
    /// Repositories opened or closed against their default: an active one
    /// is open until closed, a quiet one closed until opened.
    pub toggled: &'a HashSet<RepositoryId>,
    /// Seconds since the Unix epoch, for how long ago a quiet one was used.
    pub now: u64,
}

pub fn build(tree: &[ProjectInfo], choices: &Choices, loaded: bool) -> Rail {
    let all = || tree.iter().flat_map(statuses);
    Rail {
        projects: tree
            .iter()
            .map(|project| page::ProjectChoice {
                id: project.id.0,
                name: project.name.clone(),
                needs_you: statuses(project).filter(|s| s.needs_you()).count(),
            })
            .collect(),
        project: tree
            .iter()
            .find(|project| Some(project.id) == choices.project)
            .map(|project| build_project(project, choices)),
        needs_you: all().filter(|s| s.needs_you()).count(),
        working: all().filter(|s| *s == PaneStatus::Working).count(),
        shown: choices.shown.and_then(|pane| shown(tree, pane)),
        loaded,
        // The session's to say.
        repository: None,
        stage: "workspace",
        filled: false,
        templates: Vec::new(),
    }
}

fn build_project(project: &ProjectInfo, choices: &Choices) -> page::Project {
    let (active, quiet) = project.repositories_by_activity();
    let divider = quiet.first().copied().filter(|_| !active.is_empty());
    let repositories = active
        .iter()
        .map(|i| (*i, false))
        .chain(quiet.iter().map(|i| (*i, true)))
        .map(|(index, is_quiet)| {
            let repo = &project.repositories[index];
            let holds_shown = repo
                .checkouts
                .iter()
                .any(|c| c.panes.iter().any(|p| Some(p.id) == choices.shown));
            // A pane on screen is never hidden under a closed row.
            let open = holds_shown || is_quiet == choices.toggled.contains(&repo.id);
            let statuses: Vec<PaneStatus> = repo.checkouts.iter().flat_map(|c| c.statuses()).collect();
            page::Repository {
                id: repo.id.0,
                name: repo.name.clone(),
                quiet: is_quiet,
                divider: divider == Some(index),
                open,
                panes: repo.checkouts.iter().map(|c| c.listed_panes().count()).sum(),
                status: PaneStatus::loudest(statuses).map(PaneStatus::word),
                used: is_quiet.then(|| ago(repo.last_used, choices.now)),
                checkouts: if open {
                    repo.checkouts
                        .iter()
                        .filter(|c| c.primary || c.listed_panes().next().is_some())
                        .map(|c| checkout(c, choices.shown))
                        .collect()
                } else {
                    Vec::new()
                },
            }
        })
        .collect();
    page::Project {
        id: project.id.0,
        name: project.name.clone(),
        needs_you: statuses(project).filter(|s| s.needs_you()).count(),
        repositories,
    }
}

/// A checkout row: one with panes running in it, or the repository's
/// primary one even when empty, so there is always somewhere to start a
/// shell. The terminal client's rail lists the same ones.
fn checkout(checkout: &CheckoutInfo, shown: Option<PaneId>) -> page::Checkout {
    page::Checkout {
        id: checkout.id.0,
        name: checkout.name.clone(),
        branch: branch(checkout),
        kind: if checkout.primary { "main" } else { "worktree" },
        git: checkout.git.as_ref().map(|g| g.summary()),
        clean: checkout.git.as_ref().is_some_and(|g| !g.dirty),
        panes: checkout
            .listed_panes()
            .map(|pane| page::Pane {
                id: pane.id.0,
                agent: pane.kind == PaneKind::Agent,
                title: pane.title.clone(),
                status: pane.loudest_state().status.word(),
                note: pane.loudest_state().note.map(str::to_owned),
                on: Some(pane.id) == shown,
            })
            .collect(),
    }
}

fn branch(checkout: &CheckoutInfo) -> String {
    match &checkout.git {
        Some(git) => git.branch.clone().unwrap_or_else(|| "detached".into()),
        None => String::new(),
    }
}

fn shown(tree: &[ProjectInfo], pane: PaneId) -> Option<page::Shown> {
    for repo in tree.iter().flat_map(|p| &p.repositories) {
        for checkout in &repo.checkouts {
            let Some(info) = checkout.panes.iter().find(|p| p.id == pane) else {
                continue;
            };
            return Some(page::Shown {
                pane: pane.0,
                repository: repo.name.clone(),
                checkout: checkout.name.clone(),
                branch: branch(checkout),
                agent: info.kind == PaneKind::Agent,
                title: info.title.clone(),
                status: status(info),
            });
        }
    }
    None
}

fn status(pane: &PaneInfo) -> &'static str {
    pane.loudest_state().status.word()
}

fn statuses(project: &ProjectInfo) -> impl Iterator<Item = PaneStatus> + '_ {
    project
        .repositories
        .iter()
        .flat_map(|r| r.checkouts.iter())
        .flat_map(|c| c.statuses())
}

/// How long ago, the largest unit that fits: `5m`, `2h`, `3d`, `2w`; a
/// dash for never.
pub(crate) fn ago(then: Option<u64>, now: u64) -> String {
    let Some(then) = then else {
        return "—".into();
    };
    let secs = now.saturating_sub(then);
    match secs {
        0..=3599 => format!("{}m", (secs / 60).max(1)),
        3600..=86_399 => format!("{}h", secs / 3600),
        86_400..=604_799 => format!("{}d", secs / 86_400),
        _ => format!("{}w", secs / 604_800),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane, project, repository};
    use argus_protocol::GitStatus;

    fn choices(toggled: &HashSet<RepositoryId>) -> Choices<'_> {
        Choices { project: Some(ProjectId(1)), shown: None, toggled, now: 10_000 }
    }

    fn busy() -> Vec<ProjectInfo> {
        vec![project(1, "src", vec![
            repository(10, "dotfiles", Some(9_000), vec![checkout(100, "main", true, vec![])]),
            repository(11, "argus", None, vec![
                checkout(110, "main", true, vec![
                    pane(1, PaneKind::Agent, "Trim context", PaneStatus::Working),
                    pane(2, PaneKind::Shell, "zsh", PaneStatus::Idle),
                    pane(3, PaneKind::Editor, "vim", PaneStatus::Idle),
                ]),
                checkout(111, "spare", false, vec![]),
                checkout(112, "tracker-links", false, vec![
                    pane(4, PaneKind::Agent, "Tracker", PaneStatus::NeedsReview),
                ]),
            ]),
            repository(12, "learner", Some(1_000), vec![checkout(120, "main", true, vec![])]),
        ])]
    }

    #[test]
    fn active_repositories_lead_and_the_quiet_follow_a_rule_by_last_use() {
        let toggled = HashSet::new();
        let rail = build(&busy(), &choices(&toggled), true);
        let repos = rail.project.unwrap().repositories;
        let order: Vec<_> = repos.iter().map(|r| (r.name.as_str(), r.quiet, r.divider)).collect();
        assert_eq!(order, [("argus", false, false), ("dotfiles", true, true), ("learner", true, false)]);
        assert_eq!(repos[1].used.as_deref(), Some("16m"));
        assert_eq!(repos[2].used.as_deref(), Some("2h"));
    }

    #[test]
    fn an_active_repository_lists_checkouts_with_panes_and_its_primary() {
        let toggled = HashSet::new();
        let rail = build(&busy(), &choices(&toggled), true);
        let argus = &rail.project.unwrap().repositories[0];
        let names: Vec<_> = argus.checkouts.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["main", "tracker-links"], "the empty worktree has nothing to say");
        let panes: Vec<_> = argus.checkouts[0].panes.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(panes, ["Trim context", "zsh"], "an editor is not a pane of the checkout");
        assert_eq!(argus.panes, 3);
        assert_eq!(argus.status, Some("needs-review"));
    }

    #[test]
    fn quiet_repositories_are_closed_until_opened_and_active_ones_the_other_way() {
        let toggled = HashSet::from([RepositoryId(10), RepositoryId(11)]);
        let rail = build(&busy(), &choices(&toggled), true);
        let repos = rail.project.unwrap().repositories;
        assert!(!repos[0].open && repos[0].checkouts.is_empty(), "argus was closed");
        assert!(repos[1].open, "dotfiles was opened");
        assert!(!repos[2].open);
    }

    #[test]
    fn the_repository_holding_the_shown_pane_stays_open() {
        let toggled = HashSet::from([RepositoryId(11)]);
        let mut choices = choices(&toggled);
        choices.shown = Some(PaneId(4));
        let rail = build(&busy(), &choices, true);
        let argus = &rail.project.as_ref().unwrap().repositories[0];
        assert!(argus.open);
        assert!(argus.checkouts[1].panes[0].on);
        let shown = rail.shown.unwrap();
        assert_eq!((shown.repository.as_str(), shown.checkout.as_str()), ("argus", "tracker-links"));
        assert_eq!(shown.status, "needs-review");
    }

    #[test]
    fn counts_cover_every_project_and_the_heading_its_own() {
        let mut tree = busy();
        tree.push(project(2, "work", vec![repository(20, "api", None, vec![
            checkout(200, "main", true, vec![pane(9, PaneKind::Agent, "a", PaneStatus::Waiting)]),
        ])]));
        let toggled = HashSet::new();
        let rail = build(&tree, &choices(&toggled), true);
        assert_eq!((rail.needs_you, rail.working), (2, 1));
        assert_eq!(rail.project.unwrap().needs_you, 1);
        let choices: Vec<_> = rail.projects.iter().map(|p| (p.name.as_str(), p.needs_you)).collect();
        assert_eq!(choices, [("src", 1), ("work", 1)]);
    }

    #[test]
    fn a_checkout_says_its_branch_and_git_state() {
        let mut co = checkout(1, "main", true, vec![]);
        co.git = Some(GitStatus {
            branch: None,
            dirty: true,
            changed_files: 1,
            staged: 0,
            unstaged: 1,
            ahead: 0,
            behind: 1,
            upstream: None,
            last_commit: None,
        });
        let row = super::checkout(&co, None);
        assert_eq!((row.branch.as_str(), row.git.as_deref(), row.clean), ("detached", Some("↓1 !1"), false));
        co.git = None;
        let row = super::checkout(&co, None);
        assert_eq!((row.branch.as_str(), row.git, row.clean), ("", None, false));
    }

    #[test]
    fn a_checkout_says_whether_it_is_the_main_one_or_a_worktree() {
        assert_eq!(super::checkout(&checkout(1, "argus", true, vec![]), None).kind, "main");
        assert_eq!(super::checkout(&checkout(2, "tracker-links", false, vec![]), None).kind, "worktree");
    }

    #[test]
    fn ages_read_in_the_largest_unit_that_fits() {
        assert_eq!(ago(None, 100), "—");
        assert_eq!(ago(Some(100), 100), "1m");
        assert_eq!(ago(Some(0), 7_200), "2h");
        assert_eq!(ago(Some(0), 3 * 86_400), "3d");
        assert_eq!(ago(Some(0), 15 * 86_400), "2w");
    }

    #[test]
    #[ignore]
    fn dump_rail() {
        // The repo's dump-test convention, as text: there is no screen to
        // render a webview to here.
        let toggled = HashSet::new();
        let rail = build(&busy(), &choices(&toggled), true);
        let project = rail.project.unwrap();
        println!("{}  ({} need you)", project.name.to_uppercase(), project.needs_you);
        for repo in project.repositories {
            if repo.divider {
                println!("  ────────");
            }
            let tail = repo.used.unwrap_or_else(|| repo.panes.to_string());
            println!("  {} {}  {}", if repo.open { "▾" } else { "▸" }, repo.name, tail);
            for co in repo.checkouts {
                println!("      {} {} {}  {}", co.name, co.kind, co.branch, co.git.unwrap_or_default());
                for pane in co.panes {
                    let glyph = if pane.agent { "◆" } else { "›" };
                    println!("        {glyph} {}  {}", pane.title, pane.status);
                }
            }
        }
        println!("status bar: ▲ {} need you  ⋮ {} working", rail.needs_you, rail.working);
    }
}
