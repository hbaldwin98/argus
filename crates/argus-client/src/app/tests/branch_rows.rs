//! The checkouts column when it also lists branches that have no
//! directory of their own.

use super::*;
#[test]
fn the_branches_stay_out_of_the_column_until_they_are_asked_for() {
    // The column is for what is running. Forty branches on top of two
    // checkouts is the checkouts buried, not the branches surfaced.
    let mut h = Harness::new();
    h.app.tree[0].repositories[0].branches =
        vec!["hotfix/tls".to_string(), "spike".to_string()];
    h.keys("ll");

    assert_eq!(h.app.checkout_row_count(), 2, "the two checkouts, only");
    assert_eq!(h.app.current_branch_row(), None);

    h.key(KeyCode::Char('B'));
    assert_eq!(h.app.checkout_row_count(), 4);

    h.key(KeyCode::Char('B'));
    assert_eq!(h.app.checkout_row_count(), 2, "and away again");
}

#[test]
fn the_main_branch_leads_the_column_even_with_nothing_sitting_on_it() {
    // Whatever it is named: this repository's is "trunk", and it is
    // still the row everything else is measured against.
    let mut h = Harness::new();
    let r = &mut h.app.tree[0].repositories[0];
    r.branches = vec!["spike".to_string(), "trunk".to_string()];
    r.default_branch = Some("trunk".to_string());
    h.keys("ll");

    assert_eq!(
        h.app.checkout_row_count(),
        3,
        "the main branch plus the two checkouts — and not `spike`"
    );
    assert_eq!(h.app.current_branch_row(), Some("trunk"), "at the top");
    h.key(KeyCode::Char('j'));
    assert_eq!(
        h.app.current_checkout().map(|c| c.id),
        Some(CheckoutId(10)),
        "the checkouts follow it in their own order"
    );
}

#[test]
fn the_checkout_sitting_on_the_main_branch_leads_the_column() {
    // Same rule, the other way round: `feat` is the second checkout in
    // the tree, but it is where main lives, so it is the first row.
    let mut h = Harness::new();
    h.app.tree[0].repositories[0].default_branch = Some("feat".to_string());
    h.keys("ll");

    assert_eq!(h.app.checkout_row_count(), 2, "no branch row is invented");
    assert_eq!(h.app.current_checkout().map(|c| c.id), Some(CheckoutId(11)));
}

#[test]
fn d_on_a_branch_row_offers_to_delete_the_branch_itself() {
    let mut h = harness_on_a_branch_row();

    h.key(KeyCode::Char('D'));
    match &h.app.prompt {
        Some(Prompt::ConfirmRemove { target, label }) => {
            assert_eq!(
                *target,
                RemoveTarget::Branch {
                    checkout: CheckoutId(10),
                    branch: "hotfix/tls".to_string(),
                    force: false,
                },
                "the primary checkout is what git is run from"
            );
            assert_eq!(label, "hotfix/tls");
        }
        _ => panic!("expected a confirmation prompt"),
    }
    assert!(h.sent().is_empty(), "nothing sent before confirming");

    h.key(KeyCode::Char('y'));
    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::DeleteBranch { checkout: CheckoutId(10), branch, force: false }]
            if branch == "hotfix/tls"
    ));
}

#[test]
fn an_unmerged_branch_comes_back_as_the_harder_question() {
    let mut h = harness_on_a_branch_row();
    h.key(KeyCode::Char('D'));
    h.key(KeyCode::Char('y'));
    h.sent();

    h.app.on_server_msg(ServerMsg::BranchNotMerged {
        checkout: CheckoutId(10),
        branch: "hotfix/tls".to_string(),
    });

    match &h.app.prompt {
        Some(Prompt::ConfirmRemove { target, label }) => {
            assert_eq!(
                *target,
                RemoveTarget::Branch {
                    checkout: CheckoutId(10),
                    branch: "hotfix/tls".to_string(),
                    force: true,
                }
            );
            assert_eq!(label, "hotfix/tls");
        }
        _ => panic!("expected the forced-delete confirmation"),
    }
    assert!(
        h.app.status.is_empty(),
        "the refusal is the popup, not an alert on top of it"
    );

    h.key(KeyCode::Char('y'));
    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::DeleteBranch { checkout: CheckoutId(10), branch, force: true }]
            if branch == "hotfix/tls"
    ));
}

#[test]
fn the_harder_question_can_still_be_declined() {
    let mut h = harness_on_a_branch_row();
    h.app.on_server_msg(ServerMsg::BranchNotMerged {
        checkout: CheckoutId(10),
        branch: "hotfix/tls".to_string(),
    });
    h.sent();

    h.key(KeyCode::Esc);

    assert!(h.app.prompt.is_none());
    assert!(h.sent().is_empty(), "nothing forced on the way out");
}

#[test]
fn a_remote_only_branch_is_offered_under_the_name_it_would_have_here() {
    let mut h = harness_on_a_remote_branch_row();

    assert_eq!(
        h.app.current_remote_row(),
        Some("origin/from-elsewhere"),
        "the row says where it is"
    );
    assert_eq!(
        h.app.current_branch_row(),
        Some("from-elsewhere"),
        "but what you would switch to is the branch, not the remote's name for it"
    );

    h.key(KeyCode::Enter);
    assert!(
        matches!(
            h.sent().as_slice(),
            [ClientMsg::SwitchBranch { checkout: CheckoutId(10), branch }]
                if branch == "from-elsewhere"
        ),
        "git makes the local branch off the remote one; we only name it"
    );
}

#[test]
fn n_on_a_remote_branch_gives_it_a_worktree_under_its_local_name() {
    let mut h = harness_on_a_remote_branch_row();

    h.key(KeyCode::Char('n'));

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::CreateWorktree { checkout: CheckoutId(10), branch }]
            if branch == "from-elsewhere"
    ));
}

#[test]
fn d_on_a_remote_branch_is_refused_rather_than_becoming_a_push() {
    let mut h = harness_on_a_remote_branch_row();

    h.key(KeyCode::Char('D'));

    assert!(h.app.prompt.is_none(), "no confirmation is even offered");
    assert!(h.sent().is_empty());
    assert!(h.app.status.contains("remote"), "got {:?}", h.app.status);
}

#[test]
fn fetch_and_pull_run_in_the_selected_checkout() {
    let mut h = Harness::new();
    h.keys("llj"); // the linked worktree
    h.sent();

    h.key(KeyCode::Char('F'));
    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::Fetch {
            checkout: CheckoutId(11)
        }]
    ));

    h.key(KeyCode::Char('P'));
    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::Pull {
            checkout: CheckoutId(11)
        }]
    ));
}

#[test]
fn a_fetch_from_a_branch_row_falls_back_to_the_primary_checkout() {
    // A branch with no directory has nowhere of its own to run git.
    let mut h = harness_on_a_remote_branch_row();

    h.key(KeyCode::Char('F'));

    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::Fetch {
            checkout: CheckoutId(10)
        }]
    ));
}

#[test]
fn branch_rows_come_after_the_checkouts_and_carry_no_checkout() {
    let mut h = harness_on_a_branch_row();

    assert_eq!(h.app.checkout_row_count(), 4, "two checkouts, two branches");
    assert_eq!(h.app.current_branch_row(), Some("hotfix/tls"));
    assert!(
        h.app.current_checkout().is_none(),
        "a branch row has no checkout, so nothing hangs off it"
    );
    assert!(h.app.current_pane().is_none());

    h.key(KeyCode::Char('j'));
    assert_eq!(h.app.current_branch_row(), Some("spike"));
    h.key(KeyCode::Char('j'));
    assert_eq!(
        h.app.current_branch_row(),
        Some("spike"),
        "the last branch is the last row"
    );
}

#[test]
fn enter_on_a_branch_row_switches_the_primary_checkout_to_it() {
    let mut h = harness_on_a_branch_row();

    h.key(KeyCode::Enter);

    assert!(
        matches!(
            h.sent().as_slice(),
            [ClientMsg::SwitchBranch { checkout: CheckoutId(10), branch }] if branch == "hotfix/tls"
        ),
        "the primary checkout is where a branch with no directory goes"
    );
    assert_eq!(
        h.app.focus,
        Focus::Checkouts,
        "there is nothing to descend into"
    );
}

#[test]
fn n_on_a_branch_row_gives_that_branch_a_worktree_without_asking_for_a_name() {
    let mut h = harness_on_a_branch_row();

    h.key(KeyCode::Char('n'));

    assert!(h.app.prompt.is_none(), "the branch is already named");
    assert!(matches!(
        h.sent().as_slice(),
        [ClientMsg::CreateWorktree { checkout: CheckoutId(10), branch }] if branch == "hotfix/tls"
    ));
}

#[test]
fn a_branch_that_gets_a_checkout_stops_being_a_row_of_its_own() {
    // The daemon decides this — a branch is listed only while no
    // checkout is on it — so the client must not hold a selection that
    // outlives the row.
    let mut h = harness_on_a_branch_row();
    h.key(KeyCode::Char('j')); // the last row

    let mut tree = tree();
    tree[0].repositories[0].branches = vec!["hotfix/tls".to_string()];
    h.app.on_server_msg(ServerMsg::Tree(tree));

    assert_eq!(h.app.checkout_row_count(), 3);
    assert_eq!(h.app.sel_checkout, 2, "clamped onto the row that is left");
    assert_eq!(h.app.current_branch_row(), Some("hotfix/tls"));
}
