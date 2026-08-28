//! The diff and history viewers, and the comments sent from them.

use super::*;
#[test]
fn r_asks_the_daemon_for_the_selected_checkouts_diff() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('l')); // into the checkouts column
    let checkout = h.app.current_checkout().unwrap().id;
    h.key(KeyCode::Char('R'));

    match &h.sent()[0] {
        ClientMsg::Review { checkout: c, .. } => assert_eq!(*c, checkout),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn the_arriving_diff_opens_the_viewer_and_takes_focus() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));

    assert!(h.app.review.is_some());
    assert_eq!(h.app.focus, Focus::Review);
}

#[test]
fn a_diff_for_a_checkout_the_user_left_is_dropped() {
    // It was computed on a blocking thread; by the time it lands the
    // user may be looking at something else entirely.
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    h.key(KeyCode::Char('l'));
    h.key(KeyCode::Char('R'));
    h.sent();

    h.app
        .on_server_msg(ServerMsg::Review(diff_of(CheckoutId(9999))));
    assert!(
        h.app.review.is_none(),
        "not for the checkout we asked about"
    );

    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
    assert!(h.app.review.is_some());
}

#[test]
fn only_the_exact_latest_review_request_is_accepted() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('l'));
    h.key(KeyCode::Char('R'));
    h.key(KeyCode::Char('R'));
    h.sent();
    let checkout = h.app.current_checkout().unwrap().id;

    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
    assert!(h.app.review.is_none());

    let mut latest = diff_of(checkout);
    latest.request_id = 2;
    h.app.on_server_msg(ServerMsg::Review(latest));
    assert!(h.app.review.is_some());
}

#[test]
fn an_unsolicited_diff_never_hijacks_the_screen() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));
    assert!(h.app.review.is_none());
    assert_eq!(h.app.focus, Focus::Projects);
}

#[test]
fn a_clean_checkout_says_so_instead_of_opening_an_empty_viewer() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(
        &mut h,
        argus_protocol::Review {
            request_id: 1,
            checkout,
            base: argus_protocol::ReviewBase::Unstaged,
            files: Vec::new(),
            commit: None,
        },
    );
    assert!(h.app.review.is_none());
    assert_ne!(h.app.focus, Focus::Review);
    assert!(
        h.app.status.contains("no changes vs unstaged"),
        "{}",
        h.app.status
    );
}

#[test]
fn esc_closes_the_review_and_lands_back_on_the_checkout() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));
    h.key(KeyCode::Esc);

    assert!(h.app.review.is_none());
    assert_eq!(h.app.focus, Focus::Checkouts);
}

#[test]
fn navigation_keys_move_within_the_diff_not_the_tree() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));
    let before = (h.app.sel_project, h.app.sel_checkout, h.app.sel_pane);

    h.key(KeyCode::Char('j'));

    assert_eq!(
        (h.app.sel_project, h.app.sel_checkout, h.app.sel_pane),
        before,
        "j belongs to the diff while it's up"
    );
    let v = h.app.review.as_ref().unwrap();
    assert_eq!(v.anchor().unwrap().text, vec!["+new"]);
}

#[test]
fn r_inside_the_review_re_requests_rather_than_reusing_a_stale_diff() {
    // An agent is very likely still editing the tree underneath it.
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));

    h.key(KeyCode::Char('r'));
    assert!(matches!(h.sent()[0], ClientMsg::Review { .. }));
}

#[test]
fn v_then_j_selects_a_range_of_lines() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));

    h.key(KeyCode::Char('v'));
    h.key(KeyCode::Char('j'));

    let a = h.app.review.as_ref().unwrap().anchor().unwrap();
    assert_eq!(a.path, "src/a.rs");
    assert_eq!(a.text, vec![" keep", "+new"]);
}

#[test]
fn typing_in_the_review_never_reaches_a_pane() {
    // The review shares its column with the live pane; a keystroke
    // leaking into the child would be silent and destructive.
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));

    h.keys("jkgG");
    assert!(
        !h.sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Input { .. })),
        "no input should be forwarded"
    );
}

#[test]
fn c_opens_a_comment_prompt_anchored_to_the_cursor() {
    let mut h = review_with_agent();
    h.key(KeyCode::Char('c'));
    match &h.app.prompt {
        Some(Prompt::Comment { anchor, .. }) => assert_eq!(anchor.path, "src/a.rs"),
        _ => panic!("no comment prompt"),
    }
}

#[test]
fn a_comment_is_typed_at_the_agent_and_submitted() {
    let mut h = review_with_agent();
    let checkout = h.app.review.as_ref().unwrap().review.checkout;
    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Char('c'));
    h.keys("fix this");
    h.key(KeyCode::Enter);

    match &h.sent()[0] {
        ClientMsg::ReviewComment {
            checkout: sent_checkout,
            recipient,
            anchor,
            body,
        } => {
            assert_eq!(*sent_checkout, checkout);
            assert_eq!(*recipient, PaneId(51), "the agent, not the shell");
            assert_eq!(anchor.notification(body), "src/a.rs:2 `+new`: fix this");
            assert_eq!(body, "fix this");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(h.app.prompt.is_none());
}

#[test]
fn a_comment_chooses_between_multiple_live_agents() {
    let mut h = review_with_agent();
    h.app.tree[0].repositories[0].checkouts[0]
        .panes
        .push(PaneInfo {
            id: PaneId(52),
            kind: PaneKind::Agent,
            title: "fix tests".to_string(),
            status: PaneStatus::Working,
            note: None,
            template: Some("codex".to_string()),
            children: Vec::new(),
        });

    h.key(KeyCode::Char('c'));
    h.keys("route this");
    h.key(KeyCode::Enter);

    let picker = h.app.picker.as_ref().expect("recipient picker");
    assert!(matches!(
        &picker.kind,
        PickerKind::ReviewRecipient { panes, .. }
            if panes == &[PaneId(51), PaneId(52)]
    ));
    assert!(picker.items[1].contains("fix tests"));
    assert!(picker.items[1].contains("codex"));
    assert!(picker.items[1].contains("#52"));
    assert!(h.sent().is_empty(), "nothing is sent before choosing");

    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Enter);
    assert!(matches!(
        &h.sent()[0],
        ClientMsg::ReviewComment { recipient: PaneId(52), body, .. }
            if body == "route this"
    ));
}

#[test]
fn an_exited_agent_is_not_offered_as_a_comment_recipient() {
    let mut h = review_with_agent();
    h.app.tree[0].repositories[0].checkouts[0]
        .panes
        .push(PaneInfo {
            id: PaneId(52),
            kind: PaneKind::Agent,
            title: "old agent".to_string(),
            status: PaneStatus::Exited { code: Some(0) },
            note: None,
            template: Some("codex".to_string()),
            children: Vec::new(),
        });

    h.key(KeyCode::Char('c'));
    h.keys("only live agents");
    h.key(KeyCode::Enter);

    assert!(h.app.picker.is_none());
    assert!(matches!(
        h.sent()[0],
        ClientMsg::ReviewComment {
            recipient: PaneId(51),
            ..
        }
    ));
}

#[test]
fn a_comment_with_no_agent_to_read_it_says_so_and_sends_nothing() {
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    open_review(&mut h, diff_of(checkout));
    h.key(KeyCode::Char('c'));
    h.keys("hello");
    h.key(KeyCode::Enter);

    assert!(h.sent().is_empty());
    assert!(h.app.status.contains("no agent"), "{}", h.app.status);
}

#[test]
fn an_empty_comment_sends_nothing() {
    let mut h = review_with_agent();
    h.key(KeyCode::Char('c'));
    h.key(KeyCode::Enter);
    assert!(h.sent().is_empty());
    assert!(h.app.prompt.is_none());
}

#[test]
fn escaping_the_comment_prompt_sends_nothing_and_leaves_the_review_up() {
    let mut h = review_with_agent();
    h.key(KeyCode::Char('c'));
    h.keys("never mind");
    h.key(KeyCode::Esc);

    assert!(h.sent().is_empty());
    assert!(h.app.prompt.is_none());
    assert!(h.app.review.is_some());
    assert_eq!(h.app.focus, Focus::Review);
}

#[test]
fn a_comment_on_a_range_is_sent_as_one_message() {
    let mut h = review_with_agent();
    h.key(KeyCode::Char('v'));
    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Char('c'));
    h.keys("both lines");
    h.key(KeyCode::Enter);

    let sent = h.sent();
    assert_eq!(sent.len(), 1);
    match &sent[0] {
        ClientMsg::ReviewComment { anchor, body, .. } => {
            assert_eq!(
                anchor.notification(body),
                "src/a.rs:1-2 (2 lines): both lines"
            )
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_saved_comment_reports_delivery_from_the_daemon() {
    let mut h = Harness::new();
    h.app.on_server_msg(ServerMsg::ReviewCommentSaved {
        id: 7,
        delivered: true,
    });
    assert_eq!(h.app.status, "comment #7 saved and sent");

    h.app.on_server_msg(ServerMsg::ReviewCommentSaved {
        id: 8,
        delivered: false,
    });
    assert_eq!(h.app.status, "comment #8 saved; agent unavailable");
}

#[test]
fn review_keys_do_not_leak_into_the_comment_being_typed() {
    let mut h = review_with_agent();
    h.key(KeyCode::Char('c'));
    h.keys("jkgG");
    match &h.app.prompt {
        Some(Prompt::Comment { input, .. }) => assert_eq!(input, "jkgG"),
        _ => panic!("no comment prompt"),
    }
}

#[test]
fn n_cycles_through_panes_that_need_attention() {
    let mut h = Harness::new();
    let mut updated = tree();
    updated[0].repositories[0].checkouts[0].panes[1].status = PaneStatus::Waiting;
    updated[0].repositories[0].checkouts[0].panes[1].note =
        Some("needs a password".to_string());
    let mut review_pane = pane(102, "review agent");
    review_pane.status = PaneStatus::NeedsReview;
    updated[0].repositories[0].checkouts[1]
        .panes
        .push(review_pane);
    h.app.on_server_msg(ServerMsg::Tree(updated));
    h.sent();

    h.key(KeyCode::Char('N'));
    assert_eq!(h.app.column_pane(), Some(PaneId(101)));
    assert_eq!(h.app.focus, Focus::PaneContent);
    assert!(h.app.status.contains("needs a password"));

    h.leader();
    h.key(KeyCode::Char('N'));
    assert_eq!(h.app.column_pane(), Some(PaneId(102)));

    h.leader();
    h.key(KeyCode::Char('N'));
    assert_eq!(h.app.column_pane(), Some(PaneId(101)), "cycles at the end");
    assert!(
        !h.sent()
            .iter()
            .any(|message| matches!(message, ClientMsg::Input { .. })),
        "the leader chord must not reach a child"
    );
}

#[test]
fn n_reports_when_no_pane_needs_attention() {
    let mut h = Harness::new();
    h.key(KeyCode::Char('N'));
    assert!(h.app.status.contains("no panes need attention"));
    assert_eq!(h.app.focus, Focus::Projects);
}

#[test]
fn n_opens_the_parent_of_a_child_that_needs_attention() {
    let mut h = Harness::new();
    let mut updated = tree();
    updated[0].repositories[0].checkouts[0].panes[1]
        .children
        .push(argus_protocol::ChildAgentInfo {
            label: "database helper".to_string(),
            status: PaneStatus::Waiting,
            note: Some("needs credentials".to_string()),
        });
    h.app.on_server_msg(ServerMsg::Tree(updated));

    h.key(KeyCode::Char('N'));

    assert_eq!(h.app.column_pane(), Some(PaneId(101)));
    assert!(h.app.status.contains("claude / database helper"));
    assert!(h.app.status.contains("needs credentials"));
}

#[test]
fn e_opens_the_file_under_the_cursor_at_its_line() {
    let mut h = review_with_agent();
    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Char('e'));

    match &h.sent()[0] {
        ClientMsg::OpenInEditor { path, line, .. } => {
            assert_eq!(path, "src/a.rs");
            assert_eq!(*line, Some(2));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn opening_an_editor_gives_it_the_column_the_review_was_using() {
    let mut h = review_with_agent();
    h.key(KeyCode::Char('e'));
    assert!(h.app.review.is_none());
}

#[test]
fn b_toggles_the_diff_side_and_asks_again() {
    let mut h = review_with_agent();
    h.key(KeyCode::Char('b'));

    match &h.sent()[0] {
        ClientMsg::Review { base, .. } => {
            assert_eq!(*base, argus_protocol::ReviewBase::Staged)
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn the_chosen_base_sticks_across_reopens() {
    // `b` is a setting, not a per-visit choice.
    let mut h = review_with_agent();
    h.key(KeyCode::Char('b'));
    h.key(KeyCode::Esc);
    h.sent();

    h.key(KeyCode::Char('R'));
    match &h.sent()[0] {
        ClientMsg::Review { base, .. } => {
            assert_eq!(*base, argus_protocol::ReviewBase::Staged)
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn b_on_a_commit_review_leaves_the_side_setting_alone() {
    // The side toggle is uncommitted-only. Flipping it here would
    // change which side the next `R` opens on, with nothing on
    // screen to show that it happened.
    let mut h = Harness::new();
    let checkout = h.app.tree[0].repositories[0].checkouts[0].id;
    let mut review = diff_of(checkout);
    review.base = argus_protocol::ReviewBase::Commit;
    review.commit = Some(argus_protocol::CommitInfo {
        oid: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        short: "aaaaaaa".into(),
        summary: "fix the thing".into(),
        author: "t".into(),
        time: 0,
    });
    open_review(&mut h, review);

    h.key(KeyCode::Char('b'));
    assert_eq!(h.app.review_base, argus_protocol::ReviewBase::Unstaged);
    assert!(h.sent().is_empty(), "b must not re-request a commit");

    h.key(KeyCode::Esc);
    h.key(KeyCode::Char('R'));
    match &h.sent()[0] {
        ClientMsg::Review { base, commit, .. } => {
            assert_eq!(*base, argus_protocol::ReviewBase::Unstaged);
            assert!(commit.is_none());
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn tab_reaches_the_review_from_the_tree_and_from_inside_a_pane() {
    // §5's entry point. Inside a pane it needs the leader, since a bare
    // Tab there belongs to the child.
    let mut h = Harness::new();
    h.key(KeyCode::Char('l'));
    h.key(KeyCode::Tab);
    assert!(matches!(h.sent()[0], ClientMsg::Review { .. }));

    h.keys("l");
    h.key(KeyCode::Char('s'));
    h.sent();
    h.app.focus = Focus::PaneContent;
    h.leader();
    h.key(KeyCode::Tab);
    assert!(
        h.sent()
            .iter()
            .any(|m| matches!(m, ClientMsg::Review { .. })),
        "leader-Tab should ask for the diff"
    );
}

#[test]
fn a_review_restores_the_columns_after_a_fullscreen_pane() {
    let mut h = Harness::new();
    h.keys("llll");
    let checkout = h.app.current_checkout().unwrap().id;
    h.leader();
    h.key(KeyCode::Char('f'));
    h.leader();
    h.key(KeyCode::Tab);
    h.sent();

    h.app.on_server_msg(ServerMsg::Review(diff_of(checkout)));

    assert_eq!(h.app.focus, Focus::Review);
    assert!(!h.app.pane_fullscreen);
}

#[test]
fn t_opens_the_theme_picker_on_the_theme_already_in_use() {
    let mut h = Harness::new();
    h.app.theme = crate::theme::Theme::by_name("frappe");
    h.key(KeyCode::Char('t'));

    let picker = h.app.picker.as_ref().expect("t should open the picker");
    assert_eq!(picker.items[picker.sel], "frappe");
}

#[test]
fn choosing_a_theme_swaps_the_palette_without_asking_the_daemon() {
    // The palette is the client's business; the daemon has no opinion.
    let mut h = Harness::new();
    h.key(KeyCode::Char('t'));
    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Enter);

    assert_eq!(h.app.theme, crate::theme::Theme::by_name("macchiato"));
    assert!(h.sent().is_empty());
    assert!(h.app.picker.is_none());
}

#[test]
fn escaping_the_theme_picker_leaves_the_palette_alone() {
    let mut h = Harness::new();
    let before = h.app.theme;
    h.key(KeyCode::Char('t'));
    h.key(KeyCode::Char('j'));
    h.key(KeyCode::Esc);
    assert_eq!(h.app.theme, before);
}

#[test]
fn clicking_the_content_frame_returns_to_what_it_is_showing() {
    let mut h = Harness::new();
    laid_out(&mut h);
    h.app.on_mouse(click(48, 0)); // the card's corner, not the live grid
    assert_eq!(h.app.focus, Focus::PaneContent);
}

#[test]
fn clicking_an_empty_content_column_does_not_trap_focus_there() {
    // Focusing a pane that doesn't exist is a mode with no keys in it.
    let mut h = Harness::new();
    let mut t = tree();
    t[0].repositories[0].checkouts[0].panes.clear();
    t[0].repositories[0].checkouts[1].panes.clear();
    h.app.on_server_msg(ServerMsg::Tree(t));
    laid_out(&mut h);
    h.sent();

    h.app.on_mouse(click(36, 0));
    assert_ne!(h.app.focus, Focus::PaneContent);
}

#[test]
fn clicking_a_column_leaves_the_live_pane_subscribed() {
    // "Move over there" must not tear down the session you were on.
    let mut h = Harness::new();
    laid_out(&mut h);
    h.keys("lll"); // down into the panes column, subscribing
    h.sent();
    let watching = h.app.column_pane();
    assert!(watching.is_some(), "precondition: something is being shown");

    h.app.on_mouse(click(0, 0)); // all the way back to projects

    assert_eq!(h.app.focus, Focus::Projects);
    assert_eq!(h.app.column_pane(), watching, "still showing the same pane");
}

#[test]
fn opening_history_asks_for_commits_and_nothing_else() {
    let mut h = Harness::new();
    let checkout = open_history(&mut h);
    assert!(h.app.history.is_some());
    assert!(matches!(h.app.overlay, Some(Overlay::History)));
    assert!(
        h.sent().is_empty(),
        "no commit is summarized until it is drilled into"
    );
    assert_eq!(h.app.history.as_ref().unwrap().checkout, checkout);
    assert_eq!(
        h.app.history.as_ref().unwrap().rows.len(),
        2,
        "two headers, no file rows"
    );
}

#[test]
fn drilling_into_a_commit_asks_for_that_commit_alone() {
    let mut h = Harness::new();
    let checkout = open_history(&mut h);

    h.key(KeyCode::Char('l'));
    match h.sent().as_slice() {
        [ClientMsg::ListCommitFiles {
            checkout: c,
            commit,
        }] => {
            assert_eq!(*c, checkout);
            assert_eq!(commit, "aaaa111");
        }
        other => panic!("unexpected {other:?}"),
    }

    commit_files_arrive(&mut h, checkout, "aaaa111");
    let view = h.app.history.as_ref().unwrap();
    assert_eq!(view.rows.len(), 3);
    assert_eq!(view.commits[0].files.as_ref().unwrap().len(), 1);
}

#[test]
fn drilling_into_an_unfolded_commit_opens_it_as_a_review() {
    let mut h = Harness::new();
    let checkout = open_history(&mut h);
    h.key(KeyCode::Char('l'));
    commit_files_arrive(&mut h, checkout, "aaaa111");
    h.sent();

    h.key(KeyCode::Char('l'));
    match h.sent().as_slice() {
        [ClientMsg::Review { base, commit, .. }] => {
            assert_eq!(*base, argus_protocol::ReviewBase::Commit);
            assert_eq!(commit.as_deref(), Some("aaaa111"));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn h_folds_the_commit_before_it_closes_the_overlay() {
    let mut h = Harness::new();
    let checkout = open_history(&mut h);
    h.key(KeyCode::Char('l'));
    commit_files_arrive(&mut h, checkout, "aaaa111");

    h.key(KeyCode::Char('h'));
    assert!(
        matches!(h.app.overlay, Some(Overlay::History)),
        "folded, not closed"
    );
    assert_eq!(h.app.history.as_ref().unwrap().rows.len(), 2);

    h.key(KeyCode::Char('h'));
    assert!(h.app.overlay.is_none());
}

#[test]
fn a_summary_for_a_checkout_the_user_left_is_dropped() {
    let mut h = Harness::new();
    open_history(&mut h);
    h.key(KeyCode::Char('l'));

    commit_files_arrive(&mut h, CheckoutId(9999), "aaaa111");
    assert!(h.app.history.as_ref().unwrap().commits[0].files.is_none());
}
