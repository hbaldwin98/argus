//! Which mode has the keys: the one answer dispatch, the bar and `?` read.

use super::*;
use crate::app::{Mode, OverlayMode};

#[test]
fn the_mode_follows_what_claims_the_keys_first() {
    let mut h = Harness::new();
    assert_eq!(h.app.mode(), Mode::Rail, "a fresh client is on the rail");

    h.key(KeyCode::Char('n')); // add a project: the directory browser
    assert_eq!(h.app.mode(), Mode::DirPicker);
    h.key(KeyCode::Esc);

    h.app.overlay = Some(Overlay::SequenceDiagram);
    assert_eq!(h.app.mode(), Mode::Overlay(OverlayMode::SequenceDiagram));
    h.app.prompt = Some(Prompt::ConfirmRemove {
        target: RemoveTarget::Project(ProjectId(1)),
        label: "argus".to_string(),
    });
    assert_eq!(h.app.mode(), Mode::Prompt, "a question outranks the window under it");
    h.app.prompt = None;
    h.app.overlay = None;

    h.checkouts_stage();
    assert_eq!(h.app.mode(), Mode::Stage(View::Checkouts));
    h.key(KeyCode::Char('/'));
    assert_eq!(h.app.mode(), Mode::CheckoutFilter);
    h.key(KeyCode::Esc);

    h.key(KeyCode::Char('1'));
    h.keys("llll");
    assert_eq!(h.app.mode(), Mode::Pane, "typing into the workspace terminal");
}

#[test]
fn a_question_mark_is_typed_wherever_text_is_being_typed() {
    // A branch filter and a feature line both take text; `?` there is a
    // character, not a request for the keymap.
    let mut h = Harness::new();
    h.checkouts_stage();
    h.key(KeyCode::Char('/'));
    h.key(KeyCode::Char('?'));
    assert!(h.app.help.is_none());
    assert_eq!(h.app.checkout_filter, "?");
    h.key(KeyCode::Esc);

    h.key(KeyCode::Char('2'));
    h.app.on_server_msg(ServerMsg::Decisions(Box::new(argus_protocol::DecisionBoard {
        project: Some(ProjectId(1)),
        name: "argus".to_string(),
        features: Vec::new(),
        decisions: Vec::new(),
    })));
    h.key(KeyCode::Char('a')); // a new feature, named on a line
    h.key(KeyCode::Char('?'));
    assert!(h.app.help.is_none());
    assert_eq!(h.app.line.as_ref().map(|line| line.text.as_str()), Some("?"));
}
