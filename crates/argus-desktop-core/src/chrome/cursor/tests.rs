use super::*;
use crate::connection::page::{Checkout, Pane, Project, Repository};

fn pane(id: u64) -> Pane {
    Pane { id, agent: true, title: format!("pane {id}"), status: "idle", note: None, on: false, views: 0 }
}

fn checkout(id: u64, open: bool, panes: &[u64]) -> Checkout {
    Checkout {
        id,
        name: format!("co{id}"),
        branch: "main".into(),
        kind: "main",
        git: None,
        clean: true,
        shared: None,
        open,
        count: panes.len(),
        status: None,
        panes: if open { panes.iter().map(|&p| pane(p)).collect() } else { Vec::new() },
    }
}

/// Two repositories: checkouts 10 (panes 1, 2) and 11 (folded, pane 3) in the first, 20 (pane 4) in the
/// second, which is shut so lists nothing.
fn rail() -> Rail {
    let repo = |id, open, checkouts| Repository {
        id,
        name: format!("repo{id}"),
        quiet: false,
        divider: false,
        open,
        panes: 0,
        status: None,
        used: None,
        checkouts,
    };
    Rail {
        project: Some(Project {
            id: 1,
            name: "p".into(),
            needs_you: 0,
            repositories: vec![repo(1, true, vec![checkout(10, true, &[1, 2]), checkout(11, false, &[3])]), repo(2, true, vec![checkout(20, true, &[4])])],
            running: Vec::new(),
        }),
        templates: vec!["claude".into(), "codex".into()],
        ..Rail::default()
    }
}

fn picked(cursor: &RailCursor) -> (u64, Option<u64>) {
    let pick = cursor.picked().expect("a row is picked");
    (pick.checkout, pick.pane)
}

#[test]
fn j_and_k_step_through_every_checkout_and_its_panes_in_the_rails_order_and_stop_at_the_ends() {
    // SHELL-RAIL-20.
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.pick(10, None);
    let mut seen = Vec::new();
    for _ in 0..8 {
        cursor.key(Action::Next, &rail);
        seen.push(picked(&cursor));
    }
    assert_eq!(
        seen,
        [(10, Some(1)), (10, Some(2)), (11, None), (20, None), (20, Some(4)), (20, Some(4)), (20, Some(4)), (20, Some(4))],
        "a folded checkout lists none of its panes, and the end holds"
    );
    for _ in 0..9 {
        cursor.key(Action::Previous, &rail);
    }
    assert_eq!(picked(&cursor), (10, None), "the start holds");
}

#[test]
fn stepping_onto_a_pane_shows_it_and_onto_a_checkout_shows_nothing() {
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.pick(10, None);
    assert_eq!(cursor.key(Action::Next, &rail), Then::Show(1));
    cursor.pick(10, Some(2));
    assert_eq!(cursor.key(Action::Next, &rail), Then::Nothing);
    assert_eq!(picked(&cursor), (11, None));
}

#[test]
fn a_pick_on_no_row_steps_to_the_first_row() {
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.pick(999, None);
    cursor.key(Action::Next, &rail);
    assert_eq!(picked(&cursor), (10, None));
    cursor.pick(999, None);
    cursor.key(Action::Previous, &rail);
    assert_eq!(picked(&cursor), (10, None));
}

#[test]
fn a_rail_with_no_rows_steps_nowhere() {
    let mut cursor = RailCursor::default();
    cursor.pick(10, None);
    assert_eq!(cursor.key(Action::Next, &Rail::default()), Then::Nothing);
    assert_eq!(picked(&cursor), (10, None));
}

#[test]
fn nothing_picked_answers_no_key() {
    let rail = rail();
    let mut cursor = RailCursor::default();
    for action in [Action::Next, Action::Close, Action::Choose, Action::StartShell, Action::Fold, Action::OpenReview] {
        assert_eq!(cursor.key(action, &rail), Then::Nothing);
    }
    assert!(!cursor.start_open(), "Enter with nothing picked opens no menu");
}

#[test]
fn x_closes_a_picked_pane_and_the_pick_falls_back_to_its_checkout() {
    // SHELL-RAIL-23.
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.pick(10, Some(2));
    assert_eq!(cursor.key(Action::Close, &rail), Then::Close(2));
    assert_eq!(picked(&cursor), (10, None));
}

#[test]
fn x_on_a_picked_checkout_asks_to_remove_it_and_keeps_the_pick() {
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.pick(10, None);
    assert_eq!(cursor.key(Action::Close, &rail), Then::Remove(10));
    assert_eq!(picked(&cursor), (10, None));
}

#[test]
fn the_keys_of_a_picked_row_name_what_the_session_does() {
    // SHELL-RAIL-21, 22, 25, 26.
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.pick(10, Some(1));
    assert_eq!(cursor.key(Action::OpenReview, &rail), Then::Review(10));
    assert_eq!(cursor.key(Action::OpenHistory, &rail), Then::History(10));
    assert_eq!(cursor.key(Action::Fold, &rail), Then::Fold(10));
    assert_eq!(picked(&cursor), (10, Some(1)), "the fold takes the pick off a pane only when the session folds");
}

#[test]
fn s_and_a_start_a_shell_and_the_default_agent_and_either_lets_the_pick_go() {
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.pick(10, Some(1));
    assert_eq!(cursor.key(Action::StartShell, &rail), Then::Start { checkout: 10, agent: None });
    assert_eq!(cursor.picked(), None, "the new pane takes the keys");
    cursor.pick(20, None);
    assert_eq!(cursor.key(Action::StartAgent, &rail), Then::Start { checkout: 20, agent: Some("claude".into()) });
    assert_eq!(cursor.picked(), None);
}

#[test]
fn an_agent_asked_for_with_none_offered_says_so_and_keeps_the_pick() {
    let mut rail = rail();
    rail.templates.clear();
    let mut cursor = RailCursor::default();
    cursor.pick(10, None);
    assert_eq!(cursor.key(Action::StartAgent, &rail), Then::NoAgent);
    assert_eq!(picked(&cursor), (10, None));
}

#[test]
fn enter_on_a_checkout_toggles_its_menu_and_on_a_pane_lets_the_pick_go() {
    // SHELL-RAIL-24.
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.pick(10, None);
    cursor.key(Action::Choose, &rail);
    assert_eq!(cursor.page().start, Some(StartMenu { checkout: 10, from: StartFrom::Rail, at: 0 }));
    cursor.key(Action::Choose, &rail);
    assert!(!cursor.start_open(), "a second Enter shuts the same menu");
    cursor.pick(10, Some(1));
    cursor.key(Action::Choose, &rail);
    assert_eq!(cursor.picked(), None);
    assert!(!cursor.start_open());
}

#[test]
fn escape_lets_the_pick_go() {
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.pick(10, None);
    cursor.key(Action::CloseLayer, &rail);
    assert_eq!(cursor.picked(), None);
}

#[test]
fn a_menu_opened_from_another_place_is_not_the_one_toggled_here() {
    let mut cursor = RailCursor::default();
    cursor.toggle_start(10, StartFrom::Table);
    cursor.toggle_start(10, StartFrom::Rail);
    assert_eq!(cursor.page().start, Some(StartMenu { checkout: 10, from: StartFrom::Rail, at: 0 }), "it moved, it did not close");
}

#[test]
fn the_menus_highlight_wraps_both_ways_and_enter_starts_the_highlighted_item() {
    // SHELL-RAIL-30.
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.toggle_start(10, StartFrom::Rail);
    let at = |cursor: &RailCursor| cursor.page().start.expect("open").at;
    cursor.start_key(Action::Previous, &rail);
    assert_eq!(at(&cursor), 2, "up from the first wraps to the last");
    cursor.start_key(Action::Next, &rail);
    assert_eq!(at(&cursor), 0, "down from the last wraps to the first");
    cursor.start_key(Action::Next, &rail);
    assert_eq!(at(&cursor), 1);
    assert_eq!(cursor.start_key(Action::Choose, &rail), Then::Start { checkout: 10, agent: Some("claude".into()) });
}

#[test]
fn s_and_a_in_the_menu_start_without_moving_the_highlight_and_shut_it() {
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.pick(10, None);
    cursor.toggle_start(10, StartFrom::Rail);
    assert_eq!(cursor.start_key(Action::StartAgent, &rail), Then::Start { checkout: 10, agent: Some("claude".into()) });
    assert!(!cursor.start_open());
    assert_eq!(cursor.picked(), None, "starting lets the pick go too");
    cursor.toggle_start(10, StartFrom::Rail);
    assert_eq!(cursor.start_key(Action::StartShell, &rail), Then::Start { checkout: 10, agent: None });
}

#[test]
fn escape_shuts_the_menu_and_any_other_action_is_not_its() {
    let rail = rail();
    let mut cursor = RailCursor::default();
    cursor.toggle_start(10, StartFrom::Rail);
    assert_eq!(cursor.start_key(Action::Fold, &rail), Then::Nothing);
    assert!(cursor.start_open());
    cursor.start_key(Action::CloseLayer, &rail);
    assert!(!cursor.start_open());
}

#[test]
fn the_menu_offers_a_shell_then_an_agent_per_template_with_the_first_the_default() {
    // SHELL-RAIL-29.
    let items = starts(&["claude".to_owned(), "codex".to_owned()]);
    let labels: Vec<_> = items.iter().map(|item| (item.label.as_str(), item.agent.as_deref())).collect();
    assert_eq!(labels, [("Shell", None), ("Agent \u{b7} claude", Some("claude")), ("Agent \u{b7} codex", Some("codex"))]);
    assert_eq!(starts(&[]).len(), 1, "a daemon with no template still offers a shell");
}

#[test]
fn a_pane_gone_from_the_tree_moves_the_pick_to_its_checkout_and_nothing_else_does() {
    // SHELL-RAIL-19.
    let mut cursor = RailCursor::default();
    cursor.pick(10, Some(2));
    assert!(!cursor.settle(&rail()), "a pane still listed keeps it");
    let mut closed = rail();
    closed.project.as_mut().unwrap().repositories[0].checkouts[0].panes.retain(|p| p.id != 2);
    assert!(cursor.settle(&closed));
    assert_eq!(picked(&cursor), (10, None));
    assert!(!cursor.settle(&closed), "a checkout keeps the pick");
    cursor.pick(999, None);
    assert!(!cursor.settle(&closed), "a checkout that is gone keeps it too, marking no row");
    assert_eq!(picked(&cursor), (999, None));
}

#[test]
fn a_pane_under_a_folded_checkout_or_shut_repository_is_not_listed() {
    let mut cursor = RailCursor::default();
    cursor.pick(11, Some(3));
    assert!(cursor.settle(&rail()), "checkout 11 is folded");
    let mut shut = rail();
    shut.project.as_mut().unwrap().repositories[1].open = false;
    cursor.pick(20, Some(4));
    assert!(cursor.settle(&shut));
    assert_eq!(picked(&cursor), (20, None));
}

#[test]
fn folding_a_checkout_gives_a_pane_picked_under_it_the_checkouts_pick() {
    // SHELL-RAIL-8.
    let mut cursor = RailCursor::default();
    cursor.pick(10, Some(1));
    cursor.folding(20);
    assert_eq!(picked(&cursor), (10, Some(1)), "another checkout's fold is not its business");
    cursor.folding(10);
    assert_eq!(picked(&cursor), (10, None));
}

#[test]
fn the_cursor_answers_only_the_actions_the_rail_and_the_menu_bind() {
    for action in [Action::Next, Action::Previous, Action::Close, Action::Choose, Action::Fold, Action::CloseLayer] {
        assert!(answers(action), "{action:?}");
    }
    assert!(!answers(Action::Checkouts), "a stage's key is the stage's");
    assert!(answers_start(Action::Choose));
    assert!(!answers_start(Action::Close), "x is not the menu's");
    assert!(!answers_start(Action::Fold));
}
