//! What the Checkouts stage's gestures come to: the items of its strip menu and of a row's menu,
//! and for a button, a menu choice or a key, the calls it sends or the name it asks for first.
//!
//! The page spelled this out in each button's handler, and a window that draws the stage would have
//! spelled it again. A gesture on a row first picks the row if it is not the selected one, since the
//! stage's actions work on the selected row and a click on a row's own button must not wait for a
//! second one.

use crate::checkouts::table::{Checkouts, CheckoutsRow};
use crate::connection::page::Call;
use crate::keys::keymap::Action;

/// Something that can be done to a row, or to the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Way {
    /// Show the checkout's first pane, or switch the primary checkout to the branch.
    Open,
    Review,
    History,
    Pull,
    Files,
    /// Show the checkout's folder in the file manager.
    Reveal,
    NewShell,
    NewAgent(String),
    NewWorktree,
    Delete,
    Branches,
    Fetch,
}

/// One line of a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Rule,
    Choice(Choice),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub way: Way,
    /// The key that does the same, when the table binds one.
    pub key: Option<&'static str>,
    /// Ends something that cannot be got back.
    pub danger: bool,
}

fn choice(label: impl Into<String>, way: Way, key: Option<&'static str>) -> Item {
    Item::Choice(Choice { label: label.into(), way, key, danger: false })
}

/// What the strip's `⋯` offers: the branches no checkout is on, shown or put away, and a fetch of
/// every repository's remote.
pub fn strip_items(table: &Checkouts) -> Vec<Item> {
    vec![
        choice(if table.branches_shown { "Hide other branches" } else { "Show other branches" }, Way::Branches, Some("B")),
        choice("Fetch all", Way::Fetch, Some("F")),
    ]
}

/// What a row's `⋯` offers: for a checkout Review, History, Pull, Files and the folder, then a new
/// shell and an agent per template; for a branch a new worktree; and for a row that is not primary
/// the removal, in danger.
pub fn row_items(row: &CheckoutsRow, table: &Checkouts, templates: &[String]) -> Vec<Item> {
    let mut items = Vec::new();
    if row.checkout.is_some() {
        items.push(choice("Review", Way::Review, None));
        items.push(choice("History", Way::History, None));
        items.push(choice("Pull", Way::Pull, None));
        if table.files_offered {
            items.push(choice("Files", Way::Files, Some("E")));
        }
        if let Some(reveal) = table.reveal {
            items.push(choice(reveal, Way::Reveal, None));
        }
        items.push(Item::Rule);
        items.push(choice("New shell", Way::NewShell, None));
        items.extend(templates.iter().map(|template| choice(format!("New agent · {template}"), Way::NewAgent(template.clone()), None)));
    } else {
        items.push(choice("New worktree", Way::NewWorktree, None));
    }
    if !row.primary {
        items.push(Item::Rule);
        items.push(Item::Choice(Choice { label: removal(row).into(), way: Way::Delete, key: None, danger: true }));
    }
    items
}

/// What removing a row is called: a worktree is removed, a branch deleted.
pub fn removal(row: &CheckoutsRow) -> &'static str {
    if row.checkout.is_some() {
        "Remove"
    } else {
        "Delete"
    }
}

/// What a gesture comes to.
#[derive(Debug, Default)]
pub struct Then {
    /// The calls to send, in order.
    pub calls: Vec<Call>,
    /// A new worktree off this branch is to be named first; the window asks, and a `NewWorktree`
    /// with the name is what sends it.
    pub name_off: Option<String>,
}

impl Then {
    fn send(calls: Vec<Call>) -> Self {
        Self { calls, name_off: None }
    }
}

/// What `way` comes to on `row`, the table having `selected` selected. An action that works on the
/// selected row picks `row` first when it is not.
pub fn then(way: &Way, row: &CheckoutsRow, selected: Option<&str>) -> Then {
    let pick = || (selected != Some(row.key.as_str())).then(|| Call::SelectCheckoutRow { key: row.key.clone() });
    let act = |action| Then::send(pick().into_iter().chain([Call::CheckoutsAction { action }]).collect());
    match (way, row.checkout) {
        (Way::Branches | Way::Fetch, _) => table(way),
        (Way::Open, _) => act(Action::Choose),
        (Way::Pull, _) => act(Action::Pull),
        (Way::Files, _) => act(Action::Files),
        (Way::Delete, _) => act(Action::Delete),
        (Way::NewWorktree, _) => {
            let mut then = new_worktree(Some(row));
            then.calls.splice(0..0, pick());
            then
        }
        (Way::Review, Some(checkout)) => Then::send(vec![Call::OpenReview { checkout }]),
        (Way::History, Some(checkout)) => Then::send(vec![Call::OpenHistory { checkout }]),
        (Way::Reveal, Some(checkout)) => Then::send(vec![Call::RevealCheckout { checkout }]),
        (Way::NewShell, Some(checkout)) => Then::send(vec![Call::SpawnShell { checkout }]),
        (Way::NewAgent(template), Some(checkout)) => Then::send(vec![Call::SpawnAgent { checkout, template: template.clone() }]),
        // A branch has no changes of its own, no history, no folder and no panes.
        _ => Then::default(),
    }
}

/// A way of the table itself, which needs no row: the strip's `⋯`.
pub fn table(way: &Way) -> Then {
    match way {
        Way::Branches => Then::send(vec![Call::CheckoutsAction { action: Action::Branches }]),
        Way::Fetch => Then::send(vec![Call::CheckoutsAction { action: Action::Fetch }]),
        _ => Then::default(),
    }
}

/// A new worktree for `row`: a checkout row asks for a branch name first, off the branch it is on
/// (or its folder's name when that says nothing); a branch row sends at once, since the branch
/// brings its own name, and so does a table with no row selected.
pub fn new_worktree(row: Option<&CheckoutsRow>) -> Then {
    match row {
        Some(row) if row.checkout.is_some() => {
            let off = if row.branch.is_empty() { row.name.clone().unwrap_or_default() } else { row.branch.clone() };
            Then { calls: Vec::new(), name_off: Some(off) }
        }
        _ => Then::send(vec![Call::NewWorktree { name: None }]),
    }
}

/// What a key of the table's place comes to.
#[derive(Debug)]
pub enum Key {
    /// `/`: the filter field takes the keys. Nothing is sent.
    Filter,
    Then(Then),
}

/// The action a key of the `Checkouts` place is, as the stage runs it: the rest of the table's keys
/// go to the session as they are, and it moves the selection or acts on it. `None` for an action
/// that is not the table's.
pub fn key(action: Action, selected: Option<&CheckoutsRow>) -> Option<Key> {
    Some(match action {
        Action::Filter => Key::Filter,
        Action::NewWorktree => Key::Then(new_worktree(selected)),
        Action::Next
        | Action::Previous
        | Action::First
        | Action::Last
        | Action::Choose
        | Action::Branches
        | Action::Fetch
        | Action::Pull
        | Action::Delete
        | Action::OpenReview
        | Action::OpenHistory
        | Action::StartShell
        | Action::StartAgent
        | Action::Files => Key::Then(Then::send(vec![Call::CheckoutsAction { action }])),
        _ => return None,
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkouts::says::tests::{branch_row, row};

    fn sent(then: &Then) -> Vec<String> {
        then.calls.iter().map(|call| format!("{call:?}")).collect()
    }

    fn labels(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                Item::Rule => "---".to_owned(),
                Item::Choice(c) => format!("{}{}", if c.danger { "!" } else { "" }, c.label),
            })
            .collect()
    }

    fn table(files: bool, reveal: Option<&'static str>) -> Checkouts {
        Checkouts { files_offered: files, reveal, ..Checkouts::default() }
    }

    /// CHECKOUTS-14: the strip's menu names what B would do.
    #[test]
    fn the_strip_menu_offers_the_branches_as_they_would_toggle_and_a_fetch() {
        let hidden = labels(&strip_items(&table(false, None)));
        assert_eq!(hidden, ["Show other branches", "Fetch all"]);
        let shown = Checkouts { branches_shown: true, ..Checkouts::default() };
        assert_eq!(labels(&strip_items(&shown))[0], "Hide other branches");
    }

    /// CHECKOUTS-26.
    #[test]
    fn a_checkout_rows_menu_lists_its_ways_then_a_shell_an_agent_per_template_and_the_removal() {
        let templates = ["claude".to_owned(), "codex".to_owned()];
        let items = row_items(&row(), &table(true, Some("Show in Explorer")), &templates);
        assert_eq!(
            labels(&items),
            ["Review", "History", "Pull", "Files", "Show in Explorer", "---", "New shell", "New agent · claude", "New agent · codex", "---", "!Remove"]
        );
        let bare = row_items(&row(), &table(false, None), &[]);
        assert_eq!(labels(&bare), ["Review", "History", "Pull", "---", "New shell", "---", "!Remove"], "Files and the folder only when offered");
    }

    #[test]
    fn the_primary_checkout_has_no_removal_and_a_branch_row_offers_a_worktree_and_a_deletion() {
        let primary = CheckoutsRow { primary: true, ..row() };
        assert!(!labels(&row_items(&primary, &table(false, None), &[])).iter().any(|l| l.contains("Remove")));
        let branch = labels(&row_items(&branch_row("old"), &table(true, Some("Reveal")), &["claude".into()]));
        assert_eq!(branch, ["New worktree", "---", "!Delete"], "a branch has no panes, files or review");
    }

    #[test]
    fn a_way_on_an_unselected_row_picks_it_first_and_on_the_selected_one_does_not() {
        let desktop = row();
        assert_eq!(sent(&then(&Way::Pull, &desktop, Some("c9"))), ["SelectCheckoutRow { key: \"c1\" }", "CheckoutsAction { action: Pull }"]);
        assert_eq!(sent(&then(&Way::Pull, &desktop, Some("c1"))), ["CheckoutsAction { action: Pull }"]);
        assert_eq!(sent(&then(&Way::Open, &desktop, None)), ["SelectCheckoutRow { key: \"c1\" }", "CheckoutsAction { action: Choose }"]);
        assert_eq!(sent(&then(&Way::Delete, &desktop, Some("c1"))), ["CheckoutsAction { action: Delete }"]);
    }

    #[test]
    fn review_history_and_the_new_panes_name_the_checkout_and_pick_no_row() {
        let desktop = row();
        assert_eq!(sent(&then(&Way::Review, &desktop, Some("c9"))), ["OpenReview { checkout: 1 }"]);
        assert_eq!(sent(&then(&Way::History, &desktop, Some("c9"))), ["OpenHistory { checkout: 1 }"]);
        assert_eq!(sent(&then(&Way::Reveal, &desktop, Some("c9"))), ["RevealCheckout { checkout: 1 }"]);
        assert_eq!(sent(&then(&Way::NewShell, &desktop, None)), ["SpawnShell { checkout: 1 }"]);
        assert_eq!(sent(&then(&Way::NewAgent("claude".into()), &desktop, None)), ["SpawnAgent { checkout: 1, template: \"claude\" }"]);
        assert!(then(&Way::Review, &branch_row("old"), None).calls.is_empty(), "a branch has no changes of its own");
    }

    /// CHECKOUTS-39 and 49.
    #[test]
    fn a_new_worktree_off_a_checkout_asks_for_a_name_and_off_a_branch_or_nothing_sends_at_once() {
        let spike = CheckoutsRow { branch: "spike".into(), ..row() };
        let asked = new_worktree(Some(&spike));
        assert_eq!((asked.name_off.as_deref(), asked.calls.len()), (Some("spike"), 0));
        let unnamed = CheckoutsRow { branch: String::new(), ..row() };
        assert_eq!(new_worktree(Some(&unnamed)).name_off.as_deref(), Some("desktop"), "the folder's name when the branch says nothing");
        for none in [new_worktree(Some(&branch_row("old"))), new_worktree(None)] {
            assert_eq!((sent(&none), none.name_off), (vec!["NewWorktree { name: None }".to_owned()], None));
        }
    }

    #[test]
    fn a_new_worktree_from_a_menu_picks_the_row_before_it_asks() {
        let then = then(&Way::NewWorktree, &row(), Some("c9"));
        assert_eq!((sent(&then), then.name_off.as_deref()), (vec!["SelectCheckoutRow { key: \"c1\" }".to_owned()], Some("desktop")));
    }

    /// CHECKOUTS-28, 30: every key the table binds is a way the stage runs, and `/` is the window's.
    #[test]
    fn every_key_the_table_binds_is_run_and_the_slash_is_the_filter_fields() {
        use crate::keys::keymap::{Place, BINDINGS};
        for binding in BINDINGS.iter().filter(|b| b.place == Place::Checkouts) {
            assert!(key(binding.action, Some(&row())).is_some(), "{:?} is bound and never run", binding.action);
        }
        assert!(matches!(key(Action::Filter, None), Some(Key::Filter)));
        assert!(key(Action::CloseLayer, None).is_none(), "Escape on the table is the stage ladder's");
        let Some(Key::Then(next)) = key(Action::Next, None) else { panic!("j moves") };
        assert_eq!(sent(&next), ["CheckoutsAction { action: Next }"]);
    }

    #[test]
    fn n_asks_for_a_name_on_a_checkout_row_only() {
        let Some(Key::Then(on_checkout)) = key(Action::NewWorktree, Some(&row())) else { panic!("n is the table's") };
        assert!(on_checkout.name_off.is_some());
        let Some(Key::Then(on_branch)) = key(Action::NewWorktree, Some(&branch_row("old"))) else { panic!("n is the table's") };
        assert_eq!(sent(&on_branch), ["NewWorktree { name: None }"]);
    }

    #[test]
    fn the_strips_ways_need_no_row_and_pick_none() {
        assert_eq!(sent(&super::table(&Way::Branches)), ["CheckoutsAction { action: Branches }"]);
        assert_eq!(sent(&super::table(&Way::Fetch)), ["CheckoutsAction { action: Fetch }"]);
        assert!(super::table(&Way::Pull).calls.is_empty(), "a way on a row is no way of the table");
        assert_eq!(sent(&then(&Way::Branches, &row(), Some("c9"))), ["CheckoutsAction { action: Branches }"], "a fold row's click picks nothing");
    }
}
