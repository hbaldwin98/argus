//! What a Checkouts row says about itself in words before they are markup: the State cell, in the
//! order it is drawn, and the branch set beside a name. Kept apart from the drawing so the rule that
//! a clean, idle checkout says nothing is held by a test of its own, for every window that draws it.

use crate::checkouts::table::CheckoutsRow;

/// One thing the State cell of a checkout row says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Say {
    Detached,
    /// `3 changed`.
    Changed(String),
    /// `↑2 ↓1`, a side with nothing left out.
    Distance(String),
    /// How many panes run here; the dots are the row's `running`.
    Panes(usize),
    /// `#42`, lit when the Feature stage has it under its cursor.
    Feature { number: i64, viewed: bool },
}

/// The State cell of a checkout row. A checkout that is clean, in step with its upstream and idle
/// says nothing: only what needs a look is said, so a column of quiet rows is a column of good news.
pub fn state_of(row: &CheckoutsRow) -> Vec<Say> {
    let mut says = Vec::new();
    if row.detached {
        says.push(Say::Detached);
    }
    if row.changed > 0 {
        says.push(Say::Changed(format!("{} changed", row.changed)));
    }
    let distance: Vec<String> = [(row.ahead, '↑'), (row.behind, '↓')].into_iter().filter(|(n, _)| *n > 0).map(|(n, arrow)| format!("{arrow}{n}")).collect();
    if !distance.is_empty() {
        says.push(Say::Distance(distance.join(" ")));
    }
    if !row.running.is_empty() {
        says.push(Say::Panes(row.running.len()));
    }
    if let Some(number) = row.feature {
        says.push(Say::Feature { number, viewed: row.viewed });
    }
    says
}

/// A branch row's State cell, in the order it is drawn: the git string (`↑3`), the note, and the
/// feature number, which says why a feature's branch stays listed with the rest folded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchState {
    pub git: Option<String>,
    pub note: Option<&'static str>,
    pub feature: Option<(i64, bool)>,
}

pub fn branch_cell(row: &CheckoutsRow) -> BranchState {
    BranchState { git: row.git.clone(), note: row.note, feature: row.feature.map(|number| (number, row.viewed)) }
}

/// The branch to set beside a checkout's name: only when the folder name does not already say it.
/// A detached head is the State cell's to say.
pub fn branch_beside(row: &CheckoutsRow) -> Option<&str> {
    let name = row.name.as_deref()?;
    (!row.detached && !row.branch.is_empty() && row.branch != name).then_some(row.branch.as_str())
}

/// The words of a checkout's count: `1 pane`, `2 panes`.
pub fn panes(count: usize) -> String {
    format!("{count} pane{}", if count == 1 { "" } else { "s" })
}

/// The State cell as one line of words, for a reader that cannot see its marks: what a checkout row
/// says in the order it says it, or a branch row's git string, note and feature number.
pub fn state_words(row: &CheckoutsRow) -> String {
    let words: Vec<String> = if row.name.is_some() {
        state_of(row)
            .into_iter()
            .map(|say| match say {
                Say::Detached => "detached".to_owned(),
                Say::Changed(text) | Say::Distance(text) => text,
                Say::Panes(count) => panes(count),
                Say::Feature { number, .. } => format!("#{number}"),
            })
            .collect()
    } else {
        let cell = branch_cell(row);
        [cell.git, cell.note.map(str::to_owned), cell.feature.map(|(number, _)| format!("#{number}"))].into_iter().flatten().collect()
    };
    words.join(" ")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::checkouts::table::Running;

    /// A quiet checkout row, for the tests of every module that words one.
    pub(crate) fn row() -> CheckoutsRow {
        CheckoutsRow {
            key: "c1".into(),
            checkout: Some(1),
            name: Some("desktop".into()),
            primary: false,
            branch: "desktop".into(),
            git: None,
            clean: true,
            changed: 0,
            ahead: 0,
            behind: 0,
            detached: false,
            note: None,
            last: None,
            running: Vec::new(),
            shared: None,
            feature: None,
            viewed: false,
        }
    }

    /// A branch row with no directory.
    pub(crate) fn branch_row(branch: &str) -> CheckoutsRow {
        CheckoutsRow { key: format!("b{branch}"), checkout: None, name: None, branch: branch.into(), clean: false, ..row() }
    }

    fn running(n: u64) -> Running {
        Running { id: n, title: format!("pane {n}"), agent: false, status: "idle" }
    }

    #[test]
    fn a_clean_in_step_idle_checkout_says_nothing() {
        assert!(state_of(&row()).is_empty());
    }

    #[test]
    fn the_state_cell_says_what_needs_a_look_in_a_fixed_order() {
        let busy = CheckoutsRow { detached: true, changed: 3, ahead: 2, behind: 1, running: vec![running(1), running(2)], feature: Some(42), viewed: true, ..row() };
        assert_eq!(
            state_of(&busy),
            [Say::Detached, Say::Changed("3 changed".into()), Say::Distance("↑2 ↓1".into()), Say::Panes(2), Say::Feature { number: 42, viewed: true }]
        );
    }

    #[test]
    fn a_side_with_nothing_left_is_left_out_of_the_distance() {
        assert_eq!(state_of(&CheckoutsRow { ahead: 3, ..row() }), [Say::Distance("↑3".into())]);
        assert_eq!(state_of(&CheckoutsRow { behind: 4, ..row() }), [Say::Distance("↓4".into())]);
    }

    #[test]
    fn a_branch_row_says_its_git_string_note_and_feature() {
        let branch = CheckoutsRow { git: Some("↑3".into()), note: Some("unmerged"), feature: Some(7), ..branch_row("spike") };
        assert_eq!(branch_cell(&branch), BranchState { git: Some("↑3".into()), note: Some("unmerged"), feature: Some((7, false)) });
    }

    #[test]
    fn the_branch_shows_beside_the_name_only_when_it_differs_from_the_folder() {
        assert_eq!(branch_beside(&row()), None);
        assert_eq!(branch_beside(&CheckoutsRow { branch: "spike".into(), ..row() }), Some("spike"));
        assert_eq!(branch_beside(&CheckoutsRow { branch: "detached".into(), detached: true, ..row() }), None, "a detached head is the State cell's");
        assert_eq!(branch_beside(&CheckoutsRow { branch: String::new(), ..row() }), None);
        assert_eq!(branch_beside(&branch_row("spike")), None, "a branch row names its branch as its name");
    }

    #[test]
    fn a_pane_count_is_singular_for_one() {
        assert_eq!((panes(1).as_str(), panes(2).as_str()), ("1 pane", "2 panes"));
    }

    #[test]
    fn the_state_is_also_said_as_one_line() {
        let busy = CheckoutsRow { detached: true, changed: 3, ahead: 2, running: vec![running(1)], feature: Some(42), ..row() };
        assert_eq!(state_words(&busy), "detached 3 changed ↑2 1 pane #42");
        assert_eq!(state_words(&row()), "");
        let branch = CheckoutsRow { git: Some("↑3".into()), note: Some("unmerged"), feature: Some(7), ..branch_row("spike") };
        assert_eq!(state_words(&branch), "↑3 unmerged #7");
    }
}
