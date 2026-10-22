//! What the column beside the table says of the selected row: where it is, what it tracks, what
//! runs on it and what can be done to it, as data a window draws. It follows the selection, and
//! every button is a key the table answers to as well.

use crate::checkouts::table::{Checkouts, CheckoutsRow, LastCommit};
use crate::checkouts::ways::{removal, Way};

/// A button of the column: its words, the key that does the same, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Act {
    pub label: &'static str,
    pub key: Option<&'static str>,
    pub way: Way,
}

/// What the column's foot offers a checkout row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foot {
    /// Files, when the daemon lists a checkout's folders.
    pub files: bool,
    /// What the button that shows the folder in the file manager says; `None` off this machine.
    pub reveal: Option<&'static str>,
    /// The checkout whose start menu the `New pane` button opens.
    pub new_pane: Option<u64>,
    /// `Remove` or `Delete`, in danger, for a row that is not primary.
    pub remove: Option<&'static str>,
}

/// One line of the facts list, a label and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub label: &'static str,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    /// `#42` and the feature's title; lit while the Feature stage has it under its cursor.
    Feature { number: i64, viewed: bool, title: Option<String> },
    Commit(LastCommit),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Side {
    /// The row's name, in the mono face for a branch.
    pub title: String,
    pub mono: bool,
    pub primary: bool,
    /// The worktree's path shortened by [`repo_relative`], and the whole of it for a tooltip.
    pub path: Option<(String, String)>,
    pub note: Option<&'static str>,
    pub acts: Vec<Act>,
    pub facts: Vec<Fact>,
    /// `Running here`, or `Running here · shared by 2` once several agents share the checkout.
    pub running: Option<String>,
    pub foot: Foot,
}

/// A worktree's path from the repository's own `.argus/` folder when it lives under one, whole
/// otherwise. Backslashes become slashes only when it is cut, so the shown text uses forward ones.
pub fn repo_relative(path: &str) -> String {
    let flat = path.replace('\\', "/");
    match flat.find("/.argus/") {
        Some(at) => flat[at + 1..].to_owned(),
        None => path.to_owned(),
    }
}

/// The column for `row`, the selected one.
pub fn side(table: &Checkouts, row: &CheckoutsRow) -> Side {
    let checkout = row.checkout;
    let act = |label, key, way| Act { label, key, way };
    let acts = match checkout {
        Some(_) => vec![act("Open", None, Way::Open), act("Review", Some("R"), Way::Review), act("History", Some("H"), Way::History), act("Pull", Some("P"), Way::Pull)],
        None => vec![act("Check out", Some("N"), Way::NewWorktree)],
    };
    let mut facts = Vec::new();
    if let Some(detail) = &table.detail {
        let text = |label, value: &Option<String>| value.clone().map(|value| Fact { label, value: Value::Text(value) });
        facts.extend(text("Upstream", &detail.upstream));
        facts.extend(text("Changes", &detail.changes));
        if let Some(number) = row.feature {
            facts.push(Fact { label: "Feature", value: Value::Feature { number, viewed: row.viewed, title: detail.feature_title.clone() } });
        }
        if let Some(last) = &row.last {
            facts.push(Fact { label: "Last commit", value: Value::Commit(last.clone()) });
        }
    }
    Side {
        title: row.name.clone().unwrap_or_else(|| row.branch.clone()),
        mono: row.name.is_none(),
        primary: row.primary,
        path: table.detail.as_ref().map(|detail| (repo_relative(&detail.path), detail.path.clone())),
        note: row.note,
        acts,
        facts,
        running: (!row.running.is_empty()).then(|| match row.shared {
            Some(n) => format!("Running here · shared by {n}"),
            None => "Running here".to_owned(),
        }),
        foot: Foot {
            files: checkout.is_some() && table.files_offered,
            reveal: table.reveal.filter(|_| checkout.is_some()),
            new_pane: checkout,
            remove: (!row.primary).then(|| removal(row)),
        },
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkouts::says::tests::{branch_row, row};
    use crate::checkouts::table::{CheckoutDetail, Running};

    fn detail() -> CheckoutDetail {
        CheckoutDetail {
            name: "desktop".into(),
            path: "C:\\src\\orion\\.argus\\worktrees\\desktop".into(),
            upstream: Some("origin/desktop · 3 ahead".into()),
            changes: Some("2 staged · 4 unstaged files".into()),
            feature_title: Some("Screenshot harness".into()),
        }
    }

    fn table(files: bool, reveal: Option<&'static str>) -> Checkouts {
        Checkouts { detail: Some(detail()), files_offered: files, reveal, ..Checkouts::default() }
    }

    /// CHECKOUTS-33.
    #[test]
    fn a_path_is_cut_at_the_first_argus_folder_wherever_it_is_and_shown_with_forward_slashes() {
        assert_eq!(repo_relative("/home/me/orion/.argus/worktrees/desktop"), ".argus/worktrees/desktop");
        assert_eq!(repo_relative("C:\\src\\orion\\.argus\\worktrees\\desktop"), ".argus/worktrees/desktop");
        assert_eq!(repo_relative("/home/me/other/.argus/x/.argus/y"), ".argus/x/.argus/y", "the first, not the last");
        assert_eq!(repo_relative("C:\\src\\orion"), "C:\\src\\orion", "whole, backslashes kept, when it is under none");
    }

    /// CHECKOUTS-33..35.
    #[test]
    fn a_checkout_is_said_by_its_path_buttons_and_facts() {
        let selected = CheckoutsRow { note: None, feature: Some(42), viewed: true, last: Some(LastCommit { short: "9d41c07".into(), summary: "Draw flows".into(), ago: "40m".into() }), ..row() };
        let side = side(&table(true, Some("Show in Explorer")), &selected);
        assert_eq!((side.title.as_str(), side.mono, side.primary), ("desktop", false, false));
        assert_eq!(side.path, Some((".argus/worktrees/desktop".to_owned(), "C:\\src\\orion\\.argus\\worktrees\\desktop".to_owned())));
        assert_eq!(side.acts.iter().map(|a| (a.label, a.key)).collect::<Vec<_>>(), [("Open", None), ("Review", Some("R")), ("History", Some("H")), ("Pull", Some("P"))]);
        let labels: Vec<_> = side.facts.iter().map(|f| f.label).collect();
        assert_eq!(labels, ["Upstream", "Changes", "Feature", "Last commit"]);
        assert!(matches!(&side.facts[2].value, Value::Feature { number: 42, viewed: true, title: Some(t) } if t == "Screenshot harness"));
    }

    #[test]
    fn a_branch_is_said_in_mono_with_one_button_and_no_facts() {
        let side = side(&Checkouts::default(), &CheckoutsRow { note: Some("unmerged"), ..branch_row("spike") });
        assert_eq!((side.title.as_str(), side.mono, side.note), ("spike", true, Some("unmerged")));
        assert_eq!(side.acts.iter().map(|a| (a.label, a.key)).collect::<Vec<_>>(), [("Check out", Some("N"))]);
        assert!(side.facts.is_empty() && side.path.is_none());
        assert_eq!((side.foot.files, side.foot.reveal, side.foot.new_pane), (false, None, None));
        assert_eq!(side.foot.remove, Some("Delete"));
    }

    /// CHECKOUTS-36.
    #[test]
    fn running_here_says_how_many_agents_share_the_checkout_once_several_do() {
        let running = vec![Running { id: 1, title: "Trim".into(), agent: true, status: "working" }];
        let alone = side(&table(false, None), &CheckoutsRow { running: running.clone(), ..row() });
        assert_eq!(alone.running.as_deref(), Some("Running here"));
        let shared = side(&table(false, None), &CheckoutsRow { running, shared: Some(2), ..row() });
        assert_eq!(shared.running.as_deref(), Some("Running here · shared by 2"));
        assert_eq!(side(&table(false, None), &row()).running, None);
    }

    /// CHECKOUTS-37.
    #[test]
    fn the_foot_offers_files_and_the_folder_when_the_daemon_does_and_a_removal_unless_primary() {
        let foot = side(&table(true, Some("Reveal")), &row()).foot;
        assert_eq!(foot, Foot { files: true, reveal: Some("Reveal"), new_pane: Some(1), remove: Some("Remove") });
        let plain = side(&table(false, None), &CheckoutsRow { primary: true, ..row() }).foot;
        assert_eq!(plain, Foot { files: false, reveal: None, new_pane: Some(1), remove: None });
    }
}
