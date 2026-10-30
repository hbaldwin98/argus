use argus_desktop_core::connection::page::{
    Checkout, Connection, Event, Project, Rail, Repository, RunningPane, Shown, Stage, Status,
};
use argus_desktop_core::chrome::add_page::AddProjectPage;
use argus_desktop_core::layout::docks::{DockPage, DockTab, DocksPage, Panel};
use argus_desktop_core::panes::console::{ConsolePage, ConsolePlace};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{px, size, TestAppContext};

use crate::keys::root::tests::{asked, open, overlays, press, Opened};
use crate::kit::overlay::kind::Overlay;

fn status(connection: Connection, host: Option<&str>, detail: Option<&str>) -> Event {
    Event::Status(Status { connection, detail: detail.map(str::to_owned), alarm: None, chime: true, host: host.map(str::to_owned) })
}

fn checkout(branch: &str) -> Checkout {
    Checkout {
        id: 110,
        name: "main".into(),
        branch: branch.into(),
        kind: "main",
        git: None,
        clean: true,
        shared: None,
        open: true,
        count: 0,
        status: None,
        panes: Vec::new(),
    }
}

fn running(id: u64, title: &str) -> RunningPane {
    RunningPane { id, agent: true, title: title.into(), status: "working", note: None, repository: "argus".into(), checkout: "dev".into(), used: Some("3m".into()) }
}

/// A project with one repository, chosen, whose first checkout is on `dev`, and two panes to go back to.
fn project_rail() -> Rail {
    let repository = Repository {
        id: 11,
        name: "argus".into(),
        quiet: false,
        divider: false,
        open: true,
        panes: 0,
        status: None,
        used: None,
        checkouts: vec![checkout("dev")],
    };
    Rail {
        loaded: true,
        repository: Some(11),
        project: Some(Project { id: 1, name: "src".into(), needs_you: 0, repositories: vec![repository], running: vec![running(7, "Trim context"), running(8, "Tracker")] }),
        ..Rail::default()
    }
}

fn shown() -> Shown {
    Shown {
        pane: 7,
        repository: "argus".into(),
        checkout: "main".into(),
        branch: "dev".into(),
        agent: true,
        browser: false,
        title: "Trim context".into(),
        status: "working",
        resumed: None,
    }
}

fn hear(cx: &mut TestAppContext, opened: &Opened, events: Vec<Event>) {
    opened.events.send(events).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn has(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> bool {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).is_some()).unwrap()
}

fn click(cx: &mut TestAppContext, opened: &Opened, id: impl Into<gpui_kit::ElementId>) {
    let id = id.into();
    cx.update_window(opened.handle, |_, window, cx| window.click(id, cx)).unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn waiting_on_the_daemon_offers_the_settings_for_this_machine(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![crate::keys::root::tests::settings_event()]);
    hear(cx, &opened, vec![status(Connection::Connecting, None, Some("spawn failed"))]);
    assert!(has(cx, &opened, "empty"));
    assert!(has(cx, &opened, "connecting-detail"), "the last failed try's words are shown");
    assert!(!has(cx, &opened, "connecting-local"));
    click(cx, &opened, "connecting-settings");
    assert_eq!(overlays(cx), [Overlay::Settings]);
}

#[gpui_kit::test]
fn waiting_on_a_remote_daemon_offers_the_way_back_to_this_machine(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![status(Connection::Connecting, Some("build-box"), None)]);
    assert!(!has(cx, &opened, "connecting-settings"));
    assert!(!has(cx, &opened, "connecting-detail"));
    click(cx, &opened, "connecting-local");
    assert_eq!(asked(&opened), ["SetHost { host: None }"]);
}

#[gpui_kit::test]
fn a_tree_with_no_project_offers_to_add_one_by_asking_the_session_for_the_dialog(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(Rail { loaded: true, ..Rail::default() }))]);
    assert!(!has(cx, &opened, "connecting-settings"));
    click(cx, &opened, "add-project");
    assert_eq!(asked(&opened), ["OpenAddProject"]);
    assert!(overlays(cx).is_empty(), "the dialog comes when the session says it is open, not on the press");
    hear(cx, &opened, vec![Event::AddProject(Box::new(AddProjectPage { open: true, ..AddProjectPage::default() }))]);
    assert_eq!(overlays(cx), [Overlay::AddProject]);
}

#[gpui_kit::test]
fn a_workspace_with_no_project_says_that_a_project_added_now_lands_in_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(Rail { loaded: true, ..Rail::default() }))]);
    assert_eq!(
        label_of(cx, &opened, "no-projects").as_deref(),
        Some("No projects here yet. Projects you add now land in this workspace."),
    );
    assert!(has(cx, &opened, "add-project"), "and the button to add one is there");
}

#[gpui_kit::test]
fn a_project_with_no_pane_offers_an_agent_a_shell_and_the_panes_to_go_back_to(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(project_rail()))]);
    assert!(has(cx, &opened, "empty-where"));
    click(cx, &opened, "start-agent");
    click(cx, &opened, "open-shell");
    click(cx, &opened, ("running", 8u64));
    assert_eq!(asked(&opened), ["ShowComposer { open: true }", "SpawnShell { checkout: 110 }", "Select { pane: 8 }"]);
}

#[gpui_kit::test]
fn a_project_whose_panes_are_all_closed_offers_no_strip_of_them(cx: &mut TestAppContext) {
    let opened = open(cx);
    let mut rail = project_rail();
    rail.project.as_mut().unwrap().running.clear();
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(rail))]);
    assert!(has(cx, &opened, "start-agent"));
    assert!(!has(cx, &opened, "running"));
}

#[gpui_kit::test]
fn a_pane_on_screen_leaves_no_empty_stage(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(Rail { shown: Some(shown()), ..project_rail() }))]);
    assert!(!has(cx, &opened, "empty"));
    hear(cx, &opened, vec![Event::Rail(Box::new(project_rail()))]);
    assert!(has(cx, &opened, "empty"), "the stage is empty again when the daemon says the pane is gone");
}

#[gpui_kit::test]
fn nothing_the_empty_stage_sends_reaches_a_daemon_that_is_gone(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(project_rail()))]);
    hear(cx, &opened, vec![status(Connection::Connecting, None, None)]);
    click(cx, &opened, "start-agent");
    click(cx, &opened, ("running", 8u64));
    assert!(asked(&opened).is_empty(), "{:?}", asked(&opened));
}

#[gpui_kit::test]
fn the_stopped_card_is_up_while_stopped_and_reconnect_asks_without_closing_it(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(project_rail()))]);
    assert_eq!(overlays(cx), []);
    hear(cx, &opened, vec![status(Connection::Stopped, None, None)]);
    assert_eq!(overlays(cx), [Overlay::Stopped]);
    // Neither Escape nor the press that asked dismisses it: the daemon's word does.
    press(cx, &opened, "escape");
    assert_eq!(overlays(cx), [Overlay::Stopped]);
    // The card slides in over 0.25s of wall-clock time (gpui-component's dialog); a press whose
    // down and up straddle a frame mid-slide lands on two places and is no click.
    std::thread::sleep(std::time::Duration::from_millis(300));
    click(cx, &opened, "reconnect");
    assert_eq!(asked(&opened), ["Reconnect"]);
    assert_eq!(overlays(cx), [Overlay::Stopped]);
    hear(cx, &opened, vec![status(Connection::Connected, None, None)]);
    assert_eq!(overlays(cx), []);
}

#[gpui_kit::test]
fn the_stopped_card_is_not_up_while_reconnecting(cx: &mut TestAppContext) {
    let opened = open(cx);
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(project_rail()))]);
    hear(cx, &opened, vec![status(Connection::Connecting, None, None)]);
    assert_eq!(overlays(cx), []);
}

#[gpui_kit::test]
fn filling_the_window_puts_the_sidebar_away_and_the_strip_stays(cx: &mut TestAppContext) {
    let opened = open(cx);
    let rail = |filled| Event::Rail(Box::new(Rail { shown: Some(shown()), filled, ..project_rail() }));
    hear(cx, &opened, vec![status(Connection::Connected, None, None), rail(false)]);
    assert!(has(cx, &opened, "rail") && has(cx, &opened, "header"));
    hear(cx, &opened, vec![rail(true)]);
    assert!(!has(cx, &opened, "rail"));
    assert!(has(cx, &opened, "header"), "the strip stays: it holds the window's buttons");
}

#[gpui_kit::test]
fn a_fullscreen_console_puts_the_nav_the_foot_and_the_docks_away_and_keeps_the_sidebar(cx: &mut TestAppContext) {
    let opened = open(cx);
    let panel = Panel::Views;
    let docks = DocksPage { right: DockPage { tabs: vec![DockTab { panel: panel.clone(), title: "Views".into() }], shown: Some(panel), size: 400 }, ..DocksPage::default() };
    let console = |place| Event::Console(Box::new(ConsolePage { place, ..ConsolePage::default() }));
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(Rail { shown: Some(shown()), ..project_rail() })), Event::Docks(Box::new(docks)), console(ConsolePlace::Docked)]);
    assert!(has(cx, &opened, "nav-search") && has(cx, &opened, "foot-status") && has(cx, &opened, "dock-right"));
    hear(cx, &opened, vec![console(ConsolePlace::Fullscreen)]);
    assert!(has(cx, &opened, "rail"), "the sidebar's projects stay");
    for gone in ["nav-search", "foot-status", "dock-right"] {
        assert!(!has(cx, &opened, gone), "{gone}");
    }
    hear(cx, &opened, vec![console(ConsolePlace::Tab)]);
    assert!(has(cx, &opened, "nav-search") && has(cx, &opened, "foot-status"), "the Console stage keeps the nav");
}

#[gpui_kit::test]
fn a_stage_with_no_surface_yet_says_so_in_place_of_the_pane(cx: &mut TestAppContext) {
    let opened = open(cx);
    let rail = |stage| Event::Rail(Box::new(Rail { shown: Some(shown()), stage, ..project_rail() }));
    hear(cx, &opened, vec![status(Connection::Connected, None, None), rail(Stage::Checkouts)]);
    assert!(!has(cx, &opened, "split") && !has(cx, &opened, "empty"), "neither the pane nor the empty stage is drawn");
    hear(cx, &opened, vec![rail(Stage::Workspace)]);
    assert!(has(cx, &opened, "split"));
}

#[gpui_kit::test]
fn the_narrow_window_is_no_reason_to_lose_the_empty_stage(cx: &mut TestAppContext) {
    let opened = open(cx);
    cx.simulate_window_resize(opened.handle, size(px(720.0), px(480.0)));
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(project_rail()))]);
    assert!(has(cx, &opened, "start-agent"));
}

fn label_of(cx: &mut TestAppContext, opened: &Opened, id: &'static str) -> Option<String> {
    cx.update_window(opened.handle, |_, window, _| window.try_find(id.to_owned()).and_then(|e| e.label().map(str::to_owned))).unwrap()
}

#[gpui_kit::test]
fn the_strip_and_the_stage_body_are_held_while_stopped_or_reconnecting_and_not_before_a_tree(cx: &mut TestAppContext) {
    // SHELL-STAGE-14 and -20: dimmed with the rail, since they show what was last heard.
    let opened = open(cx);
    let held = |cx: &mut TestAppContext| (label_of(cx, &opened, "drag").unwrap(), label_of(cx, &opened, "stage-body").unwrap());
    hear(cx, &opened, vec![status(Connection::Connecting, None, None)]);
    assert_eq!(held(cx), ("Strip".into(), "Stage".into()), "before any tree there is nothing shown to dim");
    hear(cx, &opened, vec![status(Connection::Connected, None, None), Event::Rail(Box::new(project_rail()))]);
    assert_eq!(held(cx), ("Strip".into(), "Stage".into()));
    hear(cx, &opened, vec![status(Connection::Connecting, None, None)]);
    assert_eq!(held(cx), ("Strip, as last heard".into(), "Stage, as last heard".into()), "reconnecting after a tree");
    hear(cx, &opened, vec![status(Connection::Connected, None, None)]);
    assert_eq!(held(cx), ("Strip".into(), "Stage".into()));
    hear(cx, &opened, vec![status(Connection::Stopped, None, None)]);
    assert_eq!(held(cx), ("Strip, as last heard".into(), "Stage, as last heard".into()), "stopped");
}

#[gpui_kit::test]
fn the_cap_on_start_an_agent_is_in_the_buttons_ink_not_the_faint_one(cx: &mut TestAppContext) {
    let tokens = cx.update(|cx| {
        crate::window::init(cx);
        *crate::theme::Tokens::global(cx)
    });
    let caller = crate::connection::caller::Caller::new(|_| {});
    let ways = super::empty::Ways { caller: &caller, held: false, compose: Some(vec!["Ctrl".into(), "N".into()]) };
    let mut caps = super::empty::start_caps(&ways, &tokens).expect("a cap when the composer has a chord");
    assert_eq!(caps.style().text.color, Some(tokens.fg_on_accent));
    let bare = super::empty::Ways { compose: None, ..ways };
    assert!(super::empty::start_caps(&bare, &tokens).is_none());
}
