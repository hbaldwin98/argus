//! What the rail lists of the tree it was given.

use super::*;

#[gpui_kit::test]
fn the_rail_lists_the_rows_of_the_rail_it_was_given(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    for nav in Nav::ALL {
        assert_eq!(label(cx, &opened, nav.id()).as_deref(), Some(nav.label()));
    }
    assert_eq!(label(cx, &opened, "repo-11").as_deref(), Some("argus"));
    assert_eq!(label(cx, &opened, "repo-11-count").as_deref(), Some("3"));
    // A quiet repository says how long since it was used, and draws nothing under it while shut.
    assert_eq!(label(cx, &opened, "repo-10-count").as_deref(), Some("16m"));
    assert!(!present(cx, &opened, "checkout-100"));
    assert!(!present(cx, &opened, "pane-6"));
    assert_eq!(label(cx, &opened, "checkout-110").as_deref(), Some("main checkout on main"));
    assert_eq!(label(cx, &opened, "pane-1").as_deref(), Some("Trim context · agent, working"));
    assert_eq!(label(cx, &opened, "pane-2").as_deref(), Some("zsh · shell, idle"));
    // Only what asks for a look is said in words.
    assert_eq!(label(cx, &opened, "pane-4-word").as_deref(), Some("review"));
    assert!(!present(cx, &opened, "pane-1-word"));
    assert!(!present(cx, &opened, "pane-2-word"));
    // A folded checkout draws no pane and speaks for them: how many, and who shares it.
    assert!(!present(cx, &opened, "pane-5"));
    assert_eq!(label(cx, &opened, "checkout-112-folded").as_deref(), Some("1 pane folded away"));
    assert_eq!(label(cx, &opened, "checkout-112-shared").as_deref(), Some("shared by 2 agents"));
}

#[gpui_kit::test]
fn the_rail_draws_no_list_of_projects_the_brand_row_is_where_they_are_chosen(cx: &mut TestAppContext) {
    // The page lists no project under its brand: a button opens them (`switcher.png`), once the tree has loaded.
    let opened = open(cx, connected_and_busy());
    assert!(!present(cx, &opened, "project-1"));
    assert!(!present(cx, &opened, "project-2"));
    assert!(present(cx, &opened, "rail-projects"), "two projects: the name is a button");
    let mut single = busy();
    single.projects.truncate(1);
    hear(cx, &opened, vec![Event::Rail(Box::new(single))]);
    assert!(present(cx, &opened, "rail-projects"), "one project: the menu is still there, since it is how another is added");
    hear(cx, &opened, vec![Event::Rail(Box::new(Rail { loaded: false, ..Rail::default() }))]);
    assert!(!present(cx, &opened, "rail-projects"), "until the tree has loaded the heading is empty");
}

#[gpui_kit::test]
fn the_brand_button_opens_the_projects_and_choosing_one_sends_the_pages_call(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    click(cx, &opened, "rail-projects");
    let at = cx.update_window(opened.handle, |_, window, _| window.find_by_label("other").bounds().center()).unwrap();
    press(cx, &opened, at, MouseButton::Left);
    assert_eq!(taken(&opened), ["SelectProject { project: 2 }"]);
    // The open project is ticked.
    click(cx, &opened, "rail-projects");
    let ticked = cx.update_window(opened.handle, |_, window, _| window.try_find_by_label("checked").is_some()).unwrap();
    assert!(ticked, "the open project's check is drawn");
}

#[gpui_kit::test]
fn the_pane_on_screen_and_the_chosen_repository_are_marked(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    assert_eq!(selected(cx, &opened, "pane-1"), Some(true));
    assert_eq!(selected(cx, &opened, "pane-2"), Some(false));
    assert_eq!(selected(cx, &opened, "repo-11"), Some(true));
    assert_eq!(selected(cx, &opened, "nav-workspace"), Some(true));
    assert_eq!(selected(cx, &opened, "nav-code"), Some(false));
}

#[gpui_kit::test]
fn a_rail_not_yet_loaded_lists_nothing_and_an_empty_one_says_so(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connecting)]);
    assert!(present(cx, &opened, "nav-workspace"));
    assert!(!present(cx, &opened, "rail-empty"));
    assert!(present(cx, &opened, "rail-wait"));
    let empty = Rail { loaded: true, ..Rail::default() };
    hear(cx, &opened, vec![Event::Rail(Box::new(empty))]);
    assert!(present(cx, &opened, "rail-empty"));
    assert!(!present(cx, &opened, "rail-wait"));
}

#[gpui_kit::test]
fn a_click_on_a_pane_row_asks_for_the_pick_and_the_pane_and_marks_nothing_itself(cx: &mut TestAppContext) {
    // SHELL-RAIL-13: the pick is the session's, and the mark on screen is the daemon's to move.
    let opened = open(cx, connected_and_busy());
    click(cx, &opened, "pane-2");
    assert_eq!(taken(&opened), ["RailPick { checkout: 110, pane: Some(2) }", "Select { pane: 2 }"]);
    assert_eq!(picked(cx, &opened), None, "the row is picked when the session says so");
    assert_eq!(selected(cx, &opened, "pane-1"), Some(true));
    assert_eq!(selected(cx, &opened, "pane-2"), Some(false));
    hear(cx, &opened, vec![picking(110, Some(2))]);
    assert_eq!(picked(cx, &opened), Some((110, Some(2))));
}

#[gpui_kit::test]
fn the_row_the_session_says_is_picked_is_ringed(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    assert!(!present(cx, &opened, "checkout-110-ring"));
    hear(cx, &opened, vec![picking(110, None)]);
    assert!(present(cx, &opened, "checkout-110-ring"));
    assert!(!present(cx, &opened, "checkout-112-ring"));
}

#[gpui_kit::test]
fn a_row_a_carried_pane_would_move_to_is_drawn_as_the_target(cx: &mut TestAppContext) {
    use crate::kit::spots::Spots;
    let opened = open(cx, connected_and_busy());
    assert!(!present(cx, &opened, "checkout-112-target"));
    cx.update(|cx| Spots::of(cx).light(Some(112)));
    cx.update_window(opened.handle, |_, window, cx| {
        window.refresh();
        window.render_frame(cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| opened.rail.update(cx, |_, cx| cx.notify()));
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert!(present(cx, &opened, "checkout-112-target"));
    assert!(present(cx, &opened, "checkout-112-ring"));
    // The pane's own checkout is not offered, and a row not lit is not ringed unless picked.
    assert!(!present(cx, &opened, "checkout-110-target"));
    assert!(!present(cx, &opened, "checkout-110-ring"));
    cx.update(|cx| Spots::of(cx).light(None));
    cx.update(|cx| opened.rail.update(cx, |_, cx| cx.notify()));
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert!(!present(cx, &opened, "checkout-112-target"));
}

#[gpui_kit::test]
fn a_repository_has_a_clock_that_opens_its_recent_sessions_without_choosing_it(cx: &mut TestAppContext) {
    // SHELL-RAIL-5.
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "repo-10-sessions").as_deref(), Some("Recent agent sessions in dotfiles"));
    // Drawn only while the row is pointed at.
    hover(cx, &opened, "repo-10");
    click(cx, &opened, "repo-10-sessions");
    assert_eq!(taken(&opened), ["OpenSessions { repository: 10 }"]);
}

#[gpui_kit::test]
fn a_shared_checkout_says_how_many_agents_share_it_by_the_people_mark(cx: &mut TestAppContext) {
    // SHELL-RAIL-7: `people <n>` titled `shared by <n> agents`; a checkout nobody shares has none.
    let opened = open(cx, connected_and_busy());
    assert!(present(cx, &opened, "checkout-112-shared"));
    assert!(!present(cx, &opened, "checkout-110-shared"));
}

#[gpui_kit::test]
fn every_row_carries_the_words_the_page_gives_its_title(cx: &mut TestAppContext) {
    // The page's `title` is hover text here: the same words, as the label a test reads.
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "repo-11").as_deref(), Some("argus"));
    assert_eq!(label(cx, &opened, "checkout-110-fold").as_deref(), Some("Fold argus main"));
    assert_eq!(label(cx, &opened, "checkout-110-start").as_deref(), Some("Start a shell or an agent on argus main"));
    assert_eq!(label(cx, &opened, "pane-4").as_deref(), Some("Tracker · agent, needs-review"));
}

#[gpui_kit::test]
fn an_unrelated_event_does_not_redraw_the_rail_and_a_change_of_what_it_draws_does(cx: &mut TestAppContext) {
    // The rail reads a slice of what was heard, so a notice or a dialog's page is none of its business.
    let opened = open(cx, connected_and_busy());
    let changes = |cx: &mut TestAppContext| cx.update(|cx| opened.rail.read(cx).changes());
    let before = changes(cx);
    hear(cx, &opened, vec![Event::Notice { text: "something passed".into() }]);
    hear(cx, &opened, vec![Event::Rail(Box::new(busy()))]);
    assert_eq!(changes(cx), before, "nothing the rail draws changed");
    let mut changed = busy();
    changed.working = 7;
    hear(cx, &opened, vec![Event::Rail(Box::new(changed))]);
    assert_eq!(changes(cx), before + 1, "what the foot says changed");
}

#[gpui_kit::test]
fn a_browsers_row_wears_a_globe_and_says_browser_not_shell_and_a_press_shows_its_pane(cx: &mut TestAppContext) {
    // SHELL-BROWSER-20: the page drew a browser as a shell; the port tells them apart, and a press
    // is the ordinary one, since the cell the pane is shown as is the grid's to choose.
    use argus_desktop_core::browser::{BrowsersPage, PageBrowser};
    let opened = open(cx, connected_and_busy());
    assert!(!present(cx, &opened, "pane-2-browser"));
    let page = BrowsersPage {
        browsers: vec![PageBrowser {
            pane: 2,
            owner: 1,
            agent: "Trim context".into(),
            url: String::new(),
            title: String::new(),
            tabs: Vec::new(),
            shown: None,
            following: true,
            held: false,
            driver: None,
            cursor: argus_protocol::BrowserCursor::Default,
            dialog: None,
            select: None,
            sized: None,
            context: None,
        }],
    };
    hear(cx, &opened, vec![Event::Browsers(Box::new(page))]);
    assert!(present(cx, &opened, "pane-2-browser"));
    assert!(!present(cx, &opened, "pane-1-browser"), "only the browsers' rows");
    assert_eq!(label(cx, &opened, "pane-2").as_deref(), Some("zsh \u{b7} browser, idle"));
    click(cx, &opened, "pane-2");
    assert_eq!(taken(&opened), ["RailPick { checkout: 110, pane: Some(2) }", "Select { pane: 2 }"]);
}

#[gpui_kit::test]
fn the_browsers_item_goes_to_the_stage_of_every_agents_browsers(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    assert_eq!(label(cx, &opened, "nav-browsers").as_deref(), Some("Browsers"));
    click(cx, &opened, "nav-browsers");
    assert_eq!(taken(&opened), ["ShowBrowsers { pane: None }"]);
}
