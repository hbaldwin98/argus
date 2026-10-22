//! The Feature layer of the key walk (BOARD-KEYS-1 to 13, BOARD-MENU-6).

use gpui_kit::TestAppContext;

use super::*;
use crate::keys::root::tests::{overlays, press};
use crate::kit::overlay::kind::Overlay;

fn keyed(cx: &mut TestAppContext, opened: &Opened, key: &str) -> Vec<String> {
    press(cx, opened, key);
    sent(opened)
}

/// BOARD-KEYS-1, 4, 5: the moving keys, the layout and the fold are sent as the action they are, and
/// the index changes when the session says so.
#[gpui_kit::test]
fn the_index_keys_are_sent_as_the_actions_they_are(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    for (key, action) in [("j", "Next"), ("down", "Next"), ("k", "Previous"), ("up", "Previous"), ("home", "First"), ("end", "Last"), ("v", "Layout"), ("shift-v", "Layout"), ("z", "Folded")] {
        assert_eq!(keyed(cx, &opened, key), [format!("FeatureAction {{ action: {action} }}")], "{key}");
    }
    assert_eq!(selected(cx, &opened, "feature-row-trim"), Some(true), "no key predicts where the cursor goes");
}

/// BOARD-KEYS-7: a capital N opens the note; a small n is not bound here.
#[gpui_kit::test]
fn a_capital_n_opens_the_note_and_a_small_one_does_nothing(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert_eq!(keyed(cx, &opened, "shift-n"), ["Capture { gesture: Open }"]);
    assert!(keyed(cx, &opened, "n").is_empty());
}

/// BOARD-KEYS-8: the card keys act on the feature under the cursor.
#[gpui_kit::test]
fn the_card_keys_act_on_the_feature_under_the_cursor(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert_eq!(keyed(cx, &opened, "enter"), ["OpenFeature { slug: Some(\"trim\") }"]);
    assert_eq!(keyed(cx, &opened, "g"), ["GoToPane { pane: 1 }"]);
    assert_eq!(keyed(cx, &opened, "shift-a"), ["FeatureAction { action: Archive }"]);
    assert!(keyed(cx, &opened, "a").is_empty(), "archive is the capital alone");
}

/// BOARD-KEYS-8: `H` lifts the hold of a held feature at once, and asks why of one that is not.
#[gpui_kit::test]
fn h_lifts_a_hold_and_asks_why_of_a_feature_not_held(cx: &mut TestAppContext) {
    let opened = on(cx, peeking(list(), "bump"));
    assert_eq!(keyed(cx, &opened, "h"), ["FeatureAction { action: Hold }"]);
    hear(cx, &opened, vec![heard(list())]);
    assert!(keyed(cx, &opened, "h").is_empty(), "nothing is sent until the dialog's Hold");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    assert_eq!(overlays(cx), [Overlay::Hold]);
}

/// BOARD-KEYS-8: with nothing peeked, or a peeked feature the rows do not draw, a card key is not the
/// stage's: it does nothing and nothing opens.
#[gpui_kit::test]
fn a_card_key_with_nothing_to_act_on_is_not_the_stages(cx: &mut TestAppContext) {
    let nothing = Features { peek: None, ..list() };
    let opened = on(cx, nothing);
    for key in ["enter", "g", "h", "shift-a", "m"] {
        assert!(keyed(cx, &opened, key).is_empty(), "{key}");
    }
    assert!(overlays(cx).is_empty() && !present(cx, &opened, "feature-menu"));
    let mut hidden = list();
    hidden.peek = Some(peek_of(&row(99, "gone", "Folded away", "archived")));
    hear(cx, &opened, vec![heard(hidden)]);
    assert!(keyed(cx, &opened, "enter").is_empty(), "the peeked feature is not among the rows drawn");
}

/// BOARD-KEYS-13: Escape on the bare index sends nothing and puts nothing away; an open question of
/// `gpui/audit.md` §5 7 until a person decides.
#[gpui_kit::test]
fn escape_on_the_bare_index_sends_no_call(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert!(keyed(cx, &opened, "escape").is_empty());
    assert!(overlays(cx).is_empty());
}

/// BOARD-KEYS-11, SHELL-KEYS-23: the window's own digits still switch stages, and a key the stage
/// binds nothing to is held, not typed into the pane under it.
#[gpui_kit::test]
fn the_stage_digits_still_switch_and_an_unbound_key_is_held(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    assert_eq!(keyed(cx, &opened, "1"), ["ShowFeatures { open: false }"], "the Workspace");
    assert!(keyed(cx, &opened, "q").is_empty());
}

/// BOARD-MENU-6: with a menu open its keys step through it and every other key of the stage is held;
/// Escape puts it away.
#[gpui_kit::test]
fn while_a_menu_is_open_the_stages_other_keys_are_held(cx: &mut TestAppContext) {
    let opened = on(cx, list());
    press(cx, &opened, "m");
    assert!(present(cx, &opened, "feature-menu"), "M opens the Move menu");
    for key in ["j", "k", "down", "up", "v", "z", "shift-n", "enter"] {
        press(cx, &opened, key);
    }
    assert!(!sent(&opened).iter().any(|call| call.starts_with("FeatureAction") || call.starts_with("Capture")), "held");
    press(cx, &opened, "m");
    press(cx, &opened, "escape");
    assert!(!present(cx, &opened, "feature-menu"));
    assert!(sent(&opened).is_empty(), "closing a menu sends nothing");
}
