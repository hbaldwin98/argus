//! The fuzzy pickers: branches, files, changes, and agents.

use super::*;
#[test]
fn b_asks_for_the_branches_and_opens_only_when_they_arrive() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('l'));
    h.key(KeyCode::Char('b'));
    assert!(
        h.app.picker.is_none(),
        "no picker until the list is in hand"
    );
    assert!(matches!(h.sent()[0], ClientMsg::ListBranches { .. }));
}

#[test]
fn the_branch_you_are_on_is_a_label_not_a_row_to_switch_to() {
    // Switching to the branch you are already on does nothing, so
    // offering it is a row that can only waste a keystroke.
    let mut h = Harness::new();
    branches_arrive(&mut h, &["master", "feature/login", "hotfix"]);

    let p = h.app.picker.as_ref().unwrap();
    assert_eq!(p.items, vec!["feature/login", "hotfix"]);
    assert!(h.app.status.contains("on master"), "{}", h.app.status);
}

#[test]
fn typing_filters_the_branches() {
    let mut h = Harness::new();
    branches_arrive(&mut h, &["master", "feature/login", "hotfix"]);
    h.keys("log");
    assert_eq!(
        h.app.picker.as_ref().unwrap().selected(),
        Some("feature/login")
    );
}

#[test]
fn enter_switches_to_the_branch_under_the_cursor() {
    let mut h = Harness::new();
    branches_arrive(&mut h, &["master", "feature/login", "hotfix"]);
    h.keys("hot");
    h.key(KeyCode::Enter);

    match &h.sent()[0] {
        ClientMsg::SwitchBranch { branch, .. } => assert_eq!(branch, "hotfix"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_query_naming_no_branch_offers_to_create_it() {
    // Making a branch and switching to one are the same intent, so they
    // are the same gesture — but the row is explicit, never implied.
    let mut h = Harness::new();
    branches_arrive(&mut h, &["master", "hotfix"]);
    h.keys("wip");

    let p = h.app.picker.as_ref().unwrap();
    assert_eq!(p.create.as_deref(), Some("wip"));
    assert!(p.shown.is_empty(), "nothing existing matches");
}

#[test]
fn a_query_that_names_an_existing_branch_does_not_offer_to_create_it() {
    let mut h = Harness::new();
    branches_arrive(&mut h, &["master", "hotfix"]);
    h.keys("hotfix");
    assert_eq!(h.app.picker.as_ref().unwrap().create, None);
}

#[test]
fn choosing_the_create_row_makes_the_branch_here() {
    let mut h = Harness::new();
    branches_arrive(&mut h, &["master", "hotfix"]);
    h.keys("wip");
    h.key(KeyCode::Enter);

    match &h.sent()[0] {
        ClientMsg::CreateBranch { branch, .. } => assert_eq!(branch, "wip"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn the_create_row_sits_below_the_matches_rather_than_replacing_them() {
    let mut h = Harness::new();
    branches_arrive(&mut h, &["master", "hotfix", "hotfix-2"]);
    h.keys("hotfix-");

    let p = h.app.picker.as_ref().unwrap();
    assert_eq!(p.selected(), Some("hotfix-2"), "the match is still first");
    assert_eq!(p.create.as_deref(), Some("hotfix-"));

    // Enter on the top row switches; you have to move down to create.
    h.key(KeyCode::Down);
    h.key(KeyCode::Enter);
    assert!(matches!(h.sent()[0], ClientMsg::CreateBranch { .. }));
}

#[test]
fn backspace_widens_the_query_again() {
    let mut h = Harness::new();
    branches_arrive(&mut h, &["master", "feature/login", "hotfix"]);
    h.keys("log");
    assert_eq!(h.app.picker.as_ref().unwrap().shown.len(), 1);

    for _ in 0..3 {
        h.key(KeyCode::Backspace);
    }
    assert_eq!(h.app.picker.as_ref().unwrap().shown.len(), 2);
}

#[test]
fn j_and_k_are_text_in_a_fuzzy_picker() {
    // A branch with a j or a k in its name has to be typeable.
    let mut h = Harness::new();
    branches_arrive(&mut h, &["master", "jkl", "other"]);
    h.keys("jk");
    assert_eq!(h.app.picker.as_ref().unwrap().query, "jk");
    assert_eq!(h.app.picker.as_ref().unwrap().selected(), Some("jkl"));
}

#[test]
fn j_and_k_still_move_in_the_short_pickers() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('t'));
    h.key(KeyCode::Char('j'));
    assert_eq!(h.app.picker.as_ref().unwrap().sel, 1);
}

#[test]
fn esc_closes_a_fuzzy_picker_without_switching_anything() {
    let mut h = Harness::new();
    branches_arrive(&mut h, &["master", "hotfix"]);
    h.keys("hot");
    h.key(KeyCode::Esc);
    assert!(h.app.picker.is_none());
    assert!(h.sent().is_empty());
}

#[test]
fn a_stale_branch_list_is_dropped() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('l'));
    h.key(KeyCode::Char('b'));
    h.sent();
    h.app.on_server_msg(ServerMsg::Branches {
        checkout: CheckoutId(9999),
        branches: vec!["whatever".to_string()],
    });
    assert!(h.app.picker.is_none());
}

#[test]
fn f_opens_the_chosen_file_in_the_editor() {
    let mut h = Harness::new();
    files_arrive(&mut h, &["src/app.rs", "src/ui.rs", "README.md"]);
    h.keys("ui");
    h.key(KeyCode::Enter);

    match &h.sent()[0] {
        ClientMsg::OpenInEditor { path, line, .. } => {
            assert_eq!(path, "src/ui.rs");
            assert_eq!(*line, None, "no particular line was asked for");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_checkout_with_no_files_says_so_rather_than_opening_an_empty_picker() {
    let mut h = Harness::new();
    files_arrive(&mut h, &[]);
    assert!(h.app.picker.is_none());
    assert!(h.app.status.contains("no files"), "{}", h.app.status);
}

#[test]
fn the_file_picker_never_offers_to_create_anything() {
    // That row belongs to branches; a typo here should find nothing,
    // not offer to invent a file.
    let mut h = Harness::new();
    files_arrive(&mut h, &["src/app.rs"]);
    h.keys("zzzz");
    let p = h.app.picker.as_ref().unwrap();
    assert_eq!(p.create, None);
    assert_eq!(p.len(), 0);
}

// --- changes ------------------------------------------------------------

#[test]
fn f_in_the_review_jumps_the_cursor_to_the_chosen_file() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    let mut review = diff_of(checkout);
    let mut second = review.files[0].clone();
    second.path = "src/other.rs".to_string();
    review.files.push(second);
    open_review(&mut h, review);

    h.key(KeyCode::Char('f'));
    h.keys("other");
    h.key(KeyCode::Enter);

    let a = h.app.review.as_ref().unwrap().anchor().unwrap();
    assert_eq!(a.path, "src/other.rs");
    assert!(h.app.picker.is_none());
}

#[test]
fn the_change_picker_lists_the_files_with_their_markers() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));
    h.key(KeyCode::Char('f'));
    assert_eq!(h.app.picker.as_ref().unwrap().items, vec!["M src/a.rs"]);
}
