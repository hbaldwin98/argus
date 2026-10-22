//! The Move menu, the Comes-after menu and the hold dialog (BOARD-MENU-1 to 9, 14).

use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::{click_label, hear, overlays, press, type_text};
use crate::kit::overlay::kind::Overlay;

fn item(at: usize) -> ElementId {
    ElementId::from(("feature-menu-item", at))
}

/// BOARD-MENU-2, 3: the menu names the feature, offers its pane, and lists the moves Rust says, the
/// ones that follow the work off with the reason at the edge and the one already here ticked.
#[gpui_kit::test]
fn the_menu_lists_the_pane_and_the_moves_with_why_the_derived_ones_are_off(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    right_click(cx, &opened, "feature-row-trim");
    assert_eq!(label(cx, &opened, "feature-menu").as_deref(), Some("#41 Trim context injection"));
    let said: Vec<_> = (1..9).map(|at| label(cx, &opened, item(at))).collect();
    let said: Vec<_> = said.iter().map(|words| words.as_deref()).collect();
    assert_eq!(said, [Some("Go to its pane"), None, None, Some("Queued"), Some("Active"), Some("Done"), Some("Held\u{2026}"), Some("Archived")], "a rule and a heading have no name of their own");
    assert!(!present(cx, &opened, item(10)), "Comes after... is the peek's");
    assert_eq!(selected(cx, &opened, item(1)), Some(true), "the first choice has the highlight");
}

/// BOARD-MENU-3: a move that follows the work cannot be chosen: a press does nothing and the menu stays.
#[gpui_kit::test]
fn a_move_that_follows_the_work_is_off(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    right_click(cx, &opened, "feature-row-trim");
    sent(&opened);
    click(cx, &opened, item(4));
    assert!(sent(&opened).is_empty());
    assert!(present(cx, &opened, "feature-menu"));
}

/// BOARD-MENU-4: choosing Archived or Reopen closes the menu and sends the move; lifting a hold sends
/// it at once.
#[gpui_kit::test]
fn archived_reopen_and_lift_the_hold_are_sent_as_the_calls_they_are(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    right_click(cx, &opened, "feature-row-trim");
    sent(&opened);
    click(cx, &opened, item(8));
    assert_eq!(sent(&opened), ["Archive { slug: \"trim\", archived: true }"]);
    assert!(!present(cx, &opened, "feature-menu"));
    right_click(cx, &opened, "feature-row-old");
    sent(&opened);
    click(cx, &opened, item(6));
    assert_eq!(sent(&opened), ["Archive { slug: \"old\", archived: false }"]);
    right_click(cx, &opened, "feature-row-bump");
    sent(&opened);
    click(cx, &opened, item(5));
    assert_eq!(sent(&opened), ["Hold { slug: \"bump\", reason: None }"]);
}

/// BOARD-MENU-1: right click on a card or row opens the menu without a Comes-after item; and a
/// snapshot that takes the feature away takes the menu with it.
#[gpui_kit::test]
fn a_snapshot_that_takes_the_feature_away_closes_its_menu(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    right_click(cx, &opened, "feature-row-mouse");
    assert!(present(cx, &opened, "feature-menu"));
    let mut without = list();
    without.rows.retain(|row| row.slug != "mouse");
    hear(cx, &opened, vec![heard(without)]);
    assert!(!present(cx, &opened, "feature-menu"));
}

/// BOARD-MENU-6: Up, Down and `j`, `k` step through the choices that can be made, wrapping; Enter
/// chooses the one lit.
#[gpui_kit::test]
fn the_keys_step_through_the_choices_that_can_be_made_and_enter_chooses(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    right_click(cx, &opened, "feature-row-trim");
    sent(&opened);
    press(cx, &opened, "j");
    assert_eq!(selected(cx, &opened, item(7)), Some(true), "Queued, Active and Done are passed over");
    press(cx, &opened, "down");
    assert_eq!(selected(cx, &opened, item(8)), Some(true));
    press(cx, &opened, "j");
    assert_eq!(selected(cx, &opened, item(1)), Some(true), "wrapping");
    press(cx, &opened, "k");
    assert_eq!(selected(cx, &opened, item(8)), Some(true));
    press(cx, &opened, "enter");
    assert_eq!(sent(&opened), ["Archive { slug: \"trim\", archived: true }"]);
    assert!(!present(cx, &opened, "feature-menu"));
}

/// BOARD-MENU-5: Comes after... closes the menu and opens a second where it was, with a tick on what
/// the feature waits on already; choosing one sends it or takes it back.
#[gpui_kit::test]
fn comes_after_offers_the_open_features_and_takes_one_back(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    click(cx, &opened, "peek-more");
    click(cx, &opened, item(10));
    assert_eq!(label(cx, &opened, "feature-menu").as_deref(), Some("#41 comes after\u{2026}"));
    assert_eq!(label(cx, &opened, item(1)).as_deref(), Some("Mouse selection"));
    click(cx, &opened, item(1));
    assert_eq!(sent(&opened), ["Wait { slug: \"trim\", on: \"mouse\", waits: true }"]);
    click(cx, &opened, "peek-more");
    click(cx, &opened, item(10));
    click(cx, &opened, item(2));
    assert_eq!(sent(&opened), ["Wait { slug: \"trim\", on: \"ssh\", waits: false }"], "taking back");
}

/// BOARD-MENU-2: Go to its pane is the menu's first item, and sends the pane.
#[gpui_kit::test]
fn go_to_its_pane_in_the_menu_sends_the_pane(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    right_click(cx, &opened, "feature-row-trim");
    sent(&opened);
    click(cx, &opened, item(1));
    assert_eq!(sent(&opened), ["GoToPane { pane: 1 }"]);
}

/// BOARD-MENU-7, 8: Held... asks why in a dialog; Enter or Hold sends the reason as typed and closes;
/// nothing is sent until then.
#[gpui_kit::test]
fn held_asks_why_and_enter_sends_the_reason_as_typed(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    right_click(cx, &opened, "feature-row-trim");
    sent(&opened);
    click(cx, &opened, item(7));
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    assert_eq!(overlays(cx), [Overlay::Hold]);
    assert!(sent(&opened).is_empty());
    type_text(cx, &opened, "until the bump");
    press(cx, &opened, "enter");
    assert_eq!(sent(&opened), ["Hold { slug: \"trim\", reason: Some(\"until the bump\") }"]);
    assert!(overlays(cx).is_empty());
}

/// BOARD-MENU-8: Hold sends too, an empty reason as it is (Rust turns it into lifting the hold), and
/// Escape and Cancel close it sending nothing.
#[gpui_kit::test]
fn the_hold_button_sends_and_escape_and_cancel_send_nothing(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    for how in ["escape", "cancel"] {
        press(cx, &opened, "h");
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        assert_eq!(overlays(cx), [Overlay::Hold], "{how}");
        if how == "escape" {
            press(cx, &opened, "escape");
        } else {
            click_label(cx, &opened, "Cancel");
        }
        assert!(overlays(cx).is_empty() && sent(&opened).is_empty(), "{how}");
    }
    press(cx, &opened, "h");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    click_label(cx, &opened, "Hold");
    assert_eq!(sent(&opened), ["Hold { slug: \"trim\", reason: Some(\"\") }"]);
}
