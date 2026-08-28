//! What one tree row says: its git summary, its rolled-up state, its
//! glyph, and the detail line under its name.

use super::*;

// --- git summary --------------------------------------------------------

#[test]
fn a_non_repo_checkout_shows_no_git_summary() {
    assert!(git_spans(None, Theme::default()).is_empty());
}

#[test]
fn a_clean_branch_says_so_rather_than_leaving_it_to_be_inferred() {
    let s = git_spans(Some(&git(Some("master"), false, 0, 0, 0)), Theme::default());
    assert_eq!(text_of(&s).trim(), "master  clean");
}

#[test]
fn one_change_is_not_reported_as_one_changes() {
    let s = git_spans(Some(&git(Some("m"), true, 1, 0, 0)), Theme::default());
    assert_eq!(text_of(&s).trim(), "m  1 change");
}

#[test]
fn a_detached_head_says_so() {
    let s = git_spans(Some(&git(None, false, 0, 0, 0)), Theme::default());
    assert_eq!(text_of(&s).trim(), "detached");
}

#[test]
fn ahead_behind_and_dirty_render_in_a_fixed_order() {
    let s = git_spans(Some(&git(Some("wt"), true, 5, 1, 2)), Theme::default());
    assert_eq!(text_of(&s).trim(), "wt  ↑1  ↓2  5 changes");
}

#[test]
fn zero_counts_are_omitted_rather_than_shown_as_zero() {
    let s = text_of(&git_spans(
        Some(&git(Some("m"), false, 0, 0, 0)),
        Theme::default(),
    ));
    assert!(
        !s.contains('↑') && !s.contains('↓') && !s.contains('0'),
        "{s:?}"
    );
}

#[test]
fn dirty_and_behind_share_the_warn_role_while_ahead_reads_as_ok() {
    // The colors carry meaning on their own; a reader shouldn't have to
    // parse the arrows to know whether something needs attention.
    let th = Theme::default();
    let s = git_spans(Some(&git(Some("m"), true, 1, 1, 1)), th);
    let color_of = |needle: &str| {
        s.iter()
            .find(|sp| sp.content.contains(needle))
            .unwrap_or_else(|| panic!("no span containing {needle:?}"))
            .style
            .fg
    };
    assert_eq!(color_of("↑"), Some(th.ok));
    assert_eq!(color_of("↓"), Some(th.warn));
    assert_eq!(color_of("change"), Some(th.warn));
}

// --- rolled-up status ---------------------------------------------------

#[test]
fn a_parent_with_no_panes_has_no_status() {
    assert_eq!(worst_pane_status(&checkout_with(&[])), None);
}

#[test]
fn waiting_outranks_everything_because_it_is_blocked_on_you() {
    let c = checkout_with(&[
        PaneStatus::Working,
        PaneStatus::Waiting,
        PaneStatus::Exited { code: Some(1) },
    ]);
    assert_eq!(worst_pane_status(&c), Some(PaneStatus::Waiting));
}

#[test]
fn a_failed_exit_outranks_the_calm_states() {
    let c = checkout_with(&[PaneStatus::Idle, PaneStatus::Exited { code: Some(1) }]);
    assert_eq!(
        worst_pane_status(&c),
        Some(PaneStatus::Exited { code: Some(1) })
    );
}

#[test]
fn a_clean_exit_ranks_below_a_live_pane() {
    let c = checkout_with(&[PaneStatus::Exited { code: Some(0) }, PaneStatus::Idle]);
    assert_eq!(worst_pane_status(&c), Some(PaneStatus::Idle));
}

#[test]
fn descendant_status_rolls_up_through_checkout_repository_and_project() {
    let mut c = checkout_with(&[PaneStatus::Idle]);
    c.panes[0].children.push(ChildAgentInfo {
        label: "blocked child".to_string(),
        status: PaneStatus::Waiting,
        note: None,
    });
    let repository = RepositoryInfo {
        id: RepositoryId(2),
        name: "repo".to_string(),
        branches: Vec::new(),
        default_branch: None,
        remote_branches: Vec::new(),
        checkouts: vec![c],
    };
    let project = ProjectInfo {
        id: ProjectId(3),
        name: "project".to_string(),
        repositories: vec![repository],
        notes: Default::default(),
        has_note: false,
    };

    let checkout_status = worst_pane_status(&project.repositories[0].checkouts[0]);
    let repository_status = project.repositories[0]
        .checkouts
        .iter()
        .filter_map(worst_pane_status)
        .max_by_key(|s| s.urgency());
    let project_status = project
        .repositories
        .iter()
        .flat_map(|repository| repository.checkouts.iter())
        .filter_map(worst_pane_status)
        .max_by_key(|s| s.urgency());

    assert_eq!(checkout_status, Some(PaneStatus::Waiting));
    assert_eq!(repository_status, Some(PaneStatus::Waiting));
    assert_eq!(project_status, Some(PaneStatus::Waiting));
}

#[test]
fn a_kill_with_no_exit_code_counts_as_a_failure() {
    assert_eq!(
        PaneStatus::Exited { code: None }.urgency(),
        PaneStatus::Exited { code: Some(1) }.urgency()
    );
}

// --- the status glyph ---------------------------------------------------

#[test]
fn every_status_has_a_shape_distinct_glyph() {
    let th = Theme::default();
    let statuses = [
        PaneStatus::Idle,
        PaneStatus::Working,
        PaneStatus::Waiting,
        PaneStatus::NeedsReview,
        PaneStatus::Done,
        PaneStatus::Failed,
        PaneStatus::Exited { code: Some(0) },
        PaneStatus::Exited { code: Some(1) },
    ];
    let glyphs = statuses.map(|status| status_dot(Some(status), th).content.trim().to_string());

    for (i, glyph) in glyphs.iter().enumerate() {
        assert!(
            !glyphs[..i].contains(glyph),
            "{status:?} reuses glyph {glyph:?}",
            status = statuses[i]
        );
    }
    assert_ne!(
        status_dot(Some(PaneStatus::Done), th).content,
        status_dot(Some(PaneStatus::Exited { code: Some(0) }), th).content,
        "reviewed completion and process exit remain different states"
    );
}

#[test]
fn each_live_state_gets_its_own_color() {
    let th = Theme::default();
    assert_eq!(status_dot(Some(PaneStatus::Idle), th).style.fg, Some(th.ok));
    assert_eq!(
        status_dot(Some(PaneStatus::Working), th).style.fg,
        Some(th.warn)
    );
    assert_eq!(
        status_dot(Some(PaneStatus::Waiting), th).style.fg,
        Some(th.err)
    );
    assert_eq!(
        status_dot(Some(PaneStatus::NeedsReview), th).style.fg,
        Some(th.err)
    );
    assert_eq!(status_dot(Some(PaneStatus::Done), th).style.fg, Some(th.ok));
}

#[test]
fn exits_are_a_box_or_a_cross_not_a_live_state_glyph() {
    let th = Theme::default();
    let clean = status_dot(Some(PaneStatus::Exited { code: Some(0) }), th);
    let failed = status_dot(Some(PaneStatus::Exited { code: Some(1) }), th);
    assert_eq!(clean.content.trim(), "□");
    assert_eq!(failed.content.trim(), "✗");
    assert_eq!(failed.style.fg, Some(th.err), "a failure must be loud");
    assert_eq!(clean.style.fg, Some(th.dim), "a clean exit must not be");
}

#[test]
fn only_a_failing_exit_gets_spelled_out_in_words() {
    assert_eq!(exit_note(PaneStatus::Exited { code: Some(0) }), "");
    assert_eq!(exit_note(PaneStatus::Idle), "");
    assert!(exit_note(PaneStatus::Exited { code: Some(2) }).contains("exit 2"));
    assert!(exit_note(PaneStatus::Exited { code: None }).contains("killed"));
}

#[test]
fn a_pane_with_nothing_to_say_falls_back_to_its_state() {
    let th = Theme::default();
    assert_eq!(
        text_of(&pane_detail(&pane(PaneStatus::Working, None), th)),
        "working"
    );
    assert_eq!(
        text_of(&pane_detail(&pane(PaneStatus::Waiting, None), th)),
        "needs you"
    );
    assert_eq!(
        text_of(&pane_detail(&pane(PaneStatus::Failed, None), th)),
        "failed"
    );
    assert_eq!(
        text_of(&pane_detail(&pane(PaneStatus::NeedsReview, None), th)),
        "needs review"
    );
    assert_eq!(
        text_of(&pane_detail(&pane(PaneStatus::Done, None), th)),
        "done"
    );
}

#[test]
fn a_renamed_row_still_says_which_agent_is_in_it() {
    // The agent takes the name over as soon as it knows its task, so
    // without this a column of renamed rows stops telling you which
    // CLI to expect behind any of them.
    let th = Theme::default();
    let mut p = pane(PaneStatus::Working, None);
    p.title = "fixing the pty deadlock".to_string();
    p.template = Some("opencode".to_string());
    assert_eq!(text_of(&pane_detail(&p, th)), "opencode  working");

    p.note = Some("needs the db password".to_string());
    assert_eq!(
        text_of(&pane_detail(&p, th)),
        "opencode  needs the db password"
    );
}

#[test]
fn a_working_child_gets_its_own_row_under_its_pane() {
    // Agents spawned inside a pane cannot touch its row, so without a
    // row of their own there is nothing on screen saying they are
    // there at all.
    let mut app = app_with_tree();
    if let Some(p) = app.tree[0].repositories[0].checkouts[0].panes.get_mut(0) {
        p.children = vec![
            ChildAgentInfo {
                label: "searching the hook table".to_string(),
                status: PaneStatus::Working,
                note: None,
            },
            ChildAgentInfo {
                label: "running the tests".to_string(),
                status: PaneStatus::Waiting,
                note: Some("needs a password".to_string()),
            },
        ];
    }
    let out = lines(&draw_at(&mut app, 160, 24)).join(
        "
",
    );
    assert!(
        out.contains("⤷"),
        "children are marked as such:
{out}"
    );
    assert!(out.contains("searching the"), "{out}");
    assert!(out.contains("running the tests"), "{out}");
    // A child stalled on a human says so where the parent's note goes,
    // which is the whole reason to list it.
    assert!(out.contains("needs a password"), "{out}");
}

#[test]
fn the_selection_stays_on_the_pane_when_children_push_rows_down() {
    let mut app = app_with_tree();
    if let Some(p) = app.tree[0].repositories[0].checkouts[0].panes.get_mut(0) {
        p.children = vec![ChildAgentInfo {
            label: "searching the hook table".to_string(),
            status: PaneStatus::Working,
            note: None,
        }];
    }
    app.focus = Focus::Panes;
    app.sel_pane = 1;
    let buf = draw_at(&mut app, 160, 24);
    let inner = app.layout.panes.inner;
    // Row 0 is the pane, row 1 its child, so the second pane is row 2.
    let marker = buf.cell((inner.x, inner.y + ROW_HEIGHT * 2)).unwrap();
    assert_eq!(marker.symbol(), MARKER, "the marker follows the pane's row");
}

#[test]
fn flat_pane_rows_show_panes_from_other_checkouts_with_their_path() {
    let mut app = app_with_tree();
    let mut feature = pane(PaneStatus::Working, Some("updating navigation"));
    feature.id = PaneId(102);
    feature.title = "feature agent".to_string();
    app.tree[0].repositories[0].checkouts[1].panes.push(feature);

    let grouped = lines(&draw_at(&mut app, 200, 24)).join("\n");
    assert!(!grouped.contains("feature agent"));

    app.settings.pane_view = crate::settings::PaneView::Flat;
    let flat = lines(&draw_at(&mut app, 200, 24)).join("\n");
    assert!(flat.contains("panes · all"), "{flat}");
    assert!(flat.contains("feature agent"), "{flat}");
    assert!(flat.contains("argus › orion › feat"), "{flat}");
}

#[test]
fn a_row_still_called_after_its_agent_does_not_say_so_twice() {
    let th = Theme::default();
    let mut p = pane(PaneStatus::Working, None);
    p.title = "opencode".to_string();
    p.template = Some("opencode".to_string());
    assert_eq!(text_of(&pane_detail(&p, th)), "working");
}

#[test]
fn a_stalled_pane_says_what_it_wants_instead_of_that_it_wants_something() {
    // The whole point of the note: "needs you" still costs you a trip
    // into the pane to find out what for.
    let th = Theme::default();
    let spans = pane_detail(
        &pane(PaneStatus::Waiting, Some("needs the db password")),
        th,
    );
    assert_eq!(text_of(&spans), "needs the db password");
    assert_eq!(
        spans[0].style.fg,
        Some(th.err),
        "a blocked row should read as one"
    );
}

#[test]
fn a_note_on_a_calm_pane_is_not_dressed_as_an_alarm() {
    let th = Theme::default();
    let spans = pane_detail(&pane(PaneStatus::Working, Some("rewriting the parser")), th);
    assert_eq!(spans[0].style.fg, Some(th.muted));
}

#[test]
fn an_empty_note_is_not_an_empty_line() {
    let th = Theme::default();
    assert_eq!(
        text_of(&pane_detail(&pane(PaneStatus::Idle, Some("")), th)),
        "idle"
    );
}

#[test]
fn a_failed_pane_outranks_the_calm_ones_but_not_a_waiting_one() {
    // Parents show the worst child; both want you, and the one you can
    // still answer wants you most.
    assert!(PaneStatus::Failed.urgency() > PaneStatus::Working.urgency());
    assert!(PaneStatus::Failed.urgency() > PaneStatus::Exited { code: Some(1) }.urgency());
    assert!(PaneStatus::NeedsReview.urgency() > PaneStatus::Exited { code: Some(1) }.urgency());
    assert!(PaneStatus::Waiting.urgency() > PaneStatus::Failed.urgency());
}

#[test]
fn a_failed_pane_is_still_running_so_it_is_not_an_exit_cross() {
    // A cross would read as "this is over"; it isn't.
    let th = Theme::default();
    let failed = status_dot(Some(PaneStatus::Failed), th);
    assert_eq!(failed.content.trim(), "■");
    assert_eq!(failed.style.fg, Some(th.err));
}
