//! The header of an open feature and the tools on the strip (BOARD-HEAD-1 to -11).

use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, TestAppContext};

use super::{on, push, view};
use crate::checkouts::stage::tests::{click_id, expanded, label, present, selected};
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
    assert_eq!(label(cx, &opened, "feature-brief").as_deref(), Some(feature.brief.as_str()), "the brief, whole, to a reader");
    assert_eq!(label(cx, &opened, "brief-edit-button").as_deref(), Some("Edit brief"), "and the way into editing it");
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
    assert!(!present(cx, &opened, "feature-brief") && !present(cx, &opened, "brief-text"), "no brief draws no block under the line");
    assert_eq!(label(cx, &opened, "brief-write").as_deref(), Some("Write the brief"), "but the line offers to write one");
    click_id(cx, &opened, "brief-write");
    assert!(has_label(cx, &opened, "Save the brief"), "which opens the brief's form");
}

/// Draws the frame a measured box asked for, as a real window's next frame would.
fn settle(cx: &mut TestAppContext, opened: &crate::keys::root::tests::Opened) {
    for _ in 0..2 {
        cx.update_window(opened.handle, |_, window, cx| {
            window.simulate_next_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(opened.handle, |_, window, cx| window.render_frame(cx)).unwrap();
        cx.run_until_parked();
    }
}

fn height(cx: &mut TestAppContext, opened: &crate::keys::root::tests::Opened, id: &'static str) -> f32 {
    cx.update_window(opened.handle, |_, window, _| window.find(id).bounds().size.height.as_f32()).unwrap()
}

/// The scene's brief said over and over, so it runs past two lines at any width the window has.
fn long_brief(cx: &mut TestAppContext, opened: &crate::keys::root::tests::Opened) {
    let mut long = view(opened, cx);
    long.brief = [long.brief.as_str(); 12].join(" ");
    push(cx, opened, long);
    settle(cx, opened);
}

// BOARD-HEAD-12: the brief is read in two lines under the status line until the whole is asked for, and
// the whole stays inside a box that scrolls, so the outline keeps its place.
#[gpui_kit::test]
fn a_long_brief_shows_two_lines_until_show_all_and_show_less_puts_it_back(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    long_brief(cx, &opened);
    let tokens = cx.update(|cx| *crate::theme::Tokens::global(cx));
    let line = cx.update_window(opened.handle, |_, window, _| crate::theme::scale::pixels(tokens.type_body.size * tokens.type_body.leading, window.rem_size()).as_f32()).unwrap();
    let clamped = height(cx, &opened, "brief-text");
    assert_eq!(expanded(cx, &opened, "brief-toggle"), Some(false), "a brief past two lines offers the rest");
    assert_eq!(label(cx, &opened, "brief-toggle").as_deref(), Some("Show all"));
    assert!(clamped > line + 0.5 && clamped <= 2.0 * line + 1.0, "two lines while clamped: {clamped} for lines of {line}");

    click_id(cx, &opened, "brief-toggle");
    let whole = height(cx, &opened, "brief-text");
    assert_eq!(expanded(cx, &opened, "brief-toggle"), Some(true));
    assert_eq!(label(cx, &opened, "brief-toggle").as_deref(), Some("Show less"));
    assert!(whole > clamped, "the whole brief is taller than its two lines: {whole} against {clamped}");
    let most = cx.update_window(opened.handle, |_, window, _| crate::theme::scale::pixels(tokens.brief_max_h, window.rem_size()).as_f32()).unwrap();
    assert!(whole <= most + 0.5, "and never taller than the box it scrolls in: {whole} against {most}");

    let again = view(&opened, cx);
    push(cx, &opened, again);
    assert_eq!(expanded(cx, &opened, "brief-toggle"), Some(true), "a push of the same feature keeps it whole");

    click_id(cx, &opened, "brief-toggle");
    assert_eq!(expanded(cx, &opened, "brief-toggle"), Some(false), "Show less puts it back to two lines");
    assert_eq!(height(cx, &opened, "brief-text"), clamped);
    assert!(drain(&opened).is_empty(), "reading the brief is the window's own and sends nothing");
}

#[gpui_kit::test]
fn a_brief_that_fits_its_two_lines_offers_nothing_more(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let mut short = view(&opened, cx);
    short.brief = "Serve the brief on demand.".into();
    push(cx, &opened, short);
    settle(cx, &opened);
    assert!(present(cx, &opened, "brief-text"), "the brief is drawn");
    assert!(!present(cx, &opened, "brief-toggle"), "with no Show all that would show nothing more");
}

#[gpui_kit::test]
fn a_brief_read_whole_goes_back_to_two_lines_when_the_feature_closes(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    long_brief(cx, &opened);
    click_id(cx, &opened, "brief-toggle");
    assert_eq!(expanded(cx, &opened, "brief-toggle"), Some(true));
    let mut page = super::features(&opened, cx);
    let open = page.open.take();
    crate::keys::root::tests::hear(cx, &opened, vec![argus_desktop_core::connection::page::Event::Features(Box::new(page.clone()))]);
    page.open = open;
    crate::keys::root::tests::hear(cx, &opened, vec![argus_desktop_core::connection::page::Event::Features(Box::new(page))]);
    settle(cx, &opened);
    assert_eq!(expanded(cx, &opened, "brief-toggle"), Some(false), "opened again, the brief is two lines");
}

// BOARD-HEAD-3: B still opens the brief's form, which holds the brief whole.
#[gpui_kit::test]
fn b_opens_the_briefs_form_on_the_whole_brief(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    long_brief(cx, &opened);
    let feature = view(&opened, cx);
    blur(cx, &opened);
    press(cx, &opened, "b");
    let said = cx.update_window(opened.handle, |_, window, _| window.find_by_label("Brief").value().map(str::to_owned)).unwrap();
    assert_eq!(said.as_deref(), Some(feature.brief.as_str()));
    assert!(!present(cx, &opened, "brief-text"), "the form is in the brief's place");
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

// BOARD-HEAD-3, -4: Edit beside the brief opens its form, which replaces the line.
#[gpui_kit::test]
fn the_brief_opens_its_form_in_place_of_the_status_line(cx: &mut TestAppContext) {
    let opened = on(cx, "feature-work");
    let feature = view(&opened, cx);
    click_id(cx, &opened, "brief-edit-button");
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

