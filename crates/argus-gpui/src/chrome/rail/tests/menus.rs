//! The rail's right-click menus: what each row offers, and that a choice sends the page's calls.

use super::*;
use crate::kit::overlay::host::Overlays;
use crate::kit::overlay::kind::Overlay;

fn right_click(cx: &mut TestAppContext, opened: &Opened, id: &str) {
    cx.update_window(opened.handle, |_, window, cx| window.right_click(id.to_owned(), cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.run_until_parked();
    cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// Chooses the item of the open menu named `label`: the kit's menu is the one in the window with that
/// name under its own root, since a row of the rail may be named the same.
fn choose(cx: &mut TestAppContext, opened: &Opened, label: &str) {
    let at = cx
        .update_window(opened.handle, |_, window, _| {
            let item = window.elements().into_iter().find(|element| {
                element.label() == Some(label)
                    && element.path().iter().any(|id| matches!(id, gpui_kit::ElementId::Name(name) if name.as_ref() == "popup-menu"))
            });
            item.unwrap_or_else(|| panic!("the open menu has no {label}")).bounds().center()
        })
        .unwrap();
    press(cx, opened, at, MouseButton::Left);
    cx.run_until_parked();
}

fn with_agents() -> Event {
    let mut rail = busy();
    rail.templates = vec!["claude".into(), "codex".into()];
    rail.project.as_mut().unwrap().repositories[0].checkouts[1].kind = "worktree";
    Event::Rail(Box::new(rail))
}

#[gpui_kit::test]
fn a_repositorys_menu_chooses_the_repository_before_showing_its_features(cx: &mut TestAppContext) {
    // SHELL-RAIL-6.
    let opened = open(cx, connected_and_busy());
    right_click(cx, &opened, "repo-row-10");
    choose(cx, &opened, "Features");
    assert_eq!(taken(&opened), ["ChooseRepository { repository: 10 }", "ShowFeatures { open: true }"]);
}

#[gpui_kit::test]
fn a_repositorys_menu_folds_it_and_opens_its_recent_sessions(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    right_click(cx, &opened, "repo-row-10");
    choose(cx, &opened, "Expand");
    right_click(cx, &opened, "repo-row-11");
    choose(cx, &opened, "Collapse");
    right_click(cx, &opened, "repo-row-11");
    choose(cx, &opened, "Recent agent sessions");
    assert_eq!(taken(&opened), ["ToggleRepository { repository: 10 }", "ToggleRepository { repository: 11 }", "OpenSessions { repository: 11 }"]);
}

#[gpui_kit::test]
fn a_checkouts_menu_starts_what_the_daemon_offers_and_asks_for_its_review_and_history(cx: &mut TestAppContext) {
    // SHELL-RAIL-10.
    let opened = open(cx, vec![status(Connection::Connected), with_agents()]);
    right_click(cx, &opened, "checkout-110");
    choose(cx, &opened, "New agent \u{b7} codex");
    right_click(cx, &opened, "checkout-110");
    choose(cx, &opened, "New shell");
    right_click(cx, &opened, "checkout-110");
    choose(cx, &opened, "Review changes");
    right_click(cx, &opened, "checkout-110");
    choose(cx, &opened, "History");
    right_click(cx, &opened, "checkout-110");
    choose(cx, &opened, "Show in Checkouts");
    assert_eq!(
        taken(&opened),
        [
            "StartOn { checkout: 110, template: Some(\"codex\") }",
            "StartOn { checkout: 110, template: None }",
            "OpenReview { checkout: 110 }",
            "OpenHistory { checkout: 110 }",
            "GoToCheckout { checkout: 110 }",
        ]
    );
}

#[gpui_kit::test]
fn only_a_worktrees_menu_offers_to_remove_it(cx: &mut TestAppContext) {
    let opened = open(cx, vec![status(Connection::Connected), with_agents()]);
    right_click(cx, &opened, "checkout-110");
    assert!(cx.update_window(opened.handle, |_, window, _| window.try_find_by_label("Remove worktree\u{2026}").is_none()).unwrap(), "a main checkout offered its removal");
    cx.update_window(opened.handle, |_, window, cx| window.press("escape", cx)).unwrap();
    right_click(cx, &opened, "checkout-112");
    choose(cx, &opened, "Remove worktree\u{2026}");
    assert_eq!(taken(&opened), ["RemoveCheckout { checkout: 112 }"]);
}

#[gpui_kit::test]
fn a_panes_menu_shows_it_as_a_click_does_and_opens_it_beside_and_closes_it(cx: &mut TestAppContext) {
    // SHELL-RAIL-15.
    let opened = open(cx, connected_and_busy());
    right_click(cx, &opened, "pane-row-2");
    choose(cx, &opened, "Show");
    right_click(cx, &opened, "pane-row-2");
    choose(cx, &opened, "Open beside");
    right_click(cx, &opened, "pane-row-2");
    choose(cx, &opened, "Close");
    assert_eq!(
        taken(&opened),
        ["RailPick { checkout: 110, pane: Some(2) }", "Select { pane: 2 }", "OpenBeside { pane: 2 }", "Close { pane: 2 }"]
    );
}

#[gpui_kit::test]
fn a_panes_rename_opens_the_dialog_and_the_name_is_asked_of_the_session_to_judge(cx: &mut TestAppContext) {
    // SHELL-RENAME-1, 2: the window sends what was typed; trimming and comparing are `Session::rename`'s.
    let opened = open(cx, connected_and_busy());
    right_click(cx, &opened, "pane-row-2");
    choose(cx, &opened, "Rename\u{2026}");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx.update(|cx| Overlays::get(cx).is_open(Overlay::Rename)), "the rename dialog is up");
    for key in ["b", "u"] {
        cx.update_window(opened.handle, |_, window, cx| window.press(key, cx)).unwrap();
    }
    cx.update_window(opened.handle, |_, window, cx| window.press("enter", cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(taken(&opened), ["RenamePane { pane: 2, title: \"bu\" }"], "{:?}", taken(&opened));
}

#[gpui_kit::test]
fn a_held_rails_menus_send_nothing(cx: &mut TestAppContext) {
    let opened = open(cx, connected_and_busy());
    hear(cx, &opened, vec![status(Connection::Stopped)]);
    right_click(cx, &opened, "pane-row-2");
    choose(cx, &opened, "Close");
    assert!(taken(&opened).is_empty(), "{:?}", taken(&opened));
}
