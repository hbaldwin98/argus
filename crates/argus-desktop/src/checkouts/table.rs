//! What the Checkouts stage lists from the chosen repository: its rows in
//! the protocol's column order, what each says, and the selected
//! checkout's detail.

use argus_protocol::tree::column::{local_name, CheckoutRow};
use argus_protocol::{CheckoutInfo, DecisionBoard, GitStatus, PaneKind, RepositoryInfo};
use serde::Serialize;

use crate::checkouts::stage::Confirm;

/// The Checkouts stage: the chosen repository's checkouts, and the
/// branches no checkout is on while they are shown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Checkouts {
    /// The repository it lists; `None` before one is chosen.
    pub repository: Option<String>,
    pub checkouts: usize,
    /// Local and remote branches no checkout is on, shown or not.
    pub branches: usize,
    /// The repository's main branch, as the heading names it.
    pub main: Option<String>,
    pub branches_shown: bool,
    pub filter: String,
    pub rows: Vec<CheckoutsRow>,
    /// The selected row's `key`.
    pub selected: Option<String>,
    pub detail: Option<CheckoutDetail>,
    /// A question the stage is waiting on an answer to.
    pub confirm: Option<Confirm>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CheckoutsRow {
    /// Names the row in what the page sends back: `c<id>`, `b<branch>` or
    /// `r<origin/branch>`.
    pub key: String,
    /// The checkout's id; `None` for a branch with no directory.
    pub checkout: Option<u64>,
    /// The checkout's name; `None` for a branch row.
    pub name: Option<String>,
    pub primary: bool,
    pub branch: String,
    /// The compact git state, as on the rail; a branch row has none.
    pub git: Option<String>,
    pub clean: bool,
    /// Said beside the git state: `remote only` for what a fetch turned up,
    /// `merged` for a branch main already has.
    pub note: Option<&'static str>,
    /// What was last committed here; a branch row has none.
    pub last: Option<LastCommit>,
    pub running: Vec<Running>,
    /// `CheckoutInfo::shared_by`; a branch row has no agents to share it.
    pub shared: Option<usize>,
    /// The number of the feature this checkout is on, when the board says.
    pub feature: Option<i64>,
    /// That feature is the one the Feature stage has under its cursor.
    pub viewed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub agent: bool,
    #[cfg_attr(test, ts(as = "crate::connection::page::StatusWord"))]
    pub status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LastCommit {
    pub short: String,
    pub summary: String,
    /// How long ago, as the rail says it: `2h`.
    pub ago: String,
}

/// The selected checkout, said in full along the stage's foot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CheckoutDetail {
    pub name: String,
    pub path: String,
    /// Its upstream and how far it is from it: `origin/main · 1 behind`;
    /// `None` when git has not been read.
    pub upstream: Option<String>,
    /// `1 unstaged file`, `3 staged · 2 unstaged files`, `no changes`.
    pub changes: Option<String>,
}

/// What the table is asked to show besides the repository itself.
pub struct View<'a> {
    pub branches_shown: bool,
    pub filter: &'a str,
    /// The selected row's key, as the page last chose it.
    pub selected: Option<&'a str>,
    pub board: Option<&'a DecisionBoard>,
    /// The feature the Feature stage has under its cursor.
    pub viewed: Option<&'a str>,
    /// Seconds since the epoch, for how long ago a commit was.
    pub now: u64,
}

/// A commit as a row or a dialog says it.
pub fn last_commit(commit: &argus_protocol::CommitInfo, now: u64) -> LastCommit {
    LastCommit {
        short: commit.short.clone(),
        summary: commit.summary.clone(),
        ago: argus_protocol::ago(u64::try_from(commit.time).ok(), now),
    }
}

/// The key a row is chosen by: stable while rows come and go around it.
pub fn key(repo: &RepositoryInfo, row: CheckoutRow) -> Option<String> {
    Some(match row {
        CheckoutRow::Checkout(i) => format!("c{}", repo.checkouts.get(i)?.id.0),
        CheckoutRow::Branch(i) => format!("b{}", repo.branches.get(i)?),
        CheckoutRow::Remote(i) => format!("r{}", repo.remote_branches.get(i)?),
    })
}

/// The rows the table shows, filtered: the column's own order, with a
/// filter matching a row by checkout or branch name, ignoring case.
pub fn rows(repo: &RepositoryInfo, branches_shown: bool, filter: &str) -> Vec<CheckoutRow> {
    let wanted = filter.trim().to_lowercase();
    repo.column(branches_shown)
        .into_iter()
        .filter(|row| {
            wanted.is_empty() || {
                let (name, branch) = names(repo, *row);
                name.to_lowercase().contains(&wanted) || branch.to_lowercase().contains(&wanted)
            }
        })
        .collect()
}

/// The selected row: the one the page chose while it is still listed, else
/// the first, so the table always has a row its keys act on.
pub fn selected(repo: &RepositoryInfo, rows: &[CheckoutRow], chosen: Option<&str>) -> Option<CheckoutRow> {
    chosen
        .and_then(|chosen| rows.iter().find(|row| key(repo, **row).as_deref() == Some(chosen)))
        .or(rows.first())
        .copied()
}

pub fn build(repo: &RepositoryInfo, view: &View) -> Checkouts {
    let listed = rows(repo, view.branches_shown, view.filter);
    let selected = selected(repo, &listed, view.selected);
    Checkouts {
        repository: Some(repo.name.clone()),
        checkouts: repo.checkouts.len(),
        branches: repo.branches.len() + repo.remote_branches.len(),
        main: repo.default_branch.clone(),
        branches_shown: view.branches_shown,
        filter: view.filter.to_owned(),
        rows: listed.iter().filter_map(|row| row_of(repo, *row, view)).collect(),
        selected: selected.and_then(|row| key(repo, row)),
        detail: match selected {
            Some(CheckoutRow::Checkout(i)) => repo.checkouts.get(i).map(detail),
            _ => None,
        },
        // The stage's to say.
        confirm: None,
    }
}

fn row_of(repo: &RepositoryInfo, row: CheckoutRow, view: &View) -> Option<CheckoutsRow> {
    let key = key(repo, row)?;
    let feature = |found: Option<&argus_protocol::Feature>| {
        (found.map(|f| f.number).filter(|n| *n > 0), found.is_some_and(|f| Some(f.slug.as_str()) == view.viewed))
    };
    let features = view.board.map(|b| b.features.as_slice()).unwrap_or_default();
    Some(match row {
        CheckoutRow::Checkout(i) => {
            let c = repo.checkouts.get(i)?;
            let (feature, viewed) = feature(features.iter().find(|f| f.checkouts.contains(&c.path)));
            CheckoutsRow {
                key,
                checkout: Some(c.id.0),
                name: Some(c.name.clone()),
                primary: c.primary,
                branch: branch(c),
                git: c.git.as_ref().map(GitStatus::summary),
                clean: c.git.as_ref().is_some_and(|g| !g.dirty),
                note: None,
                last: c.git.as_ref().and_then(|g| g.last_commit.as_ref()).map(|commit| last_commit(commit, view.now)),
                running: c
                    .listed_panes()
                    .map(|p| Running { agent: p.kind == PaneKind::Agent, status: p.loudest_state().status.word() })
                    .collect(),
                shared: c.shared_by(),
                feature,
                viewed,
            }
        }
        CheckoutRow::Branch(_) | CheckoutRow::Remote(_) => {
            let remote = matches!(row, CheckoutRow::Remote(_));
            let (_, name) = names(repo, row);
            let local = if remote { local_name(&name).unwrap_or(&name) } else { &name };
            let (feature, viewed) =
                feature(features.iter().find(|f| f.origin_branch.as_deref() == Some(local)));
            // How far ahead of main, where the daemon counted it: a
            // remote-only branch has no local commits to count.
            let ahead = if remote { None } else { repo.unmerged.get(&name).copied() };
            CheckoutsRow {
                key,
                checkout: None,
                name: None,
                primary: false,
                git: ahead.filter(|n| *n > 0).map(|n| format!("↑{n}")),
                branch: name,
                clean: false,
                note: match (remote, ahead) {
                    (true, _) => Some("remote only"),
                    (false, Some(0)) => Some("merged"),
                    (false, Some(_)) => Some("unmerged"),
                    (false, None) => None,
                },
                last: None,
                running: Vec::new(),
                shared: None,
                feature,
                viewed,
            }
        }
    })
}

/// A row's checkout name (empty for a branch row) and branch.
fn names(repo: &RepositoryInfo, row: CheckoutRow) -> (String, String) {
    match row {
        CheckoutRow::Checkout(i) => repo.checkouts.get(i).map(|c| (c.name.clone(), branch(c))).unwrap_or_default(),
        CheckoutRow::Branch(i) => (String::new(), repo.branches.get(i).cloned().unwrap_or_default()),
        CheckoutRow::Remote(i) => (String::new(), repo.remote_branches.get(i).cloned().unwrap_or_default()),
    }
}

fn branch(c: &CheckoutInfo) -> String {
    match &c.git {
        Some(git) => git.branch.clone().unwrap_or_else(|| "detached".into()),
        None => String::new(),
    }
}

fn detail(c: &CheckoutInfo) -> CheckoutDetail {
    CheckoutDetail {
        name: c.name.clone(),
        path: c.path.clone(),
        upstream: c.git.as_ref().map(|g| {
            let distance = match (g.ahead, g.behind) {
                (0, 0) => "in sync".to_string(),
                (a, 0) => format!("{a} ahead"),
                (0, b) => format!("{b} behind"),
                (a, b) => format!("{a} ahead · {b} behind"),
            };
            match &g.upstream {
                Some(upstream) => format!("{upstream} · {distance}"),
                None if g.ahead == 0 && g.behind == 0 => "none".to_string(),
                None => distance,
            }
        }),
        changes: c.git.as_ref().map(changes),
    }
}

fn changes(g: &GitStatus) -> String {
    let files = |n: usize| if n == 1 { "file" } else { "files" };
    match (g.staged, g.unstaged) {
        _ if !g.dirty => "no changes".into(),
        (0, 0) => format!("{} changed {}", g.changed_files, files(g.changed_files)),
        (s, 0) => format!("{s} staged {}", files(s)),
        (0, u) => format!("{u} unstaged {}", files(u)),
        (s, u) => format!("{s} staged · {u} unstaged {}", files(u)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{checkout, pane, repository};
    use argus_protocol::{PaneKind, PaneStatus};

    fn view<'a>(branches_shown: bool, filter: &'a str, selected: Option<&'a str>) -> View<'a> {
        View { branches_shown, filter, selected, board: None, viewed: None, now: 0 }
    }

    fn repo() -> RepositoryInfo {
        let mut repo = repository(10, "argus", None, vec![
            checkout(100, "main", true, Vec::new()),
            checkout(101, "desktop", false, Vec::new()),
        ]);
        repo.default_branch = Some("main".into());
        repo.branches = vec!["old".into()];
        repo.remote_branches = vec!["origin/transcripts".into()];
        repo
    }

    fn keys(table: &Checkouts) -> Vec<&str> {
        table.rows.iter().map(|r| r.key.as_str()).collect()
    }

    #[test]
    fn checkouts_lead_and_branches_follow_only_when_shown() {
        assert_eq!(keys(&build(&repo(), &view(false, "", None))), ["c100", "c101"]);
        let all = build(&repo(), &view(true, "", None));
        assert_eq!(keys(&all), ["c100", "c101", "bold", "rorigin/transcripts"]);
        assert_eq!(all.rows[3].note, Some("remote only"));
        assert_eq!((all.checkouts, all.branches), (2, 2));
    }

    #[test]
    fn a_filter_keeps_the_rows_whose_names_match() {
        assert_eq!(keys(&build(&repo(), &view(true, "TRANS", None))), ["rorigin/transcripts"]);
    }

    #[test]
    fn the_selection_holds_while_listed_and_falls_to_the_first_row() {
        assert_eq!(build(&repo(), &view(false, "", Some("c101"))).selected.as_deref(), Some("c101"));
        assert_eq!(build(&repo(), &view(false, "", Some("bold"))).selected.as_deref(), Some("c100"));
        assert!(build(&repo(), &view(false, "nothing", None)).selected.is_none());
    }

    #[test]
    fn a_selected_checkout_is_said_in_full_and_a_branch_is_not() {
        let table = build(&repo(), &view(true, "", Some("c101")));
        assert_eq!(table.detail.as_ref().map(|d| d.name.as_str()), Some("desktop"));
        assert!(build(&repo(), &view(true, "", Some("bold"))).detail.is_none());
    }

    #[test]
    fn a_branch_row_says_whether_main_has_it() {
        let mut repo = repo();
        repo.branches = vec!["spike".into(), "done".into()];
        repo.unmerged = [("spike".to_string(), 3), ("done".to_string(), 0)].into();
        let table = build(&repo, &view(true, "", None));
        let row = |key: &str| table.rows.iter().find(|r| r.key == key).unwrap().clone();
        assert_eq!((row("bspike").git.as_deref(), row("bspike").note), (Some("↑3"), Some("unmerged")));
        assert_eq!((row("bdone").git, row("bdone").note), (None, Some("merged")));
    }

    #[test]
    fn a_checkout_row_says_how_many_agents_share_it_once_several_do() {
        let mut repo = repo();
        let agent = |id, title| pane(id, PaneKind::Agent, title, PaneStatus::Working);
        repo.checkouts[0].panes = vec![agent(1, "Trim"), agent(2, "Tracker"), pane(3, PaneKind::Shell, "zsh", PaneStatus::Idle)];
        repo.checkouts[1].panes = vec![agent(4, "Desktop")];
        let table = build(&repo, &view(true, "", None));
        let shared: Vec<_> = table.rows.iter().map(|r| (r.key.as_str(), r.shared)).collect();
        assert_eq!(shared, [("c100", Some(2)), ("c101", None), ("bold", None), ("rorigin/transcripts", None)]);
    }
}
