//! The speech engine on a thread of its own: transcribe.cpp (the library Handy runs) loaded as the
//! window asks, one run at a time through it, and the model let go after ten idle minutes.
//!
//! The window talks to it with [`Ask`]s and hears [`Said`]s on whatever thread the engine is on, so
//! inference never stalls the window. A load waits for nothing the window does: audio that arrives
//! while the model is still loading is queued and fed once it is up, so the first words are kept.

pub mod load;
pub mod run;

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use load::{load, Config, Loaded};

/// learn-harness's owner's choice: the model goes after ten minutes unused.
pub const IDLE_UNLOAD: Duration = Duration::from_secs(10 * 60);
/// How often idleness is looked at: Handy's idle watcher's cadence.
const IDLE_CHECK: Duration = Duration::from_secs(10);

/// One run's beginning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub run: u64,
    /// The language to transcribe; none lets a multilingual model hear which (transcribe.cpp rejects
    /// the literal "auto", and leaving the language out is its auto slot).
    pub language: Option<String>,
    /// Whether the Silero gate decides what the model hears.
    pub gate: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// What to load, or nothing: loads at once, so the first run is warm.
    Configure(Option<Config>),
    Start(Start),
    Audio { run: u64, pcm: Vec<f32> },
    /// The speaker is done: finish and say the final text.
    Stop { run: u64 },
    /// Drop the run unsaid.
    Cancel { run: u64 },
}

/// Whether a model is ready, and on what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Nothing is configured.
    None,
    Loading,
    Ready {
        backend: String,
        device: String,
        /// Auto lost the GPU this session and runs on the CPU.
        cpu_fallback: bool,
    },
    /// Let go after the idle time; the next run loads it again.
    Idle,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Status(Status),
    /// The run's stream began.
    Started { run: u64 },
    /// The text so far: what will not change, and the model's current guess after it.
    Text { run: u64, committed: String, tentative: String },
    /// The whole transcript, trimmed, and the language the model heard when it was not told one.
    Final { run: u64, text: String, language: Option<String> },
    Failed { run: u64, reason: String },
}

/// The engine's handle: dropping the last of its clones ends the thread and lets the model go.
#[derive(Clone)]
pub struct Engine {
    asks: Sender<Ask>,
}

impl Engine {
    pub fn spawn(say: impl Fn(Said) + Send + 'static) -> std::io::Result<Engine> {
        // ggml writes every buffer it schedules to stderr unless told otherwise; through `log` it
        // reaches whatever the window installed, or nothing. Set once, before any model loads.
        static QUIET: std::sync::Once = std::sync::Once::new();
        QUIET.call_once(transcribe_cpp::init_logging);
        let (asks, heard) = mpsc::channel();
        std::thread::Builder::new().name("dictation-engine".into()).spawn(move || {
            Worker { asks: heard, say: Box::new(say), wanted: None, loaded: None, gpu_lost: false, used: Instant::now(), failure: None }.work();
        })?;
        Ok(Engine { asks })
    }

    pub fn ask(&self, ask: Ask) {
        // A closed channel is an engine whose thread is gone; there is nobody to ask.
        let _ = self.asks.send(ask);
    }
}

struct Worker {
    asks: Receiver<Ask>,
    say: Box<dyn Fn(Said) + Send>,
    wanted: Option<Config>,
    loaded: Option<Loaded>,
    gpu_lost: bool,
    used: Instant,
    /// Why the last load failed, for the run that asked for it.
    failure: Option<String>,
}

impl Worker {
    fn work(mut self) {
        let mut next = None;
        loop {
            let ask = match next.take() {
                Some(ask) => ask,
                None => match self.asks.recv_timeout(IDLE_CHECK) {
                    Ok(ask) => ask,
                    Err(RecvTimeoutError::Timeout) => {
                        self.idle();
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                },
            };
            next = self.handle(ask);
        }
    }

    fn handle(&mut self, ask: Ask) -> Option<Ask> {
        match ask {
            Ask::Configure(config) => {
                // The same thing asked again is not a reason to load an idle model.
                if config != self.wanted {
                    self.wanted = config;
                    self.ensure_loaded();
                }
                None
            }
            Ask::Start(start) => {
                self.used = Instant::now();
                if !self.ensure_loaded() {
                    let reason = match &self.wanted {
                        None => "No model is set up for dictation".to_owned(),
                        Some(_) => self.failure.take().unwrap_or_else(|| "The model did not load".into()),
                    };
                    (self.say)(Said::Failed { run: start.run, reason });
                    return None;
                }
                let loaded = self.loaded.as_mut().expect("loaded");
                let next = run::serve(loaded, &start, &self.asks, &*self.say);
                self.used = Instant::now();
                next
            }
            // What a run that never began, or has ended, is still sent.
            Ask::Audio { .. } | Ask::Stop { .. } | Ask::Cancel { .. } => None,
        }
    }

    /// Loads what is wanted unless it is loaded; whether a model is ready.
    fn ensure_loaded(&mut self) -> bool {
        let Some(wanted) = self.wanted.clone() else {
            if self.loaded.take().is_some() {
                tracing::info!("dictation: model unloaded");
            }
            (self.say)(Said::Status(Status::None));
            return false;
        };
        if self.loaded.as_ref().is_some_and(|loaded| loaded.config == wanted) {
            return true;
        }
        self.loaded = None;
        (self.say)(Said::Status(Status::Loading));
        let began = Instant::now();
        match load(&wanted, &mut self.gpu_lost) {
            Ok(loaded) => {
                tracing::info!("dictation: loaded {} on {} ({}) in {} ms", wanted.model.display(), loaded.backend, loaded.device, began.elapsed().as_millis());
                let ready = Status::Ready { backend: loaded.backend.clone(), device: loaded.device.clone(), cpu_fallback: self.gpu_lost && wanted.device == load::Device::Auto };
                self.loaded = Some(loaded);
                self.failure = None;
                self.used = Instant::now();
                (self.say)(Said::Status(ready));
                true
            }
            Err(reason) => {
                tracing::warn!("dictation: loading {} failed: {reason}", wanted.model.display());
                self.failure = Some(reason.clone());
                (self.say)(Said::Status(Status::Failed(reason)));
                false
            }
        }
    }

    fn idle(&mut self) {
        if self.loaded.is_some() && self.used.elapsed() >= IDLE_UNLOAD {
            tracing::info!("dictation: idle, unloading the model");
            self.loaded = None;
            (self.say)(Said::Status(Status::Idle));
        }
    }
}
