//! The header of an open feature and the tools on the strip (BOARD-HEAD-1 to -11).

use gpui_kit::test::TestQueryExt as _;
use gpui_kit::{AppContext as _, TestAppContext};

use super::{on, push, view};
use crate::checkouts::stage::tests::{click_id, label, present, selected};
use crate::keys::root::tests::{click_label, drain, has_label, press};

// BOARD-HEAD-1: how it stands, how far it is, the tickets and the brief.
#[gpui_kit::test]
fn the_header_says_how_it_stands_how_far_it_is_its_tickets_and_its_brief(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    assert!(has_label(cx, &opened, feature.standing), "the standing pill");
    let tally = format!("{} of {} tasks \u{b7} {} decided \u{b7} {} open", feature.tasks_done, feature.tasks_total, feature.decided, feature.open);
    assert!(has_label(cx, &opened, &tally), "the progress bar says what it counts: {tally}");
    assert!(has_label(cx, &opened, feature.tickets.as_deref().expect("it has tickets")), "one button for the linked tickets");
    let brief = label(cx, &opened, "feature-brief").expect("the brief line");
    assert!(brief.contains(&feature.brief) && brief.ends_with("Edit the brief (B)"), "the brief and the way into editing it: {brief}");
}

#[gpui_kit::test]
fn a_feature_with_no_tasks_has_no_progress_and_no_brief_says_so(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let mut empty = view(&opened, cx);
    empty.tasks_total = 0;
    empty.tasks_done = 0;
    empty.brief = String::new();
    empty.links.clear();
    empty.tickets = None;
    push(cx, &opened, empty);
    assert!(!present(cx, &opened, "feature-progress"), "no bar for no tasks");
    assert!(!present(cx, &opened, "feature-tickets"), "and no tickets button for no tickets");
    assert_eq!(label(cx, &opened, "feature-brief").as_deref(), Some("Write the brief (B)"));
}

// BOARD-HEAD-2: the tickets button opens the link dialog.
#[gpui_kit::test]
fn the_tickets_button_opens_the_link_dialog(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    click_id(cx, &opened, "feature-tickets");
    assert_eq!(drain(&opened), ["OpenLink { map: false }"]);
}

// BOARD-HEAD-5: three radio segments in order, each with a count, the lens read lit.
#[gpui_kit::test]
fn the_lenses_are_a_radio_with_a_count_each_and_choosing_one_sends_it(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    assert_eq!((selected(cx, &opened, "lens-1"), selected(cx, &opened, "lens-2"), selected(cx, &opened, "lens-3")), (Some(true), Some(false), Some(false)));
    let counts = (feature.tasks_total, feature.decided + feature.open);
    assert_eq!(counts, (7, 3), "the page's Work 7, Record 3");
    click_id(cx, &opened, "lens-2");
    assert_eq!(drain(&opened), ["SetLens { lens: Record }"]);
    click_id(cx, &opened, "lens-3");
    assert_eq!(drain(&opened), ["SetLens { lens: Flows }"]);
}

#[gpui_kit::test]
fn the_digits_are_the_lenses_before_they_are_the_stages(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    blur(cx, &opened);
    for (key, lens) in [("1", "Work"), ("2", "Record"), ("3", "Flows")] {
        press(cx, &opened, key);
        assert_eq!(drain(&opened), [format!("SetLens {{ lens: {lens} }}")], "{key}");
    }
}

fn blur(cx: &mut TestAppContext, opened: &crate::keys::root::tests::Opened) {
    crate::keys::root::tests::blur(cx, opened);
}

// BOARD-HEAD-3, -4: the brief is the way into editing it, and its form replaces the line.
#[gpui_kit::test]
fn the_brief_opens_its_form_in_place_of_the_status_line(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    click_id(cx, &opened, "feature-brief");
    assert!(has_label(cx, &opened, "Save the brief") && has_label(cx, &opened, "Cancel"));
    assert!(!present(cx, &opened, "feature-progress"), "the line is the form while it is written");
    let said = cx.update_window(opened.handle, |_, window, _| window.find_by_label("Brief").value().map(str::to_owned)).unwrap();
    assert_eq!(said.as_deref(), Some(feature.brief.as_str()), "the form opens on the brief as it is");
}

// BOARD-HEAD-7: the feature menu.
#[gpui_kit::test]
fn the_feature_menu_offers_the_brief_the_tickets_and_the_reconcile(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    assert!(has_label(cx, &opened, "Feature menu") || has_label(cx, &opened, "Feature menu: Reconcile, a proposal waits"));
    click_id(cx, &opened, "feature-menu-button");
    for item in ["Edit the brief", "Link a ticket\u{2026}"] {
        assert!(has_label(cx, &opened, item), "{item}");
    }
    click_label(cx, &opened, "Link a ticket\u{2026}");
    assert_eq!(drain(&opened), ["OpenLink { map: false }"]);
}

#[gpui_kit::test]
fn the_reconcile_entry_is_in_the_menu_and_a_waiting_proposal_marks_the_button(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let mut waiting = view(&opened, cx);
    waiting.reconcile = Some("proposed");
    waiting.reconcile_entry = Some(argus_desktop_core::board::lenses::head::reconcile_entry(Some("proposed"), Some("3d"), Some(11)));
    push(cx, &opened, waiting);
    assert!(present(cx, &opened, "feature-attention"), "a dot says a proposal waits");
    assert!(has_label(cx, &opened, "Feature menu: Reconcile, a proposal waits"));
    click_id(cx, &opened, "feature-menu-button");
    click_label(cx, &opened, "Reconcile \u{2014} a proposal waits");
    assert_eq!(drain(&opened), ["OpenReconcile"]);
}

#[gpui_kit::test]
fn the_views_that_name_the_feature_are_in_the_menu(cx: &mut TestAppContext) {
    use argus_desktop_core::connection::page::FeatureViewLink;
    let opened = on(cx, "feature-work");
    let mut named = view(&opened, cx);
    named.views = vec![FeatureViewLink { pane: 4, id: "plan".into(), title: "The plan".into() }];
    push(cx, &opened, named);
    click_id(cx, &opened, "feature-menu-button");
    click_label(cx, &opened, "The plan");
    assert_eq!(drain(&opened), ["OpenView { pane: 4, id: \"plan\" }"]);
}

