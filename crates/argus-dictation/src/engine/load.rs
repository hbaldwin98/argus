//! A model made ready to stream: loaded on the device asked for, Silero beside it on the CPU, and
//! warmed with a second of near-silence so the first words spoken do not pay for the backend's
//! first compute (a cold first feed took 2 to 3.7 s on learn-harness's owner's machine).
//!
//! As Handy's engine supervisor does, a load that fails on Auto marks the GPU unusable for the rest of
//! the process and loads on the CPU; one asked of the GPU outright never falls back, since a silent CPU
//! run is not what was chosen.

use std::path::PathBuf;

use transcribe_cpp::{
    Backend, DeviceType, Model, ModelOptions, ParakeetBufferedStreamOptions, ParakeetStreamOptions, RunOptions, Session,
    StreamExtension, StreamOptions, VadSession,
};

use crate::models::catalog::Shape;
use crate::vad::SAMPLE_RATE;

/// Where the model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Device {
    /// The GPU when there is one, else the CPU.
    #[default]
    Auto,
    Gpu,
    Cpu,
}

/// What to load: the model file and how it streams, Silero's file when the gate is wanted, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub model: PathBuf,
    pub shape: Shape,
    pub vad: Option<PathBuf>,
    pub device: Device,
}

pub(crate) struct Loaded {
    pub config: Config,
    pub session: Session,
    pub vad: Option<VadSession>,
    /// Silero's frame, in samples (512: 32 ms).
    pub frame_samples: usize,
    pub backend: String,
    pub device: String,
    // The sessions hold their models too; these are kept so dropping `Loaded` is the unload.
    _model: Model,
    _vad_model: Option<Model>,
}

pub(crate) const NO_GPU: &str = "No GPU found — choose Auto or CPU in Settings";

/// The stream shape as transcribe.cpp's stream-slot extension.
pub(crate) fn stream_options(shape: Shape) -> StreamOptions {
    let family = match shape {
        Shape::Buffered { left_ms, chunk_ms, right_ms } => {
            StreamExtension::ParakeetBuffered(ParakeetBufferedStreamOptions { left_ms: Some(left_ms), chunk_ms: Some(chunk_ms), right_ms: Some(right_ms) })
        }
        Shape::CacheAware { att_context_right } => StreamExtension::ParakeetStream(ParakeetStreamOptions { att_context_right: Some(att_context_right) }),
    };
    StreamOptions { family: Some(family), ..StreamOptions::default() }
}

fn options(device: Device) -> Result<ModelOptions, String> {
    match device {
        Device::Auto => Ok(ModelOptions::default()),
        Device::Cpu => Ok(ModelOptions { backend: Backend::Cpu, ..ModelOptions::default() }),
        Device::Gpu => {
            let all = transcribe_cpp::devices();
            let gpu = all.iter().find(|d| d.device_type == DeviceType::Gpu).or_else(|| all.iter().find(|d| d.device_type == DeviceType::Igpu));
            match gpu {
                Some(gpu) => Ok(ModelOptions { device: Some(gpu.clone()), ..ModelOptions::default() }),
                None => Err(NO_GPU.into()),
            }
        }
    }
}

/// Loads `config`, on the CPU when an earlier load on Auto lost the GPU; `gpu_lost` is set when this
/// one does.
pub(crate) fn load(config: &Config, gpu_lost: &mut bool) -> Result<Loaded, String> {
    let device = if config.device == Device::Auto && *gpu_lost { Device::Cpu } else { config.device };
    let model = match Model::load_with(&config.model, &options(device)?) {
        Ok(model) => model,
        Err(err) if device == Device::Auto => {
            tracing::warn!("dictation: loading on Auto failed ({err}); the CPU from now on");
            *gpu_lost = true;
            Model::load_with(&config.model, &options(Device::Cpu)?).map_err(|err| err.to_string())?
        }
        Err(err) => return Err(err.to_string()),
    };
    let session = model.session().map_err(|err| err.to_string())?;
    let (vad_model, vad, frame_samples) = match &config.vad {
        Some(path) => {
            let vad_model = Model::load_with(path, &ModelOptions { backend: Backend::Cpu, ..ModelOptions::default() }).map_err(|err| err.to_string())?;
            let frame = vad_model.vad_info().map_err(|err| err.to_string())?.frame_samples.max(1) as usize;
            let session = vad_model.vad_session().map_err(|err| err.to_string())?;
            (Some(vad_model), Some(session), frame)
        }
        None => (None, None, 512),
    };
    let named = model.device().ok();
    let device = named.map(|d| if d.description.trim().is_empty() { d.name } else { d.description.trim().to_owned() }).unwrap_or_default();
    let mut loaded = Loaded {
        config: config.clone(),
        backend: model.backend(),
        device,
        session,
        vad,
        frame_samples,
        _model: model,
        _vad_model: vad_model,
    };
    warm(&mut loaded);
    Ok(loaded)
}

/// A second of near-silence through a real stream; a warm-up that fails costs only its time.
fn warm(loaded: &mut Loaded) {
    let shape = loaded.config.shape;
    let Ok(mut stream) = loaded.session.stream(&RunOptions::default(), &stream_options(shape)) else { return };
    let _ = stream.feed(&vec![1e-4; SAMPLE_RATE as usize]);
    let _ = stream.finalize();
}
