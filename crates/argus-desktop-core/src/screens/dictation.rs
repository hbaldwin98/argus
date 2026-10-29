//! The scenes of dictation: the panel in each state the design page draws (loading, listening,
//! finishing, pasted, the pane closed, not set up, a microphone refused), and the Settings section
//! with one model ready and the other downloading.
//!
//! Only the GPUI window has an engine to report a machine with, so only it draws these, and the
//! page's `write_screens` passes them over.

use argus_protocol::{PaneId, ServerMsg};

use super::{scene, tree, Scene, Window, P_TRIM};
use crate::connection::page::Call;
use crate::dictation::machine::{Engine, FileState, Machine, Microphone, ModelFile};
use crate::dictation::model::Model;
use crate::dictation::run::Mode;
use crate::dictation::{DictationGesture, Heard, Why};

/// This machine as the design page draws it: Parakeet downloaded and up on the GPU, Nemotron 41%
/// of the way down, two microphones.
fn machine(parakeet: FileState, engine: Engine) -> Machine {
    let nemotron = 751_094_240;
    Machine {
        files: vec![
            ModelFile { model: Model::Parakeet, size: 731_357_568, state: parakeet },
            ModelFile { model: Model::Nemotron, size: nemotron, state: FileState::Downloading { done: nemotron * 41 / 100, total: nemotron } },
        ],
        engine,
        microphones: vec![
            Microphone { id: "wasapi:{0.0.1.00000000}.{headset}".into(), name: "Headset Microphone (Arctis 7)".into() },
            Microphone { id: "wasapi:{0.0.1.00000000}.{array}".into(), name: "Microphone Array (Realtek)".into() },
        ],
        default_microphone: Some("Headset Microphone (Arctis 7)".into()),
    }
}

fn up() -> Engine {
    Engine::Ready { model: Model::Parakeet, backend: "Vulkan".into(), device: "NVIDIA GeForce RTX 4070".into(), cpu_fallback: false }
}

/// The Workspace on the claude pane, in a window whose engine reported `machine`.
fn reported(machine: Machine) -> Window {
    let mut w = Window::showing(P_TRIM);
    w.s.dictation_heard(Heard::Machine(machine));
    w
}

fn gesture(w: &mut Window, gesture: DictationGesture) {
    w.call(Call::Dictation { gesture });
}

/// Push-to-talk held on the claude pane with the model up: run 1.
fn holding() -> Window {
    let mut w = reported(machine(FileState::Ready, up()));
    gesture(&mut w, DictationGesture::Press { mode: Mode::Hold, field: false });
    w.s.dictation_heard(Heard::Started { run: 1 });
    w
}

/// `holding`, a sentence under way.
fn speaking() -> Window {
    let mut w = holding();
    w.s.dictation_heard(Heard::Text { run: 1, committed: "so the settings section should list both".into(), tentative: " models and".into() });
    w
}

pub fn scenes() -> Vec<Scene> {
    vec![
        scene("dictation-listening", "Dictation — listening, the words so far", || speaking().s),
        scene("dictation-loading", "Dictation — the model loading at the first press", || {
            let mut w = reported(machine(FileState::Ready, Engine::Unloaded));
            gesture(&mut w, DictationGesture::Press { mode: Mode::Hold, field: false });
            w.s
        }),
        scene("dictation-finishing", "Dictation — let go, the transcript finishing", || {
            let mut w = speaking();
            gesture(&mut w, DictationGesture::Release);
            w.s
        }),
        scene("dictation-pasted", "Dictation — pasted into the pane", || {
            let mut w = speaking();
            gesture(&mut w, DictationGesture::Release);
            w.s.dictation_heard(Heard::Final { run: 1, text: "So the settings section should list both models and the device picker.".into() });
            w.s
        }),
        scene("dictation-held", "Dictation — the pane closed while it was spoken", || {
            let mut w = speaking();
            gesture(&mut w, DictationGesture::Release);
            let mut without = tree();
            for checkout in without.iter_mut().flat_map(|p| &mut p.repositories).flat_map(|r| &mut r.checkouts) {
                checkout.panes.retain(|pane| pane.id != PaneId(P_TRIM));
            }
            w.receive(ServerMsg::Tree(without));
            w.s.dictation_heard(Heard::Final { run: 1, text: "Rename the toggle to remove filler words.".into() });
            w.s
        }),
        scene("dictation-not-set-up", "Dictation — no model downloaded", || {
            let mut nothing = machine(FileState::Missing, Engine::Unloaded);
            for file in &mut nothing.files {
                file.state = FileState::Missing;
            }
            let mut w = reported(nothing);
            gesture(&mut w, DictationGesture::Press { mode: Mode::Hold, field: false });
            w.s
        }),
        scene("dictation-refused", "Dictation — the system refused the microphone", || {
            let mut w = holding();
            w.s.dictation_heard(Heard::Failed { run: 1, why: Why::Refused });
            w.s
        }),
        scene("dictation-settings", "Dictation — its Settings section, one model ready and one downloading", || reported(machine(FileState::Ready, up())).s),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::page::Event;
    use crate::screens::NOW;

    fn panel(name: &str) -> Option<crate::dictation::panel::PanelPage> {
        let scene = scenes().into_iter().find(|scene| scene.name == name).expect(name);
        let events = (scene.build)().everything(NOW);
        let page = events.into_iter().find_map(|event| if let Event::Dictation(page) = event { Some(*page) } else { None }).expect("dictation is sent");
        page.panel
    }

    #[test]
    fn each_scene_draws_the_state_it_is_named_for() {
        let state = |name: &str| panel(name).map(|panel| panel.state);
        assert_eq!(state("dictation-listening").as_deref(), Some("Listening"));
        assert_eq!(state("dictation-loading").as_deref(), Some("Loading Parakeet\u{2026}"));
        assert_eq!(state("dictation-finishing").as_deref(), Some("Finishing"));
        assert_eq!(state("dictation-pasted").as_deref(), Some("Pasted"));
        assert_eq!(state("dictation-held").as_deref(), Some("The pane closed"));
        assert_eq!(state("dictation-not-set-up").as_deref(), Some("No model downloaded"));
        assert_eq!(state("dictation-refused").as_deref(), Some("The system refused the microphone"));
        assert_eq!(state("dictation-settings"), None, "no run: the section alone");
    }
}
