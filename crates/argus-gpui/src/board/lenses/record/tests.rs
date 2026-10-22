use super::*;
use crate::board::snapshots;

// BOARD-RECORD-2: a decision that is out is faint and struck through; an open one's label is in attention.
#[test]
fn a_decision_that_is_out_is_faint_and_an_open_ones_label_is_in_attention() {
    for state in ["superseded", "rejected", "withdrawn"] {
        assert!(out(state), "{state}");
        assert_eq!(label_tone(state), Tone::Faint, "{state}");
    }
    for state in ["accepted", "open"] {
        assert!(!out(state), "{state}");
    }
    assert_eq!(label_tone("open"), Tone::Attention);
    assert_eq!(label_tone("accepted"), Tone::Ref);
}

// BOARD-RECORD-1: a decision made says nothing at the end of its row.
#[test]
fn a_decision_made_says_no_state_and_any_other_says_its_word() {
    assert_eq!(state_word("accepted"), "");
    assert_eq!(state_word("open"), "open");
    assert_eq!(state_word("rejected"), "rejected");
}

#[test]
fn the_cursors_row_is_the_child_after_the_label() {
    let view = snapshots::open("feature-record");
    let rows = &view.record.as_ref().unwrap().rows;
    let at = rows.iter().position(|row| row.on).unwrap();
    assert_eq!(lit_child(&view), Some(1 + at));
    let mut none = view;
    none.record.as_mut().unwrap().rows.iter_mut().for_each(|row| row.on = false);
    assert_eq!(lit_child(&none), None);
}
