use super::*;
use crate::connection::page::TranscriptLine;
use jiff::tz::TimeZone;

fn clock() -> Clock {
    Clock::new("2026-10-21T15:30:00Z".parse().unwrap(), TimeZone::UTC)
}

fn row(live: bool) -> SessionRowView {
    SessionRowView { row: 0, title: "t".into(), harness: "claude".into(), checkout: "main".into(), live, turns: Some(31), ago: "1h".into(), removed: false, on: false }
}

fn preview(started: Option<&str>) -> SessionPreview {
    SessionPreview {
        title: "t".into(),
        harness: "claude".into(),
        checkout: "main".into(),
        session_id: "2b7f1c9e-41d3".into(),
        started: started.map(str::to_owned),
        loading: false,
        notice: None,
        entries: Vec::<TranscriptLine>::new(),
        earlier: 0,
        offer: "go",
        command: None,
        why: None,
        readable: true,
    }
}

fn reader(started: Option<&str>, ended: Option<&str>) -> ReaderPage {
    ReaderPage {
        session: preview(started),
        repository: "argus".into(),
        ended: ended.map(str::to_owned),
        turns: Some(1),
        jumps: Vec::new(),
        find: String::new(),
        matches: Vec::new(),
        found: None,
        target: None,
        moves: 0,
    }
}

#[test]
fn a_rows_meta_says_what_ran_where_and_how_many_turns() {
    assert_eq!(row_meta(&row(true)), "claude on main \u{b7} 31 turns");
    assert_eq!(row_meta(&SessionRowView { turns: Some(1), ..row(true) }), "claude on main \u{b7} 1 turn");
    assert_eq!(row_meta(&SessionRowView { turns: None, ..row(true) }), "claude on main");
}

#[test]
fn a_removed_worktree_is_named_as_one_with_its_tooltip() {
    let gone = SessionRowView { removed: true, checkout: "kitty-keys".into(), ..row(false) };
    assert_eq!(row_meta(&gone), "claude on kitty-keys, since removed \u{b7} 31 turns");
    assert_eq!(row_tip(&gone).as_deref(), Some("ran in kitty-keys, a worktree since removed"));
    assert_eq!(row_tip(&row(false)), None);
}

#[test]
fn the_ended_group_folds_only_with_a_live_session_and_nothing_searched_or_filtered() {
    let mut page = SessionsPage { rows: vec![row(true), row(false)], ..SessionsPage::default() };
    assert!(folds(&page));
    page.search = "  ".into();
    assert!(folds(&page), "blank is nothing searched");
    page.search = "hook".into();
    assert!(!folds(&page));
    page.search.clear();
    page.harness = Some("claude".into());
    assert!(!folds(&page));
    page.harness = None;
    page.checkout = Some("main".into());
    assert!(!folds(&page));
    page.checkout = None;
    page.rows = vec![row(false)];
    assert!(!folds(&page), "with none live it stays open");
}

#[test]
fn the_ended_summary_drops_an_unknown_newest() {
    let page = SessionsPage { ended_count: 4, ended_newest: "20m".into(), ..SessionsPage::default() };
    assert_eq!(ended_summary(&page), "4 \u{b7} newest 20m ago");
    for none in ["", "\u{2014}"] {
        assert_eq!(ended_summary(&SessionsPage { ended_newest: none.into(), ..page.clone() }), "4");
    }
}

#[test]
fn the_filter_menu_lists_every_then_each_and_checks_the_current_value() {
    let page = SessionsPage {
        harnesses: vec!["claude".into(), "codex".into()],
        harness: Some("codex".into()),
        checkouts: vec!["main".into()],
        checkout: None,
        ..SessionsPage::default()
    };
    let items = filter_items(&page);
    let lines: Vec<String> = items
        .iter()
        .map(|item| match item {
            FilterItem::Heading(text) => format!("# {text}"),
            FilterItem::Rule => "---".to_owned(),
            FilterItem::Choice { label, checked, .. } => format!("{}{label}", if *checked { "\u{2713} " } else { "" }),
        })
        .collect();
    assert_eq!(lines, ["# Harness", "Every harness", "claude", "\u{2713} codex", "---", "# Checkout", "\u{2713} Every checkout", "main"]);
    // Choosing one filter keeps the other.
    match &items[2] {
        FilterItem::Choice { harness, checkout, .. } => assert_eq!((harness.as_deref(), checkout.as_deref()), (Some("claude"), None)),
        other => panic!("{other:?}"),
    }
    match &items[7] {
        FilterItem::Choice { harness, checkout, .. } => assert_eq!((harness.as_deref(), checkout.as_deref()), (Some("codex"), Some("main"))),
        other => panic!("{other:?}"),
    }
    assert_eq!(filter_active(&page).as_deref(), Some("codex"));
    assert_eq!(filter_active(&SessionsPage { harness: Some("a".into()), checkout: Some("b".into()), ..SessionsPage::default() }).as_deref(), Some("a \u{b7} b"));
    assert_eq!(filter_active(&SessionsPage::default()), None);
}

#[test]
fn the_start_is_a_day_and_a_twelve_hour_time_and_an_unreadable_one_is_said_as_written() {
    assert_eq!(started(&preview(Some("2026-10-19T14:02:11Z")), &clock()).as_deref(), Some("started Oct 19, 2:02 PM"));
    assert_eq!(started(&preview(Some("2026-10-19T00:05:00Z")), &clock()).as_deref(), Some("started Oct 19, 12:05 AM"));
    assert_eq!(started(&preview(Some("2026-10-19T12:00:00Z")), &clock()).as_deref(), Some("started Oct 19, 12:00 PM"));
    assert_eq!(started(&preview(Some("yesterday-ish")), &clock()).as_deref(), Some("started yesterday-ish"));
    assert_eq!(started(&preview(None), &clock()), None);
}

#[test]
fn earlier_entries_are_counted_in_the_previews_and_the_readers_words() {
    assert_eq!(earlier(1, false), "1 earlier entry not shown");
    assert_eq!(earlier(3, false), "3 earlier entries not shown");
    assert_eq!(earlier(2, true), "2 earlier entries not kept: the newest are read");
}

#[test]
fn the_readers_span_is_one_day_said_once_or_two_clocks() {
    let same = reader_meta(&reader(Some("2026-10-21T13:52:00Z"), Some("2026-10-21T15:03:00Z")), &clock());
    assert_eq!(same.span.as_deref(), Some("13:52\u{2013}15:03 today"));
    let two = reader_meta(&reader(Some("2026-10-20T13:52:00Z"), Some("2026-10-21T15:03:00Z")), &clock());
    assert_eq!(two.span.as_deref(), Some("13:52 yesterday \u{2013} 15:03 today"));
    let end_only = reader_meta(&reader(None, Some("2026-10-21T15:03:00Z")), &clock());
    assert_eq!(end_only.span.as_deref(), Some("15:03 today"));
    let start_only = reader_meta(&reader(Some("2026-10-21T13:52:00Z"), None), &clock());
    assert_eq!(start_only.span.as_deref(), Some("13:52 today"));
    let bad_start = reader_meta(&reader(Some("nonsense"), Some("2026-10-21T15:03:00Z")), &clock());
    assert_eq!(bad_start.span, None, "an unreadable start hides the end too");
    assert_eq!(reader_meta(&reader(None, None), &clock()).span, None);
    assert_eq!((same.repository.as_str(), same.checkout.as_str(), same.turns.as_deref()), ("argus", "main", Some("1 turn")));
}

#[test]
fn the_find_count_is_nothing_no_match_or_the_place_among_the_matches() {
    let mut r = reader(None, None);
    assert_eq!(find_count(&r), None);
    r.find = "  ".into();
    assert_eq!(find_count(&r), None, "a blank find matches nothing and says nothing");
    r.find = "brief".into();
    assert_eq!(find_count(&r).as_deref(), Some("no match"));
    r.matches = vec![2, 5, 9];
    r.found = Some(5);
    assert_eq!(find_count(&r).as_deref(), Some("2 of 3"));
    r.found = None;
    assert_eq!(find_count(&r).as_deref(), Some("0 of 3"));
}

#[test]
fn a_tool_is_told_with_its_verb_whatever_its_case_and_an_unknown_one_keeps_its_name() {
    assert_eq!(tool_verb("Read"), Some((ToolIcon::Read, "Read")));
    assert_eq!(tool_verb("VIEW"), Some((ToolIcon::Read, "Read")));
    assert_eq!(tool_verb("Grep"), Some((ToolIcon::Search, "Searched for")));
    assert_eq!(tool_verb("write"), Some((ToolIcon::Edit, "Wrote")));
    assert_eq!(tool_verb("apply_patch"), Some((ToolIcon::Edit, "Edited")));
    assert_eq!(tool_verb("exec_command"), Some((ToolIcon::Run, "Ran")));
    assert_eq!(tool_verb("WebFetch"), None);
}
