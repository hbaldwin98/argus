//! The nav's tail: the Extensions and Views folds, and what the built-in items keep.

use argus_desktop_core::extensions::views::{ExtTab, ExtensionsPage};
use argus_desktop_core::layout::docks::{DockPage, DocksPage, Panel};
use argus_desktop_core::views::saved::{PageSaved, PageScope, SavedPanel, SavedViewsPage};
use argus_protocol::AgentView;

use super::*;

fn tab(extension: &str, view: &str, on: bool) -> ExtTab {
    ExtTab {
        panel: Panel::Extension(format!("{extension}/{view}")),
        extension: extension.into(),
        view: view.into(),
        title: format!("{view} view"),
        icon: Some("rocket".into()),
        page: false,
        on,
    }
}

fn extensions(listed: bool) -> Event {
    Event::Extensions(Box::new(ExtensionsPage { views: vec![tab("docker", "containers", true), tab("ci", "runs", false)], listed, ..ExtensionsPage::default() }))
}

fn saved(id: &str, scope: &str, here: bool, under: Option<&str>, stale: bool) -> PageSaved {
    PageSaved {
        id: id.into(),
        panel: Panel::Saved(id.into()),
        scope: PageScope { rank: 1, label: scope.into() },
        here,
        features: Vec::new(),
        recipe: None,
        sources: Vec::new(),
        built_at: 0,
        built_commit: None,
        stale,
        view: AgentView { id: id.into(), title: format!("{id} title"), ..Default::default() },
        diagrams: Vec::new(),
        refreshing: false,
        pressed: Vec::new(),
        under: under.map(str::to_owned),
    }
}

fn views(panels: Vec<SavedPanel>) -> Event {
    Event::SavedViews(Box::new(SavedViewsPage {
        views: vec![
            saved("plan", "argus", true, None, false),
            saved("detail", "argus", true, Some("plan"), true),
            saved("world", "Global", false, None, false),
        ],
        panels,
        can_press: true,
    }))
}

/// The busy rail, connected, and what else was heard.
fn with(mut more: Vec<Event>) -> Vec<Event> {
    let mut events = vec![status(Connection::Connected), Event::Rail(Box::new(busy()))];
    events.append(&mut more);
    events
}

#[gpui_kit::test]
fn the_folds_draw_nothing_while_they_hold_nothing(cx: &mut TestAppContext) {
    // SHELL-STAGE-18.
    let opened = open(cx, connected_and_busy());
    assert!(!present(cx, &opened, "nav-extensions"));
    assert!(!present(cx, &opened, "nav-views"));
}

#[gpui_kit::test]
fn the_extensions_fold_counts_its_views_and_asks_rust_to_open_it(cx: &mut TestAppContext) {
    let opened = open(cx, with(vec![extensions(false)]));
    assert_eq!(label(cx, &opened, "nav-extensions").as_deref(), Some("Extensions, 2"));
    assert!(!present(cx, &opened, "ext-view-docker-containers"), "folded, the views are not drawn");
    click(cx, &opened, "nav-extensions");
    assert_eq!(taken(&opened), ["ToggleExtensions"], "the fold is Rust's, drawn when it says");
    hear(cx, &opened, vec![extensions(true)]);
    assert!(present(cx, &opened, "ext-view-docker-containers") && present(cx, &opened, "ext-view-ci-runs"));
}

#[gpui_kit::test]
fn an_extension_view_is_lit_while_on_screen_and_opens_where_it_lives(cx: &mut TestAppContext) {
    let opened = open(cx, with(vec![extensions(true)]));
    assert_eq!(selected(cx, &opened, "ext-view-docker-containers"), Some(true));
    assert_eq!(selected(cx, &opened, "ext-view-ci-runs"), Some(false));
    assert_eq!(label(cx, &opened, "ext-view-ci-runs").as_deref(), Some("runs view \u{b7} from ci, docked"));
    click(cx, &opened, "ext-view-ci-runs");
    assert_eq!(taken(&opened), ["OpenExtensionView { extension: \"ci\", view: \"runs\" }"]);
}

#[gpui_kit::test]
fn the_views_fold_is_the_windows_own_and_lists_what_is_about_here_first(cx: &mut TestAppContext) {
    // CHECKOUTS-CODE-VIEWS-SAVED-13..16.
    let opened = open(cx, with(vec![views(Vec::new())]));
    assert_eq!(label(cx, &opened, "nav-views").as_deref(), Some("Views, 3"));
    assert!(!present(cx, &opened, "saved-plan"), "folded");
    click(cx, &opened, "nav-views");
    assert!(taken(&opened).is_empty(), "no call: the fold is the window's");
    assert!(present(cx, &opened, "saved-plan"));
    assert!(!present(cx, &opened, "saved-world"), "Here keeps what is about here");
    click(cx, &opened, "views-all");
    assert!(present(cx, &opened, "saved-world"));
    click(cx, &opened, "views-here");
    assert!(!present(cx, &opened, "saved-world"));
}

#[gpui_kit::test]
fn a_saved_view_opens_and_the_views_under_it_are_behind_a_twist(cx: &mut TestAppContext) {
    let opened = open(cx, with(vec![views(Vec::new())]));
    click(cx, &opened, "nav-views");
    click(cx, &opened, "saved-plan");
    assert_eq!(taken(&opened), ["OpenSavedView { id: \"plan\", from: None }"]);
    assert!(!present(cx, &opened, "saved-detail"), "the linked view is folded under its parent");
    assert_eq!(label(cx, &opened, "saved-plan-twist").as_deref(), Some("Show the 1 view listed under it"));
    click(cx, &opened, "saved-plan-twist");
    assert!(present(cx, &opened, "saved-detail"));
    assert_eq!(label(cx, &opened, "saved-plan-twist").as_deref(), Some("Hide the 1 view listed under it"));
    assert!(present(cx, &opened, "saved-detail-stale"), "a stale view says so");
    assert!(!present(cx, &opened, "saved-plan-stale"));
}

#[gpui_kit::test]
fn a_saved_view_is_lit_while_a_panel_showing_it_is_in_a_dock(cx: &mut TestAppContext) {
    let panels = vec![SavedPanel { panel: Panel::Saved("plan".into()), columns: vec!["plan".into()] }];
    let docks = DocksPage { right: DockPage { shown: Some(Panel::Saved("plan".into())), ..DockPage::default() }, ..DocksPage::default() };
    let opened = open(cx, with(vec![views(panels), Event::Docks(Box::new(docks))]));
    click(cx, &opened, "nav-views");
    assert_eq!(selected(cx, &opened, "saved-plan"), Some(true));
    hear(cx, &opened, vec![Event::Docks(Box::default())]);
    assert_eq!(selected(cx, &opened, "saved-plan"), Some(false), "the panel is gone from its dock");
}

#[gpui_kit::test]
fn a_fold_whose_filter_leaves_nothing_draws_its_list_without_rows(cx: &mut TestAppContext) {
    let only_elsewhere = Event::SavedViews(Box::new(SavedViewsPage { views: vec![saved("world", "Global", false, None, false)], panels: Vec::new(), can_press: true }));
    let opened = open(cx, with(vec![only_elsewhere]));
    click(cx, &opened, "nav-views");
    assert!(!present(cx, &opened, "saved-world"));
    assert_eq!(label(cx, &opened, "nav-empty").as_deref(), Some("None about here"));
}

#[gpui_kit::test]
fn the_lists_come_under_the_chips_extensions_first_and_a_held_rail_sends_nothing_from_them(cx: &mut TestAppContext) {
    let opened = open(cx, with(vec![extensions(true), views(Vec::new())]));
    let top = |cx: &mut TestAppContext, id: &str| cx.update_window(opened.handle, |_, window, _| window.find(id.to_owned()).bounds().origin.y.as_f32()).unwrap();
    let chips = top(cx, "rail-chips");
    click(cx, &opened, "nav-views");
    assert!(top(cx, "nav-extensions-head") > chips);
    assert!(top(cx, "nav-views-head") > top(cx, "nav-extensions-head"));
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    click(cx, &opened, "nav-extensions");
    click(cx, &opened, "ext-view-ci-runs");
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}

fn docks_with_present(panel: Panel, stage: bool) -> Event {
    use argus_desktop_core::layout::docks::{PanelKind, PresentPanel};
    let docks = DocksPage { present: vec![PresentPanel { panel, kind: PanelKind::Extension, floats: false, stage, closes: false, cell: false }], ..DocksPage::default() };
    Event::Docks(Box::new(docks))
}

#[gpui_kit::test]
fn an_extension_row_is_a_handle_that_docks_its_view_and_a_press_that_never_travels_is_its_click(cx: &mut TestAppContext) {
    // SHELL-PDRAG-6: the payload is the panel; a press that goes the threshold is the work area's drag.
    let panel = Panel::Extension("ci/runs".into());
    let opened = open_shell(cx);
    hear(cx, &opened, vec![extensions(true), docks_with_present(panel, true)]);
    forget(&opened);
    // No travel: the row's own click, and no drag.
    click(cx, &opened, "ext-view-ci-runs");
    assert_eq!(taken(&opened), ["OpenExtensionView { extension: \"ci\", view: \"runs\" }"]);
    assert!(!present(cx, &opened, "drop-preview"));
    // Carried over the stage it lands as the page, which Rust's `PresentPanel` says it can.
    let from = cx.update_window(opened.handle, |_, window, _| window.find("ext-view-ci-runs").bounds().center()).unwrap();
    cx.update_window(opened.handle, |_, window, cx| {
        window.pointer_down(from, MouseButton::Left, cx);
        window.pointer_move(point(from.x + px(30.0), from.y), Some(MouseButton::Left), cx);
        window.pointer_move(point(px(900.0), px(300.0)), Some(MouseButton::Left), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert!(present(cx, &opened, "drop-preview"), "the work area is carrying the panel");
}
