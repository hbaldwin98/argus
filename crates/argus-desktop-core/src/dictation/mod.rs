//! Dictation as the session holds it: the preferences, what this machine offers, the one run at a
//! time, and what goes to the window's engine and comes back from it.
//!
//! The engine, the microphone and the model files are the window's (`argus-dictation`, which this
//! crate never depends on): it reports the machine and the run's progress here, and takes the
//! asks this queues. Only text crosses to the daemon, as the paste it already handles, so a
//! window on another machine's daemon dictates as one on this machine's does.

pub mod hold;
pub mod machine;
pub mod model;
pub mod panel;
pub mod preferences;
pub mod run;
pub mod section;

use argus_protocol::PaneId;

use crate::connection::sent::Sent;
use crate::keys::chords::{press, KeyChord};
use crate::keys::keymap::{Action, Place, BINDINGS, LEADER};
use machine::{Engine, Machine};
use model::Model;
use panel::{ChipPage, Keys, PanelPage};
use preferences::{DictationChange, DictationPreferences, Device};
use run::{Mode, Phase, Problem, Run, Target};
use section::SectionPage;

/// Everything the window draws of dictation, whole, whenever it changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DictationPage {
    pub section: SectionPage,
    /// None while dictation is off.
    pub chip: Option<ChipPage>,
    /// None while no run has anything to say.
    pub panel: Option<PanelPage>,
}

/// What the person does to dictation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictationGesture {
    Set { change: DictationChange },
    /// The push-to-talk chord went down, or the toggle was asked for. `field` is whether a text
    /// field of the window's has the keys, which the window knows and the session does not.
    Press { mode: Mode, field: bool },
    /// The push-to-talk chord let go.
    Release,
    Escape,
    /// Held text into the pane that has the keys now.
    PasteHeld,
    CopyHeld,
    /// A finished run's panel has said its piece.
    Dismiss { run: u64 },
    Download { model: Model },
    CancelDownload { model: Model },
    Delete { model: Model },
    /// The field locked at the press could not be typed into.
    TargetGone { run: u64, text: String },
}

/// Why the engine could not hear a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    NoMicrophone,
    Refused,
    NoGpu,
    Failed(String),
}

/// What the window's engine reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    Machine(Machine),
    /// The microphone is open and the model up.
    Started { run: u64 },
    /// The text so far, cleaned as it will be pasted.
    Text { run: u64, committed: String, tentative: String },
    Final { run: u64, text: String },
    Failed { run: u64, why: Why },
}

/// What the session asks of the window's engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ask {
    Listen { run: u64, model: Model, language: Option<String>, microphone: Option<String>, device: Device, vad: bool, remove_fillers: bool },
    Stop { run: u64 },
    Cancel { run: u64 },
    /// Type the run's text into the field that had the keys at its press.
    Type { run: u64, text: String },
    Download { model: Model },
    CancelDownload { model: Model },
    Delete { model: Model },
}

/// What the session does for a run: the daemon's paste, the clipboard, a notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deliver {
    Paste { pane: PaneId, text: String, enter: bool },
    Copy(String),
    Notice(String),
}

/// Where the keys are, as the session sees it when a gesture or a report arrives.
pub struct Here<'a> {
    /// The pane typing goes to now, and its name.
    pub pane: Option<(PaneId, String)>,
    /// Whether a pane is still open.
    pub alive: &'a dyn Fn(PaneId) -> bool,
}

pub struct Dictation {
    chosen: DictationPreferences,
    unsaved: bool,
    /// None until the window reports one: a window without the engine never does.
    machine: Option<Machine>,
    run: Option<Run>,
    runs: u64,
    asks: Vec<Ask>,
    sent: Sent<DictationPage>,
}

/// The toggle's keys as the section and the panel write them: `Ctrl Space · V`.
pub fn toggle_written() -> String {
    let chord = BINDINGS.iter().find(|binding| binding.place == Place::Pane && binding.action == Action::Dictate).and_then(|binding| binding.chords.first());
    let leader = press(&KeyChord::from(&LEADER));
    match chord {
        Some(chord) => format!("{leader} \u{b7} {}", press(&KeyChord { leader: false, ..KeyChord::from(chord) }).to_uppercase()),
        None => leader,
    }
}

impl Dictation {
    pub fn new(chosen: DictationPreferences) -> Self {
        Dictation { chosen, unsaved: false, machine: None, run: None, runs: 0, asks: Vec::new(), sent: Sent::default() }
    }

    pub fn chosen(&self) -> &DictationPreferences {
        &self.chosen
    }

    /// Whether they changed since last asked, so they are saved.
    pub fn take_unsaved(&mut self) -> bool {
        std::mem::take(&mut self.unsaved)
    }

    pub fn take_asks(&mut self) -> Vec<Ask> {
        std::mem::take(&mut self.asks)
    }

    /// A page that has just loaded holds nothing: it hears it again.
    pub fn resend(&mut self) {
        self.sent.forget();
    }

    /// Whether the person's keys are dictation's to hear: on, and a window with the engine.
    pub fn listening_for_keys(&self) -> bool {
        self.chosen.enabled && self.machine.is_some()
    }

    pub fn gesture(&mut self, gesture: DictationGesture, here: &Here) -> Vec<Deliver> {
        match gesture {
            DictationGesture::Set { change } => return self.set(change),
            DictationGesture::Press { mode, field } => return self.press(mode, field, here),
            DictationGesture::Release => {
                if self.run.as_ref().is_some_and(|run| run.live() && run.mode == Mode::Hold) {
                    self.stop();
                }
            }
            DictationGesture::Escape => {
                if let Some(run) = self.run.take() {
                    if run.open() {
                        self.asks.push(Ask::Cancel { run: run.id });
                    }
                }
            }
            DictationGesture::PasteHeld => return self.paste_held(here),
            DictationGesture::CopyHeld => {
                if let Some(text) = self.run.as_ref().and_then(Run::held).map(str::to_owned) {
                    self.run = None;
                    return vec![Deliver::Copy(text)];
                }
            }
            DictationGesture::Dismiss { run } => {
                if self.run.as_ref().is_some_and(|now| now.id == run && !now.open() && now.held().is_none()) {
                    self.run = None;
                }
            }
            DictationGesture::Download { model } => self.asks.push(Ask::Download { model }),
            DictationGesture::CancelDownload { model } => self.asks.push(Ask::CancelDownload { model }),
            DictationGesture::Delete { model } => self.asks.push(Ask::Delete { model }),
            DictationGesture::TargetGone { run, text } => {
                if let Some(now) = self.run.as_mut().filter(|now| now.id == run) {
                    now.phase = Phase::Held { text };
                }
            }
        }
        Vec::new()
    }

    pub fn heard(&mut self, heard: Heard, here: &Here) -> Vec<Deliver> {
        let ours = |run: u64, now: &Option<Run>| now.as_ref().is_some_and(|now| now.id == run && now.open());
        match heard {
            Heard::Machine(machine) => self.machine = Some(machine),
            Heard::Started { run } if ours(run, &self.run) => {
                if let Some(now) = self.run.as_mut().filter(|now| now.phase == Phase::Loading) {
                    now.phase = Phase::Listening;
                }
            }
            Heard::Text { run, committed, tentative } if ours(run, &self.run) => {
                if let Some(now) = self.run.as_mut() {
                    (now.committed, now.tentative) = (committed, tentative);
                }
            }
            Heard::Final { run, text } if ours(run, &self.run) => return self.finished(text, here),
            Heard::Failed { run, why } if ours(run, &self.run) => {
                if let Some(now) = self.run.as_mut() {
                    now.phase = match why {
                        Why::NoMicrophone => Phase::NotSetUp(Problem::NoMicrophone),
                        Why::Refused => Phase::NotSetUp(Problem::Refused),
                        Why::NoGpu => Phase::NotSetUp(Problem::NoGpu),
                        Why::Failed(reason) => Phase::Failed { reason },
                    };
                }
            }
            // A report for a run that was discarded or replaced.
            _ => {}
        }
        Vec::new()
    }

    fn set(&mut self, change: DictationChange) -> Vec<Deliver> {
        let before = self.chosen.clone();
        if let Err(why) = self.chosen.change(change) {
            return vec![Deliver::Notice(why)];
        }
        if self.chosen != before {
            self.unsaved = true;
        }
        if !self.chosen.enabled {
            if let Some(run) = self.run.take().filter(Run::open) {
                self.asks.push(Ask::Cancel { run: run.id });
            }
        }
        Vec::new()
    }

    fn press(&mut self, mode: Mode, field: bool, here: &Here) -> Vec<Deliver> {
        let model = self.chosen.model;
        let Some(machine) = &self.machine else {
            return vec![Deliver::Notice("this window has no dictation: it is argus-desktop-next's".into())];
        };
        let (ready, any_ready) = (machine.ready(model), model::MODELS.iter().any(|other| machine.ready(*other)));
        let loaded = matches!(&machine.engine, Engine::Ready { model: up, .. } if *up == model);
        if let Some(run) = &self.run {
            // The chord held repeats; a second press of either ends a run that is live.
            let (live, repeat) = (run.live(), mode == Mode::Hold && run.mode == Mode::Hold);
            // Finishing, or text held for the person to place: a press must not lose it.
            let busy = run.open() || run.held().is_some();
            if live && !repeat {
                self.stop();
            }
            if busy {
                return Vec::new();
            }
        }
        self.runs += 1;
        let id = self.runs;
        let problem = if !self.chosen.enabled {
            Some(Problem::Off)
        } else if ready {
            None
        } else if any_ready {
            Some(Problem::NotDownloaded(model))
        } else {
            Some(Problem::NoModel)
        };
        if let Some(problem) = problem {
            self.run = Some(Run::not_set_up(id, mode, model, problem));
            return Vec::new();
        }
        let target = match (&here.pane, field) {
            (_, true) => Target::Field,
            (Some((pane, name)), false) => Target::Pane { pane: *pane, name: name.clone() },
            (None, false) => {
                self.run = None;
                return vec![Deliver::Notice("nothing has the keys to dictate into: show a pane first".into())];
            }
        };
        self.run = Some(Run::started(id, mode, target, model, loaded));
        self.asks.push(Ask::Listen {
            run: id,
            model,
            language: model::listens_for(model, &self.chosen.language).map(str::to_owned),
            microphone: self.chosen.microphone.clone(),
            device: self.chosen.device,
            vad: self.chosen.vad,
            remove_fillers: self.chosen.remove_fillers,
        });
        Vec::new()
    }

    fn stop(&mut self) {
        if let Some(run) = self.run.as_mut().filter(|run| run.live()) {
            run.phase = Phase::Finishing;
            self.asks.push(Ask::Stop { run: run.id });
        }
    }

    /// The run's final text: into its pane if it is still open, else held for the person.
    fn finished(&mut self, text: String, here: &Here) -> Vec<Deliver> {
        let Some(run) = self.run.as_mut() else { return Vec::new() };
        let text = text.trim().to_owned();
        if text.is_empty() {
            run.phase = Phase::Empty;
            return Vec::new();
        }
        let text = if self.chosen.trailing_space { format!("{text} ") } else { text };
        match &run.target {
            Some(Target::Pane { pane, .. }) if (here.alive)(*pane) => {
                let pane = *pane;
                run.phase = Phase::Pasted { text: text.clone() };
                vec![Deliver::Paste { pane, text, enter: self.chosen.enter_after }]
            }
            Some(Target::Field) => {
                self.asks.push(Ask::Type { run: run.id, text: text.clone() });
                run.phase = Phase::Pasted { text };
                Vec::new()
            }
            _ => {
                run.phase = Phase::Held { text };
                Vec::new()
            }
        }
    }

    fn paste_held(&mut self, here: &Here) -> Vec<Deliver> {
        let Some(run) = self.run.as_mut() else { return Vec::new() };
        let Some(text) = run.held().map(str::to_owned) else { return Vec::new() };
        let Some((pane, name)) = here.pane.clone() else {
            return vec![Deliver::Notice("no pane has the keys to paste into".into())];
        };
        run.target = Some(Target::Pane { pane, name });
        run.phase = Phase::Pasted { text: text.clone() };
        vec![Deliver::Paste { pane, text, enter: self.chosen.enter_after }]
    }

    /// What the window has not heard of dictation, if it has an engine to hear it with.
    pub fn take(&mut self, alive: &dyn Fn(PaneId) -> bool) -> Option<DictationPage> {
        let page = self.machine.as_ref().map(|machine| {
            let hold = self.chosen.hold.written();
            let toggle = toggle_written();
            let keys = Keys { hold: &hold, toggle: &toggle };
            let panel = self.run.as_ref().map(|run| {
                let alive = match &run.target {
                    Some(Target::Pane { pane, .. }) => alive(*pane),
                    _ => true,
                };
                panel::panel(run, machine, &keys, alive)
            });
            DictationPage {
                chip: self.chosen.enabled.then(|| panel::chip(&self.chosen, machine, self.run.as_ref())),
                panel,
                section: section::section(&self.chosen, machine, hold.clone(), toggle.clone()),
            }
        });
        self.sent.take(page)
    }
}

#[cfg(test)]
mod tests;
