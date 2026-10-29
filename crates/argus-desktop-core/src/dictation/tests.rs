//! The run from press to paste, as the design page settles it (the Argus Dictation scope's "Where
//! the text goes" and "Every state the panel can be in"), and the page the window draws of it.

use argus_protocol::PaneId;

use super::machine::{Engine, FileState, Machine, ModelFile};
use super::model::Model;
use super::panel::{Meter, Tone};
use super::preferences::{DictationChange, DictationPreferences};
use super::run::{Mode, Phase, Problem};
use super::*;

fn machine(parakeet: FileState, nemotron: FileState, engine: Engine) -> Machine {
    Machine {
        files: vec![
            ModelFile { model: Model::Parakeet, size: 731_357_568, state: parakeet },
            ModelFile { model: Model::Nemotron, size: 751_094_240, state: nemotron },
        ],
        engine,
        microphones: Vec::new(),
        default_microphone: None,
    }
}

fn ready() -> Machine {
    machine(FileState::Ready, FileState::Missing, Engine::Unloaded)
}

fn open(pane: PaneId) -> impl Fn(PaneId) -> bool {
    move |p| p == pane
}

/// A dictation with a machine whose Parakeet is downloaded, and the keys in pane 1.
fn dictation() -> Dictation {
    let mut d = Dictation::new(DictationPreferences::default());
    let alive = |_| true;
    d.heard(Heard::Machine(ready()), &Here { pane: None, alive: &alive });
    d
}

fn here(pane: u64, alive: &dyn Fn(PaneId) -> bool) -> Here<'_> {
    Here { pane: Some((PaneId(pane), format!("pane {pane}"))), alive }
}

fn press(d: &mut Dictation, mode: Mode, pane: u64) -> Vec<Deliver> {
    let alive = |_| true;
    d.gesture(DictationGesture::Press { mode, field: false }, &here(pane, &alive))
}

fn gesture(d: &mut Dictation, gesture: DictationGesture) -> Vec<Deliver> {
    let alive = |_| true;
    d.gesture(gesture, &here(1, &alive))
}

fn run_id(d: &mut Dictation) -> u64 {
    d.take_asks().iter().find_map(|ask| match ask {
        Ask::Listen { run, .. } => Some(*run),
        _ => None,
    }).expect("the engine was asked to listen")
}

fn phase(d: &Dictation) -> Option<Phase> {
    d.run.as_ref().map(|run| run.phase.clone())
}

#[test]
fn the_target_is_locked_at_the_press_and_the_text_goes_there_as_one_paste() {
    // "It is locked for the run. Clicking another pane mid-sentence doesn't split the dictation."
    let mut d = dictation();
    press(&mut d, Mode::Hold, 1);
    let run = run_id(&mut d);
    gesture(&mut d, DictationGesture::Release);
    assert_eq!(d.take_asks(), vec![Ask::Stop { run }]);
    let alive = |_| true;
    let delivered = d.heard(Heard::Final { run, text: " Paste this into the focused pane. ".into() }, &here(2, &alive));
    assert_eq!(delivered, vec![Deliver::Paste { pane: PaneId(1), text: "Paste this into the focused pane.".into(), enter: false }]);
    assert_eq!(phase(&d), Some(Phase::Pasted { text: "Paste this into the focused pane.".into() }));
}

#[test]
fn escape_discards_a_live_run_and_nothing_it_said_is_pasted() {
    let mut d = dictation();
    press(&mut d, Mode::Toggle, 1);
    let run = run_id(&mut d);
    assert!(gesture(&mut d, DictationGesture::Escape).is_empty());
    assert_eq!(d.take_asks(), vec![Ask::Cancel { run }]);
    assert_eq!(phase(&d), None, "the panel goes");
    let alive = |_| true;
    assert!(d.heard(Heard::Final { run, text: "late words".into() }, &here(1, &alive)).is_empty(), "a discarded run's text goes nowhere");
}

#[test]
fn a_target_closed_while_speaking_holds_the_text_for_the_person_to_place() {
    // "If target pane closed mid-run: hold text in panel, Enter pastes into now-focused pane, C copies, Esc drops."
    let mut d = dictation();
    press(&mut d, Mode::Hold, 1);
    let run = run_id(&mut d);
    gesture(&mut d, DictationGesture::Release);
    let gone = open(PaneId(2));
    assert!(d.heard(Heard::Final { run, text: "rename the toggle".into() }, &here(2, &gone)).is_empty(), "nothing is pasted into a closed pane");
    assert_eq!(phase(&d), Some(Phase::Held { text: "rename the toggle".into() }));
    assert_eq!(d.take_asks(), vec![Ask::Stop { run }]);
    press(&mut d, Mode::Hold, 2);
    assert!(d.take_asks().is_empty(), "a press does not lose held text");

    let mut copied = d.run.clone();
    let delivered = gesture(&mut d, DictationGesture::PasteHeld);
    assert_eq!(delivered, vec![Deliver::Paste { pane: PaneId(1), text: "rename the toggle".into(), enter: false }], "into the pane with the keys now");

    std::mem::swap(&mut d.run, &mut copied);
    assert_eq!(gesture(&mut d, DictationGesture::CopyHeld), vec![Deliver::Copy("rename the toggle".into())]);
    assert_eq!(phase(&d), None);

    d.run = copied.map(|mut run| {
        run.phase = Phase::Held { text: "x".into() };
        run
    });
    gesture(&mut d, DictationGesture::Escape);
    assert_eq!(phase(&d), None, "Esc drops it");
}

#[test]
fn a_held_run_is_not_dismissed_by_its_timer() {
    let mut d = dictation();
    press(&mut d, Mode::Hold, 1);
    let run = run_id(&mut d);
    let gone = |_| false;
    d.heard(Heard::Final { run, text: "keep me".into() }, &here(1, &gone));
    gesture(&mut d, DictationGesture::Dismiss { run });
    assert!(matches!(phase(&d), Some(Phase::Held { .. })), "the text is never thrown away silently");
}

#[test]
fn a_final_with_nothing_heard_pastes_nothing() {
    // The engine retries an empty stream in one batch pass (argus-dictation's `engine::run`); what
    // reaches here empty after that was silence.
    let mut d = dictation();
    press(&mut d, Mode::Hold, 1);
    let run = run_id(&mut d);
    let alive = |_| true;
    assert!(d.heard(Heard::Final { run, text: "  ".into() }, &here(1, &alive)).is_empty());
    assert_eq!(phase(&d), Some(Phase::Empty));
}

#[test]
fn letting_go_ends_a_held_run_but_not_a_toggled_one_and_a_held_chord_repeating_is_one_press() {
    let mut d = dictation();
    press(&mut d, Mode::Toggle, 1);
    let run = run_id(&mut d);
    gesture(&mut d, DictationGesture::Release);
    assert!(d.take_asks().is_empty(), "a toggled run is not held");
    press(&mut d, Mode::Toggle, 1);
    assert_eq!(d.take_asks(), vec![Ask::Stop { run }], "pressed again, it stops");

    let mut d = dictation();
    press(&mut d, Mode::Hold, 1);
    run_id(&mut d);
    press(&mut d, Mode::Hold, 1);
    press(&mut d, Mode::Hold, 1);
    assert!(d.take_asks().is_empty(), "the key repeating while held");
    assert!(d.run.as_ref().is_some_and(|run| run.live()));
}

#[test]
fn the_model_loading_is_said_until_the_engine_has_the_microphone() {
    let mut d = dictation();
    press(&mut d, Mode::Hold, 1);
    let run = run_id(&mut d);
    assert_eq!(phase(&d), Some(Phase::Loading), "the engine has nothing loaded");
    let alive = |_| true;
    d.heard(Heard::Started { run }, &here(1, &alive));
    assert_eq!(phase(&d), Some(Phase::Listening));

    let mut d = Dictation::new(DictationPreferences::default());
    let up = Engine::Ready { model: Model::Parakeet, backend: "Vulkan".into(), device: "RTX".into(), cpu_fallback: false };
    d.heard(Heard::Machine(machine(FileState::Ready, FileState::Missing, up)), &here(1, &alive));
    press(&mut d, Mode::Hold, 1);
    assert_eq!(phase(&d), Some(Phase::Listening), "a model already up listens at once");
}

#[test]
fn a_press_with_no_model_says_what_to_download_and_starts_nothing() {
    let mut d = Dictation::new(DictationPreferences::default());
    let alive = |_| true;
    d.heard(Heard::Machine(machine(FileState::Missing, FileState::Missing, Engine::Unloaded)), &here(1, &alive));
    press(&mut d, Mode::Hold, 1);
    assert!(d.take_asks().is_empty());
    assert_eq!(phase(&d), Some(Phase::NotSetUp(Problem::NoModel)));
    let page = d.take(&alive).unwrap().panel.unwrap();
    assert_eq!(page.state, "No model downloaded");
    assert_eq!(page.note.as_deref(), Some("Download Parakeet (731 MB) or Nemotron (751 MB) to dictate."));
    assert!(page.opens_settings);

    d.heard(Heard::Machine(machine(FileState::Missing, FileState::Ready, Engine::Unloaded)), &here(1, &alive));
    press(&mut d, Mode::Hold, 1);
    assert_eq!(phase(&d), Some(Phase::NotSetUp(Problem::NotDownloaded(Model::Parakeet))));
    let page = d.take(&alive).unwrap().panel.unwrap();
    assert_eq!(page.note.as_deref(), Some("Download it (731 MB) in Settings, or choose Nemotron, which is."));
}

#[test]
fn a_microphone_the_system_refused_is_a_problem_to_set_up_not_a_failure() {
    let mut d = dictation();
    press(&mut d, Mode::Hold, 1);
    let run = run_id(&mut d);
    let alive = |_| true;
    d.heard(Heard::Failed { run, why: Why::Refused }, &here(1, &alive));
    assert_eq!(phase(&d), Some(Phase::NotSetUp(Problem::Refused)));
    let page = d.take(&alive).unwrap().panel.unwrap();
    assert_eq!((page.state.as_str(), page.tone), ("The system refused the microphone", Tone::Bad));
}

#[test]
fn a_text_field_gets_the_text_typed_and_a_field_gone_holds_it() {
    let mut d = dictation();
    let alive = |_| true;
    d.gesture(DictationGesture::Press { mode: Mode::Hold, field: true }, &here(1, &alive));
    let run = run_id(&mut d);
    assert!(d.heard(Heard::Final { run, text: "a comment".into() }, &here(1, &alive)).is_empty(), "no paste to a pane");
    assert_eq!(d.take_asks(), vec![Ask::Type { run, text: "a comment".into() }]);
    gesture(&mut d, DictationGesture::TargetGone { run, text: "a comment".into() });
    assert_eq!(phase(&d), Some(Phase::Held { text: "a comment".into() }));
}

#[test]
fn enter_after_and_the_trailing_space_are_the_preferences() {
    let mut d = dictation();
    gesture(&mut d, DictationGesture::Set { change: DictationChange::EnterAfter { on: true } });
    gesture(&mut d, DictationGesture::Set { change: DictationChange::TrailingSpace { on: true } });
    assert!(d.take_unsaved());
    press(&mut d, Mode::Hold, 1);
    let run = run_id(&mut d);
    let alive = |_| true;
    let delivered = d.heard(Heard::Final { run, text: "go".into() }, &here(1, &alive));
    assert_eq!(delivered, vec![Deliver::Paste { pane: PaneId(1), text: "go ".into(), enter: true }]);
}

#[test]
fn a_press_with_no_pane_and_no_field_says_so() {
    let mut d = dictation();
    let alive = |_| true;
    let said = d.gesture(DictationGesture::Press { mode: Mode::Toggle, field: false }, &Here { pane: None, alive: &alive });
    assert!(matches!(said.as_slice(), [Deliver::Notice(_)]));
    assert!(d.take_asks().is_empty());
}

#[test]
fn a_window_without_the_engine_is_never_sent_dictation() {
    // The Tauri page has no engine: it never reports a machine, so it never hears an event its
    // bindings do not have.
    let mut d = Dictation::new(DictationPreferences::default());
    let alive = |_| true;
    assert!(d.take(&alive).is_none());
    let said = d.gesture(DictationGesture::Press { mode: Mode::Toggle, field: false }, &here(1, &alive));
    assert!(matches!(said.as_slice(), [Deliver::Notice(_)]));
    assert!(!d.listening_for_keys());
}

#[test]
fn turning_dictation_off_mid_run_discards_it() {
    let mut d = dictation();
    press(&mut d, Mode::Toggle, 1);
    let run = run_id(&mut d);
    gesture(&mut d, DictationGesture::Set { change: DictationChange::Enabled { on: false } });
    assert_eq!(d.take_asks(), vec![Ask::Cancel { run }]);
    let alive = |_| true;
    assert!(d.take(&alive).unwrap().chip.is_none(), "no chip while off");
    assert!(!d.listening_for_keys());
}

#[test]
fn a_refused_choice_is_said_and_changes_nothing() {
    let mut d = dictation();
    let leader = super::hold::Hold { shift: false, ..super::hold::Hold::default() };
    let said = gesture(&mut d, DictationGesture::Set { change: DictationChange::Hold { hold: leader } });
    assert_eq!(said, vec![Deliver::Notice("Ctrl Space is the leader".into())]);
    assert!(!d.take_unsaved());
}

#[test]
fn the_panel_names_the_target_and_the_keys_that_answer_it() {
    let mut d = dictation();
    press(&mut d, Mode::Hold, 1);
    let run = run_id(&mut d);
    let alive = |_| true;
    d.heard(Heard::Started { run }, &here(1, &alive));
    let page = d.take(&alive).unwrap();
    let panel = page.panel.unwrap();
    assert_eq!(panel.state, "Listening");
    assert_eq!(panel.target, Some(("to".into(), "pane 1".into())));
    assert_eq!(panel.meter, Some(Meter::Live));
    assert_eq!(panel.placeholder.as_deref(), Some("Start speaking\u{2026}"));
    assert_eq!(panel.keys[0], ("release".into(), "paste".into()));
    assert_eq!(panel.keys[2], ("Ctrl Shift Space".into(), "held".into()));
    assert_eq!(page.chip.unwrap().words, "Listening");
    assert!(d.take(&alive).is_none(), "heard once");
    d.heard(Heard::Text { run, committed: "so the".into(), tentative: " set".into() }, &here(1, &alive));
    let panel = d.take(&alive).unwrap().panel.unwrap();
    assert_eq!((panel.committed.as_str(), panel.tentative.as_str(), panel.placeholder), ("so the", " set", None));
}

#[test]
fn the_toggle_is_written_as_the_leader_and_its_key() {
    assert_eq!(toggle_written(), "Ctrl Space \u{b7} V");
}
