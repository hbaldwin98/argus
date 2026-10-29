//! The engine behind the window's dictation, in a build with it: `argus-dictation`'s microphone,
//! model files and transcribe.cpp, driven by what the session asks and reporting back what they do.
//!
//! One thread holds all of it and hears one queue: the session's asks, the engine's words, and the
//! downloads' progress. So a run's microphone, its engine run and its text are only ever changed in
//! one order. Text goes back cleaned (`argus_dictation::text`), since what the session pastes and
//! the panel shows are the cleaned words; typing into a field is the window's own, on its thread.

pub mod files;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use argus_desktop_core::dictation::machine::{Engine as EngineState, FileState, Machine, Microphone};
use argus_desktop_core::dictation::model::Model;
use argus_desktop_core::dictation::preferences::Device;
use argus_desktop_core::dictation::{Ask, Heard, Why};
use argus_dictation::capture::devices::{default_microphone, microphones};
use argus_dictation::capture::stream::{self, Capture, Level, Refusal};
use argus_dictation::engine::load::{self, Config, NO_GPU};
use argus_dictation::engine::{self, Engine, Said, Start, Status};
use argus_dictation::models::fetch::Fetched;
use argus_dictation::text::{clean, live, Clean};
use gpui_kit::{AnyWindowHandle, App};

use crate::connection::owner::Owner;
use files::Files;

/// How often the microphones are listed again while nothing else happens: one plugged in shows in
/// the Settings sheet within this.
const RELIST: Duration = Duration::from_secs(30);

/// What the service's thread hears.
pub enum Job {
    Ask(Ask),
    Said(Said),
    Progress { model: Model, done: u64 },
    Fetched { model: Model, ended: Result<Fetched, String> },
}

/// The run being heard: how its text is cleaned.
struct Run {
    id: u64,
    remove_fillers: bool,
    language: Option<String>,
}

struct Service {
    owner: Arc<Owner>,
    jobs: Sender<Job>,
    engine: Engine,
    files: Files,
    machine: Machine,
    /// The model the engine was last told to load.
    configured: Option<Model>,
    capture: Option<Capture>,
    run: Option<Run>,
    level: Arc<AtomicU32>,
}

/// Starts the service for `owner`'s session and the window `window` types into.
pub fn install(owner: Arc<Owner>, window: AnyWindowHandle, cx: &mut App) {
    let level = super::level::Level::of(cx).cell();
    let (jobs, heard) = mpsc::channel();
    let (typing, mut typed) = tokio::sync::mpsc::unbounded_channel::<(u64, String)>();
    let asks = jobs.clone();
    owner.on_dictation(move |ask| match ask {
        Ask::Type { run, text } => {
            let _ = typing.send((run, text));
        }
        ask => {
            let _ = asks.send(Job::Ask(ask));
        }
    });
    let said = jobs.clone();
    let engine = match Engine::spawn(move |words| {
        let _ = said.send(Job::Said(words));
    }) {
        Ok(engine) => engine,
        Err(err) => {
            tracing::warn!("dictation: the engine did not start: {err}");
            return;
        }
    };
    let service = Service { owner, jobs, engine, files: Files::new(files::folder()), machine: Machine::default(), configured: None, capture: None, run: None, level };
    if let Err(err) = std::thread::Builder::new().name("dictation".into()).spawn(move || service.serve(heard)) {
        tracing::warn!("dictation: the service did not start: {err}");
        return;
    }
    // A field's text is typed on the window's thread, where the keys are.
    cx.spawn(async move |cx| {
        while let Some((run, text)) = typed.recv().await {
            let _ = window.update(cx, |_, window, cx| {
                if let Some(keys) = super::keys::DictationKeys::get(cx) {
                    keys.type_into(run, &text, window, cx);
                }
            });
        }
    })
    .detach();
}

impl Service {
    fn serve(mut self, jobs: Receiver<Job>) {
        self.list_microphones();
        self.machine.files = self.files.look();
        self.report();
        loop {
            match jobs.recv_timeout(RELIST) {
                Ok(job) => self.take(job),
                Err(RecvTimeoutError::Timeout) => {
                    if self.list_microphones() {
                        self.report();
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn heard(&self, heard: Heard) {
        self.owner.act(|session| session.dictation_heard(heard));
    }

    fn report(&self) {
        self.heard(Heard::Machine(self.machine.clone()));
    }

    /// Whether the list changed.
    fn list_microphones(&mut self) -> bool {
        let listed: Vec<Microphone> = microphones().into_iter().map(|mic| Microphone { id: mic.id, name: mic.name }).collect();
        let default = default_microphone();
        let changed = listed != self.machine.microphones || default != self.machine.default_microphone;
        (self.machine.microphones, self.machine.default_microphone) = (listed, default);
        changed
    }

    fn take(&mut self, job: Job) {
        match job {
            Job::Ask(ask) => self.ask(ask),
            Job::Said(said) => self.said(said),
            Job::Progress { model, done } => {
                self.set_file(model, FileState::Downloading { done, total: files::entry(model).entry.size });
            }
            Job::Fetched { model, ended } => {
                self.files.ended(model);
                let state = match ended {
                    Ok(_) => self.files.look().into_iter().find(|file| file.model == model).map_or(FileState::Missing, |file| file.state),
                    Err(reason) => FileState::Failed { reason },
                };
                self.set_file(model, state);
            }
        }
    }

    fn set_file(&mut self, model: Model, state: FileState) {
        if let Some(file) = self.machine.files.iter_mut().find(|file| file.model == model) {
            file.state = state;
        }
        self.report();
    }

    fn ask(&mut self, ask: Ask) {
        match ask {
            Ask::Listen { run, model, language, microphone, device, vad, remove_fillers } => {
                let catalog = files::entry(model);
                let gate = self.files.vad().filter(|_| vad);
                let config = Config {
                    model: self.files.path(model),
                    shape: catalog.shape,
                    vad: gate.clone(),
                    device: match device {
                        Device::Auto => load::Device::Auto,
                        Device::Gpu => load::Device::Gpu,
                        Device::Cpu => load::Device::Cpu,
                    },
                };
                self.configured = Some(model);
                self.engine.ask(engine::Ask::Configure(Some(config)));
                self.engine.ask(engine::Ask::Start(Start { run, language: language.clone(), gate: gate.is_some() }));
                self.run = Some(Run { id: run, remove_fillers, language });
                let feeding = self.engine.clone();
                match stream::start(microphone.as_deref(), Level::shared(self.level.clone()), move |pcm| feeding.ask(engine::Ask::Audio { run, pcm })) {
                    Ok(capture) => self.capture = Some(capture),
                    Err(refusal) => {
                        self.engine.ask(engine::Ask::Cancel { run });
                        self.run = None;
                        let why = match refusal {
                            Refusal::NoMicrophone => Why::NoMicrophone,
                            Refusal::Denied => Why::Refused,
                            Refusal::Failed(reason) => Why::Failed(format!("The microphone would not open: {reason}")),
                        };
                        self.heard(Heard::Failed { run, why });
                    }
                }
            }
            // The microphone closes first, so every chunk it gave is queued before the stop.
            Ask::Stop { run } => {
                self.close_microphone();
                self.engine.ask(engine::Ask::Stop { run });
            }
            Ask::Cancel { run } => {
                self.close_microphone();
                self.engine.ask(engine::Ask::Cancel { run });
                self.run = None;
            }
            Ask::Download { model } => {
                self.files.download(model, self.jobs.clone());
                let at = self.machine.files.iter().find(|file| file.model == model).map(|file| file.state.clone());
                let done = match at {
                    Some(FileState::Partial { done, .. }) => done,
                    _ => 0,
                };
                self.set_file(model, FileState::Downloading { done, total: files::entry(model).entry.size });
            }
            Ask::CancelDownload { model } => self.files.cancel(model),
            Ask::Delete { model } => {
                if self.configured == Some(model) {
                    self.engine.ask(engine::Ask::Configure(None));
                    self.configured = None;
                }
                let state = match self.files.delete(model) {
                    Ok(()) => FileState::Missing,
                    Err(reason) => FileState::Failed { reason },
                };
                self.set_file(model, state);
            }
            // The window's own (`install`); never queued here.
            Ask::Type { .. } => {}
        }
    }

    fn close_microphone(&mut self) {
        self.capture = None;
        self.level.store(0f32.to_bits(), Ordering::Relaxed);
    }

    fn said(&mut self, said: Said) {
        match said {
            Said::Status(status) => {
                self.machine.engine = match (status, self.configured) {
                    (Status::Loading, Some(model)) => EngineState::Loading { model },
                    (Status::Ready { backend, device, cpu_fallback }, Some(model)) => EngineState::Ready { model, backend, device, cpu_fallback },
                    (Status::Failed(reason), _) => EngineState::Failed { reason },
                    _ => EngineState::Unloaded,
                };
                self.report();
            }
            Said::Started { run } => self.heard(Heard::Started { run }),
            Said::Text { run, committed, tentative } => {
                let Some(now) = self.run.as_ref().filter(|now| now.id == run) else { return };
                let (committed, tentative) = live(&committed, &tentative, now.remove_fillers, now.language.as_deref());
                self.heard(Heard::Text { run, committed, tentative });
            }
            Said::Final { run, text, language } => {
                let Some(now) = self.run.take().filter(|now| now.id == run) else { return };
                let language = now.language.or(language);
                let text = clean(&text, Clean { remove_fillers: now.remove_fillers, language: language.as_deref(), final_text: true });
                self.heard(Heard::Final { run, text });
            }
            Said::Failed { run, reason } => {
                self.close_microphone();
                self.run = None;
                let why = if reason == NO_GPU { Why::NoGpu } else { Why::Failed(reason) };
                self.heard(Heard::Failed { run, why });
            }
        }
    }
}
