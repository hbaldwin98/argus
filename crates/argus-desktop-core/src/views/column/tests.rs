use super::*;
use crate::layout::docks::{DockPage, DockTab, FloatBounds, FloatPage};
use crate::views::saved::{PageFeature, PageScope, PageSource};
use argus_protocol::AgentView;
use jiff::tz::TimeZone;
use jiff::Timestamp;

fn clock() -> Clock {
    Clock::new("2026-10-21T15:30:00Z".parse::<Timestamp>().unwrap(), TimeZone::UTC)
}

fn view() -> PageSaved {
    PageSaved {
        id: "plan".into(),
        panel: Panel::Saved("plan".into()),
        scope: PageScope { rank: 1, label: "argus".into() },
        here: true,
        features: Vec::new(),
        recipe: None,
        sources: Vec::new(),
        built_at: "2026-10-21T15:00:00Z".parse::<Timestamp>().unwrap().as_second(),
        built_commit: Some("6ce099a1234567890".into()),
        stale: false,
        view: AgentView { id: "plan".into(), title: "plan".into(), ..Default::default() },
        diagrams: Vec::new(),
        refreshing: false,
        pressed: Vec::new(),
        under: None,
    }
}

#[test]
fn the_meta_row_says_how_long_ago_and_the_first_seven_of_the_commit() {
    // CHECKOUTS-CODE-VIEWS-SAVED-6.
    let built = built(&clock(), &view());
    assert_eq!(built.said, "built 30 min ago at 6ce099a");
    assert_eq!(built.exact, "Oct 21, 2026 15:00");
}

#[test]
fn a_pending_refresh_is_said_after_the_commit() {
    let mut saved = view();
    saved.refreshing = true;
    assert_eq!(built(&clock(), &saved).said, "built 30 min ago at 6ce099a \u{b7} asked to build it again");
}

#[test]
fn a_view_with_no_commit_says_no_commit() {
    let mut saved = view();
    saved.built_commit = None;
    assert_eq!(built(&clock(), &saved).said, "built 30 min ago");
    assert_eq!(details(&clock(), &saved).built, "Oct 21, 2026 15:00");
}

#[test]
fn details_cut_the_commit_at_twelve_and_leave_out_what_is_empty() {
    // SAVED-8.
    let mut saved = view();
    let plain = details(&clock(), &saved);
    assert_eq!(plain.built, "Oct 21, 2026 15:00 \u{b7} commit 6ce099a12345");
    assert_eq!((plain.features, plain.recipe, plain.sources.len()), (None, None, 0));
    saved.features = vec![PageFeature { repository: 1, number: 7 }, PageFeature { repository: 1, number: 9 }];
    saved.recipe = Some("Read the board".into());
    saved.sources = vec![PageSource { kind: "url", text: "https://x.test".into() }, PageSource { kind: "note", text: "n".into() }];
    let full = details(&clock(), &saved);
    assert_eq!(full.features.as_deref(), Some("#7, #9"));
    assert_eq!(full.recipe.as_deref(), Some("Read the board"));
    assert_eq!(full.sources.iter().map(|s| s.link).collect::<Vec<_>>(), [true, false]);
}

fn docks(tabs: usize, floating: bool) -> DocksPage {
    let panel = Panel::Saved("plan".into());
    let tab = |panel: Panel| DockTab { title: panel.to_string(), panel };
    let mut right = DockPage { tabs: vec![tab(panel.clone())], ..DockPage::default() };
    for n in 1..tabs {
        right.tabs.push(tab(Panel::Saved(format!("other{n}"))));
    }
    let mut page = DocksPage { right, ..DocksPage::default() };
    if floating {
        page.right.tabs.clear();
        page.floating.push(FloatPage { panel, title: "plan".into(), bounds: FloatBounds { x: 0, y: 0, width: 400, height: 300 } });
    }
    page
}

#[test]
fn a_lone_view_in_a_tabbed_dock_draws_no_head_of_its_own() {
    // SAVED-4: the tab names, closes and moves it.
    let panel = Panel::Saved("plan".into());
    assert!(!headed(1, &panel, &docks(2, false)));
}

#[test]
fn a_lone_view_in_a_dock_of_its_own_a_float_and_every_trail_of_columns_have_a_head() {
    // A float's head is its frame's title bar, which carries the stale chip.
    let panel = Panel::Saved("plan".into());
    assert!(headed(1, &panel, &docks(1, false)));
    assert!(headed(1, &panel, &docks(1, true)));
    assert!(headed(2, &panel, &docks(2, false)));
    assert!(headed(3, &panel, &docks(1, true)));
}

#[test]
fn a_panel_is_floating_when_the_docks_list_it_as_a_dialog() {
    let panel = Panel::Saved("plan".into());
    assert!(floating(&panel, &docks(1, true)));
    assert!(!floating(&panel, &docks(1, false)));
}
