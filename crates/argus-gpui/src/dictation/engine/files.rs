//! The model files as the service holds them: where they are kept, where each stands, and the
//! downloads under way, each on a thread of its own and stopped by a flag.
//!
//! Silero rides with the first model fetched: a model is never ready to dictate with until the gate
//! beside it is too, and it is too small (1.2 MB) to be a choice of its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use argus_desktop_core::dictation::machine::{FileState, ModelFile};
use argus_desktop_core::dictation::model::{Model, MODELS};
use argus_dictation::models::catalog::{self, Entry, VAD};
use argus_dictation::models::fetch::{fetch, Fetched, Https};
use argus_dictation::models::store::{self, OnDisk};

use super::Job;

/// How often a download's progress is said: learn-harness's throttle.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// Where the files are kept: beside the window's other files, in the instance's own folder, unless
/// `ARGUS_DICTATION_MODELS` names one (a test's, or a second instance sharing the first's models).
pub fn folder() -> PathBuf {
    std::env::var_os("ARGUS_DICTATION_MODELS").map(PathBuf::from).unwrap_or_else(|| argus_protocol::config_dir().join("dictation"))
}

/// The catalog's entry for a model the session names.
pub fn entry(model: Model) -> catalog::Model {
    catalog::model(model.id()).expect("every model the session offers is in the catalog")
}

pub struct Files {
    pub dir: PathBuf,
    downloading: HashMap<Model, Arc<AtomicBool>>,
}

impl Files {
    pub fn new(dir: PathBuf) -> Self {
        Files { dir, downloading: HashMap::new() }
    }

    /// Where each model stands on disk now.
    pub fn look(&self) -> Vec<ModelFile> {
        MODELS
            .iter()
            .map(|&model| {
                let entry = entry(model).entry;
                let state = match (store::on_disk(&self.dir, &entry), store::on_disk(&self.dir, &VAD)) {
                    (OnDisk::Ready, OnDisk::Ready) => FileState::Ready,
                    // The model is whole and the gate is not: it is fetched with the model's next download.
                    (OnDisk::Ready, _) => FileState::Partial { done: entry.size, total: entry.size },
                    (OnDisk::Partial(done), _) => FileState::Partial { done, total: entry.size },
                    (OnDisk::Missing, _) => FileState::Missing,
                };
                ModelFile { model, size: entry.size, state }
            })
            .collect()
    }

    pub fn path(&self, model: Model) -> PathBuf {
        store::path(&self.dir, &entry(model).entry)
    }

    /// Silero's file, when it is whole.
    pub fn vad(&self) -> Option<PathBuf> {
        (store::on_disk(&self.dir, &VAD) == OnDisk::Ready).then(|| store::path(&self.dir, &VAD))
    }

    pub fn downloading(&self, model: Model) -> bool {
        self.downloading.contains_key(&model)
    }

    /// Starts `model`'s download, and Silero's before it when it is not whole; `jobs` hears its
    /// progress and its end.
    pub fn download(&mut self, model: Model, jobs: Sender<Job>) {
        if self.downloading(model) {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        self.downloading.insert(model, stop.clone());
        let dir = self.dir.clone();
        let spawned = std::thread::Builder::new().name("dictation-download".into()).spawn(move || {
            let ended = fetch_with_gate(&dir, &entry(model).entry, &stop, &jobs, model);
            let _ = jobs.send(Job::Fetched { model, ended });
        });
        if let Err(err) = spawned {
            self.downloading.remove(&model);
            tracing::warn!("dictation: could not start a download: {err}");
        }
    }

    pub fn cancel(&self, model: Model) {
        if let Some(stop) = self.downloading.get(&model) {
            stop.store(true, Ordering::Relaxed);
        }
    }

    /// A download ended: it is no longer under way.
    pub fn ended(&mut self, model: Model) {
        self.downloading.remove(&model);
    }

    pub fn delete(&self, model: Model) -> Result<(), String> {
        store::delete(&self.dir, &entry(model).entry).map_err(|err| format!("could not delete {}: {err}", model.short()))
    }
}

fn fetch_with_gate(dir: &Path, entry: &Entry, stop: &AtomicBool, jobs: &Sender<Job>, model: Model) -> Result<Fetched, String> {
    let source = Https::default();
    // A gate already on disk is not fetched again (`fetch`).
    if fetch(&VAD, dir, &source, stop, &mut |_| {})? == Fetched::Stopped {
        return Ok(Fetched::Stopped);
    }
    let mut said = Instant::now() - PROGRESS_EVERY;
    fetch(entry, dir, &source, stop, &mut |done| {
        if said.elapsed() >= PROGRESS_EVERY {
            said = Instant::now();
            let _ = jobs.send(Job::Progress { model, done });
        }
    })
}
