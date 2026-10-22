//! The saved panel in the work area, as a dock draws it: columns, headers, the buttons that go to the
//! daemon, the questions kept across a redraw. (The look is photographed: scene `saved-view`.)

use argus_desktop_core::connection::page::{Connection, Event};
use argus_desktop_core::layout::docks::{DockPage, DocksPage, FloatBounds, FloatPage, Panel, PanelKind, PresentPanel};
use argus_desktop_core::panes::console::ConsolePlace;
use argus_desktop_core::views::saved::{PageSaved, PageScope, SavedPanel as Trail, SavedViewsPage};
use argus_protocol::AgentView;
use gpui_kit::TestAppContext;
use serde_json::json;

use super::super::delete;
use super::*;
use crate::layout::work::tests::{bounds, console, docks, hear, label, open, present, present_id, status, tab, taken, Opened};

fn saved(id: &str, title: &str, stale: bool) -> PageSaved {
    PageSaved {
        id: id.into(),
        panel: Panel::Saved(id.into()),
        scope: PageScope { rank: 0, label: "Global".into() },
        here: true,
        features: vec![],
        recipe: Some("Read the board".into()),
        sources: vec![],
        built_at: 0,
        built_commit: Some("6ce099a1234567".into()),
        stale,
        view: AgentView {
            id: id.into(),
            title: title.into(),
            blocks: serde_json::from_value(json!([{"kind": "callout", "tone": "info", "text": "from inside"}])).unwrap(),
            ..Default::default()
        },
        diagrams: vec![],
        refreshing: false,
        pressed: vec![],
        under: None,
    }
}

fn trail(panel: &str, columns: &[&str]) -> Trail {
    Trail { panel: Panel::Saved(panel.into()), columns: columns.iter().map(|id| (*id).to_owned()).collect() }
}

fn page(views: Vec<PageSaved>, trails: Vec<Trail>) -> Event {
    Event::SavedViews(Box::new(SavedViewsPage { views, panels: trails, can_press: true }))
}

/// The right dock holding `names` as tabs, the first shown.
fn with_tabs(names: &[&str]) -> DocksPage {
    let mut page = docks();
    let panels: Vec<Panel> = names.iter().map(|name| Panel::Saved((*name).into())).collect();
    page.right = DockPage { tabs: panels.iter().map(|panel| tab(panel, &panel.to_string())).collect(), shown: panels.first().cloned(), size: 400 };
    page.present = vec![present(&Panel::Console, PanelKind::Console, true)];
    page.present.extend(panels.iter().map(|panel| present(panel, PanelKind::Saved, false)));
    page
}

fn opened(cx: &mut TestAppContext, docks: DocksPage, views: Vec<PageSaved>, trails: Vec<Trail>) -> Opened {
    open(cx, vec![status(Connection::Connected), console(ConsolePlace::Docked), Event::Docks(Box::new(docks)), page(views, trails)])
}

fn click(cx: &mut TestAppContext, opened: &Opened, id: &str) {
    use gpui_kit::test::TestWindowExt as _;
    cx.update_window(opened.handle, |_, window, cx| window.click(id.to_owned(), cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

#[test]
fn a_panel_grows_to_show_a_column_added_after_its_first_draw_only() {
    // SAVED-10.
    assert!(!grew(None, 2), "a panel's first draw keeps its left");
    assert!(grew(Some(1), 2));
    assert!(!grew(Some(2), 2));
    assert!(!grew(Some(3), 2), "a column closed scrolls nowhere");
}

#[gpui_kit::test]
fn a_trail_is_columns_side_by_side_each_under_its_own_header(cx: &mut TestAppContext) {
    // SAVED-1, -5.
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", false), saved("b", "Detail", false)], vec![trail("a", &["a", "b"])]);
    let (first, second) = (bounds(cx, &opened, "saved-col-saved:a-0"), bounds(cx, &opened, "saved-col-saved:a-1"));
    assert!(second.origin.x >= first.origin.x + first.size.width - gpui_kit::px(1.0), "side by side");
    assert_eq!(label(cx, &opened, "saved-col-saved:a-1").as_deref(), Some("Detail"));
    assert!(present_id(cx, &opened, "saved-head-saved:a"), "several columns have a head of the panel's own");
    assert!(present_id(cx, &opened, "saved-title-saved:a-1-float"), "a column after the first can be a dialog");
    assert!(present_id(cx, &opened, "saved-title-saved:a-1-pop"));
    assert!(!present_id(cx, &opened, "saved-title-saved:a-0-pop"), "the first column is never popped out");
}

#[gpui_kit::test]
fn a_lone_view_in_a_dock_with_tabs_is_named_by_its_tab_and_draws_no_head(cx: &mut TestAppContext) {
    // SAVED-4.
    let opened = opened(cx, with_tabs(&["a", "b"]), vec![saved("a", "Plan", false), saved("b", "Other", false)], vec![trail("a", &["a"]), trail("b", &["b"])]);
    assert!(!present_id(cx, &opened, "saved-head-saved:a"));
    assert!(!present_id(cx, &opened, "saved-title-saved:a-0"), "a lone view has no title row");
    assert!(present_id(cx, &opened, "saved-meta-row-saved:a-0"));
}

#[gpui_kit::test]
fn a_lone_view_in_a_dock_of_its_own_draws_the_head_and_closes_the_panel_with_it(cx: &mut TestAppContext) {
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", true)], vec![trail("a", &["a"])]);
    assert!(present_id(cx, &opened, "saved-head-saved:a"));
    click(cx, &opened, "saved-close-saved:a");
    assert_eq!(taken(&opened), ["CloseSavedColumn { panel: Saved(\"a\"), column: 0 }"]);
}

#[gpui_kit::test]
fn a_floating_lone_view_has_no_head_beside_its_frames_title_bar(cx: &mut TestAppContext) {
    let mut page = with_tabs(&["z"]);
    let float = Panel::Saved("a".into());
    page.floating.push(FloatPage { panel: float.clone(), title: "Plan".into(), bounds: FloatBounds { x: 40, y: 40, width: 560, height: 400 } });
    page.present.push(PresentPanel { panel: float, kind: PanelKind::Saved, floats: true, stage: false, closes: true });
    let opened = opened(cx, page, vec![saved("a", "Plan", false), saved("z", "Zed", false)], vec![trail("a", &["a"]), trail("z", &["z"])]);
    assert!(present_id(cx, &opened, "saved-meta-row-saved:a-0"), "the same element, drawn in the frame");
    assert!(!present_id(cx, &opened, "saved-head-saved:a"));
}

#[gpui_kit::test]
fn the_buttons_ask_the_daemon_and_the_column_changes_only_when_it_answers(cx: &mut TestAppContext) {
    // SAVED-6.
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", false)], vec![trail("a", &["a"])]);
    click(cx, &opened, "saved-meta-row-saved:a-0-refresh");
    click(cx, &opened, "saved-meta-row-saved:a-0-stale");
    assert_eq!(taken(&opened), ["RefreshSavedView { id: \"a\" }", "MarkSavedViewStale { id: \"a\" }"]);
}

#[gpui_kit::test]
fn refresh_waits_while_one_is_pending_and_mark_stale_while_it_is_stale(cx: &mut TestAppContext) {
    let mut view = saved("a", "Plan", true);
    view.refreshing = true;
    let opened = opened(cx, with_tabs(&["a"]), vec![view], vec![trail("a", &["a"])]);
    click(cx, &opened, "saved-meta-row-saved:a-0-refresh");
    click(cx, &opened, "saved-meta-row-saved:a-0-stale");
    assert!(taken(&opened).is_empty(), "both are disabled");
}

#[gpui_kit::test]
fn delete_asks_first_and_the_question_survives_the_view_being_saved_again(cx: &mut TestAppContext) {
    // SAVED-7.
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", false)], vec![trail("a", &["a"])]);
    opened.heard.update(cx, |heard, _| heard.memory.borrow_mut().set_asking(&delete::key("saved:a", 0, "a"), true));
    hear(cx, &opened, vec![page(vec![saved("a", "Plan", false)], vec![trail("a", &["a"])])]);
    assert!(present_id(cx, &opened, "saved-meta-row-saved:a-0-ask"), "kept across the republish");
    click(cx, &opened, "saved-meta-row-saved:a-0-no");
    assert!(!present_id(cx, &opened, "saved-meta-row-saved:a-0-ask"));
    assert!(taken(&opened).is_empty(), "No sends nothing");
    opened.heard.update(cx, |heard, _| heard.memory.borrow_mut().set_asking(&delete::key("saved:a", 0, "a"), true));
    hear(cx, &opened, vec![]);
    click(cx, &opened, "saved-meta-row-saved:a-0-yes");
    assert_eq!(taken(&opened), ["DeleteSavedView { id: \"a\" }"]);
}

#[gpui_kit::test]
fn details_unfold_under_the_header_and_stay_unfolded_when_the_view_is_saved_again(cx: &mut TestAppContext) {
    // SAVED-8.
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", false)], vec![trail("a", &["a"])]);
    assert!(!present_id(cx, &opened, "saved-meta-a"));
    click(cx, &opened, "saved-meta-row-saved:a-0-details");
    assert!(present_id(cx, &opened, "saved-meta-a"));
    assert!(present_id(cx, &opened, "saved-meta-Recipe"));
    hear(cx, &opened, vec![page(vec![saved("a", "Plan", false)], vec![trail("a", &["a"])])]);
    assert!(present_id(cx, &opened, "saved-meta-a"));
    click(cx, &opened, "saved-meta-row-saved:a-0-details");
    assert!(!present_id(cx, &opened, "saved-meta-a"));
}

#[gpui_kit::test]
fn a_column_whose_view_is_not_held_says_so_with_its_id_and_closes(cx: &mut TestAppContext) {
    // SAVED-2.
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", false)], vec![trail("a", &["gone"])]);
    assert_eq!(label(cx, &opened, "saved-col-saved:a-0").as_deref(), Some("View not found"));
    click(cx, &opened, "saved-close-saved:a-0");
    assert_eq!(taken(&opened), ["CloseSavedColumn { panel: Saved(\"a\"), column: 0 }"]);
}

#[gpui_kit::test]
fn the_panel_is_one_element_whether_it_is_docked_or_floating(cx: &mut TestAppContext) {
    // SAVED-12.
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", false)], vec![trail("a", &["a"])]);
    let asked = |cx: &mut TestAppContext, panel: Panel| {
        cx.update(|cx| crate::views::saved::registry::element(&panel, &opened.heard, &Caller::new(|_| {}), cx).map(|element| element.entity_id()))
    };
    let (first, again) = (asked(cx, Panel::Saved("a".into())), asked(cx, Panel::Saved("a".into())));
    assert!(first.is_some());
    assert_eq!(first, again);
    assert_ne!(first, asked(cx, Panel::Saved("b".into())));
    assert!(asked(cx, Panel::Views).is_none());
}

fn wheel_over(cx: &mut TestAppContext, opened: &Opened, id: &str, dy: f32) {
    use gpui_kit::test::TestWindowExt as _;
    let at = bounds(cx, opened, id).center();
    cx.update_window(opened.handle, |_, window, cx| {
        let wheel = gpui_kit::ScrollWheelEvent {
            position: at,
            delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(dy))),
            ..Default::default()
        };
        window.dispatch_event(gpui_kit::InputEvent::to_platform_input(wheel), cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_vertical_wheel_over_a_column_never_moves_the_columns_sideways(cx: &mut TestAppContext) {
    // SAVED-1: the page's `.saved-cols` is overflow-y hidden, so only a column scrolls down.
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", false), saved("b", "Detail", false)], vec![trail("a", &["a", "b"])]);
    let before = bounds(cx, &opened, "saved-col-saved:a-0");
    assert!(bounds(cx, &opened, "saved-col-saved:a-1").right() > bounds(cx, &opened, "saved-cols-saved:a").right(), "the columns overflow the panel");
    wheel_over(cx, &opened, "saved-col-saved:a-0-body", -60.0);
    assert_eq!(bounds(cx, &opened, "saved-col-saved:a-0").origin.x, before.origin.x);
}

#[gpui_kit::test]
fn a_lone_saved_panel_in_a_dock_of_its_own_has_its_own_head_and_not_the_docks(cx: &mut TestAppContext) {
    // SAVED-4: one head, the panel's.
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", false)], vec![trail("a", &["a"])]);
    assert!(present_id(cx, &opened, "saved-head-saved:a"));
    assert!(!present_id(cx, &opened, "dock-right-head"));
}

#[gpui_kit::test]
fn a_floating_frames_bar_wears_the_stale_chip_and_the_column_count(cx: &mut TestAppContext) {
    // SAVED-4: a floating panel's head is its frame's title bar.
    let mut page = with_tabs(&["z"]);
    for name in ["a", "t"] {
        let float = Panel::Saved(name.into());
        page.floating.push(FloatPage { panel: float.clone(), title: name.into(), bounds: FloatBounds { x: 40, y: 40, width: 560, height: 400 } });
        page.present.push(PresentPanel { panel: float, kind: PanelKind::Saved, floats: true, stage: false, closes: true });
    }
    let views = vec![saved("a", "Plan", true), saved("t", "Trail", false), saved("b", "Detail", false), saved("z", "Zed", false)];
    let opened = opened(cx, page, views, vec![trail("a", &["a"]), trail("t", &["t", "b"]), trail("z", &["z"])]);
    assert_eq!(label(cx, &opened, "saved-stale-saved:a").as_deref(), Some("stale"));
    assert!(!present_id(cx, &opened, "saved-count-saved:a"));
    assert_eq!(label(cx, &opened, "saved-count-saved:t").as_deref(), Some("2 columns"));
    assert!(!present_id(cx, &opened, "saved-stale-saved:t"));
}

#[gpui_kit::test]
fn a_panel_closed_and_opened_again_is_a_fresh_element(cx: &mut TestAppContext) {
    // The page's `drawSavedPanels` deletes the element of a panel no longer listed.
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", false)], vec![trail("a", &["a"])]);
    let asked = |cx: &mut TestAppContext| {
        cx.update(|cx| crate::views::saved::registry::element(&Panel::Saved("a".into()), &opened.heard, &Caller::new(|_| {}), cx).map(|element| element.entity_id()))
    };
    let first = asked(cx);
    hear(cx, &opened, vec![page(vec![saved("a", "Plan", false)], vec![])]);
    hear(cx, &opened, vec![page(vec![saved("a", "Plan", false)], vec![trail("a", &["a"])])]);
    assert_ne!(first, asked(cx));
}

#[gpui_kit::test]
fn built_ago_is_said_again_on_the_windows_half_minute_tick(cx: &mut TestAppContext) {
    // SAVED-4: `built <ago>` is told in minutes and must not go stale between the daemon's events.
    let opened = opened(cx, with_tabs(&["a"]), vec![saved("a", "Plan", false)], vec![trail("a", &["a"])]);
    let element = cx.update(|cx| super::super::registry::element(&Panel::Saved("a".into()), &opened.heard, &crate::connection::caller::Caller::new(|_| {}), cx)).unwrap();
    // Counted at the notify itself: a forced frame would redraw the root whether or not the timer asked.
    let notified = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let count = notified.clone();
    let _watching = cx.update(|cx| cx.observe(&element, move |_, _| count.set(count.get() + 1)));
    cx.executor().advance_clock(crate::views::age::EVERY / 2);
    cx.run_until_parked();
    assert_eq!(notified.get(), 0, "not before the half minute");
    cx.executor().advance_clock(crate::views::age::EVERY);
    cx.run_until_parked();
    assert!(notified.get() >= 1, "the timer asks for a new draw");
}
