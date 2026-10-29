//! The microphone while a run listens: its own format and rate turned into 16 kHz mono chunks, and
//! how loud it is now, for the panel's meter.
//!
//! cpal's stream is not `Send` on every platform, so it is built, played and dropped on a thread of
//! its own, which reports whether it opened before `start` returns.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait as _, StreamTrait as _};
use cpal::{ErrorKind, Sample as _, SampleFormat};

use super::devices::input;
use super::resample::{downmix, Resampler};
use crate::vad::SAMPLE_RATE;

/// How much audio a chunk handed on holds: 50 ms, so the caption keeps up with the voice.
const CHUNK: usize = SAMPLE_RATE as usize / 20;

/// Why the microphone could not be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The system has no input device.
    NoMicrophone,
    /// The system would not let this process use it (a privacy setting).
    Denied,
    Failed(String),
}

/// How loud the microphone was in the last chunk, as the root mean square of its samples.
#[derive(Clone, Default)]
pub struct Level(Arc<AtomicU32>);

impl Level {
    /// A level written into `shared`, which the window reads its meter from.
    pub fn shared(shared: Arc<AtomicU32>) -> Level {
        Level(shared)
    }

    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn set(&self, rms: f32) {
        self.0.store(rms.to_bits(), Ordering::Relaxed);
    }
}

/// The open microphone; dropping it closes it.
pub struct Capture {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    level: Level,
}

impl Capture {
    pub fn level(&self) -> Level {
        self.level.clone()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Opens `microphone` (by id; the system's default when none, or when it is gone) and hands each
/// 16 kHz mono chunk to `deliver`, on the audio thread, until the capture is dropped; `level` says
/// how loud each chunk was.
pub fn start(microphone: Option<&str>, level: Level, deliver: impl FnMut(Vec<f32>) + Send + 'static) -> Result<Capture, Refusal> {
    let microphone = microphone.map(str::to_owned);
    let metered = level.clone();
    let (stop, stopped) = mpsc::channel::<()>();
    let (opened, opening) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("dictation-capture".into())
        .spawn(move || {
            let stream = match open(microphone.as_deref(), deliver, metered) {
                Ok(stream) => stream,
                Err(refusal) => {
                    let _ = opened.send(Err(refusal));
                    return;
                }
            };
            let _ = opened.send(Ok(()));
            // Held until the capture is dropped, which closes the channel.
            let _ = stopped.recv();
            drop(stream);
        })
        .map_err(|err| Refusal::Failed(err.to_string()))?;
    match opening.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(Capture { stop: Some(stop), thread: Some(thread), level }),
        Ok(Err(refusal)) => {
            let _ = thread.join();
            Err(refusal)
        }
        Err(_) => Err(Refusal::Failed("the microphone did not open".into())),
    }
}

fn refusal(err: cpal::Error) -> Refusal {
    match err.kind() {
        ErrorKind::PermissionDenied => Refusal::Denied,
        ErrorKind::DeviceNotAvailable => Refusal::NoMicrophone,
        _ => Refusal::Failed(err.to_string()),
    }
}

/// Turns each callback's samples into 16 kHz mono and hands them on in [`CHUNK`]s.
struct Converter<F> {
    channels: usize,
    resampler: Resampler,
    pending: Vec<f32>,
    deliver: F,
    level: Level,
}

impl<F: FnMut(Vec<f32>)> Converter<F> {
    fn take(&mut self, interleaved: &[f32]) {
        let mono = downmix(interleaved, self.channels);
        self.pending.extend(self.resampler.push(&mono));
        while self.pending.len() >= CHUNK {
            let chunk: Vec<f32> = self.pending.drain(..CHUNK).collect();
            let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            self.level.set(rms);
            (self.deliver)(chunk);
        }
    }
}

fn open(microphone: Option<&str>, deliver: impl FnMut(Vec<f32>) + Send + 'static, level: Level) -> Result<cpal::Stream, Refusal> {
    let device = input(microphone).ok_or(Refusal::NoMicrophone)?;
    let supported = device.default_input_config().map_err(refusal)?;
    let format = supported.sample_format();
    let config = supported.config();
    let mut converter = Converter { channels: usize::from(config.channels).max(1), resampler: Resampler::new(config.sample_rate, SAMPLE_RATE), pending: Vec::new(), deliver, level };
    let mut floats = Vec::new();
    macro_rules! as_floats {
        ($data:expr, $($format:ident => $ty:ty),*) => {
            match format {
                $(SampleFormat::$format => $data.as_slice::<$ty>().map(|samples| {
                    floats.clear();
                    floats.extend(samples.iter().map(|s| s.to_sample::<f32>()));
                }),)*
                _ => None,
            }
        };
    }
    if !matches!(format, SampleFormat::I8 | SampleFormat::I16 | SampleFormat::I32 | SampleFormat::U8 | SampleFormat::U16 | SampleFormat::U32 | SampleFormat::F32 | SampleFormat::F64) {
        return Err(Refusal::Failed(format!("the microphone's {format:?} samples are not read")));
    }
    let stream = device
        .build_input_stream_raw(
            config,
            format,
            move |data, _| {
                if as_floats!(data, I8 => i8, I16 => i16, I32 => i32, U8 => u8, U16 => u16, U32 => u32, F32 => f32, F64 => f64).is_some() {
                    converter.take(&floats);
                }
            },
            |err| tracing::warn!("the microphone stopped: {err}"),
            Some(Duration::from_secs(5)),
        )
        .map_err(refusal)?;
    stream.play().map_err(refusal)?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_microphone_at_its_mix_rate_is_handed_on_in_fifty_millisecond_chunks_at_sixteen_kilohertz() {
        let chunks = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = chunks.clone();
        let level = Level::default();
        let mut converter = Converter { channels: 2, resampler: Resampler::new(48_000, SAMPLE_RATE), pending: Vec::new(), deliver: move |chunk: Vec<f32>| seen.borrow_mut().push(chunk), level: level.clone() };
        // A second of stereo at 48 kHz, in callbacks of 10 ms.
        for _ in 0..100 {
            converter.take(&[0.5; 960]);
        }
        let chunks = chunks.borrow();
        assert!(chunks.iter().all(|chunk| chunk.len() == CHUNK));
        assert_eq!(chunks.len(), 19, "a second is twenty chunks, short of the filter's few samples");
        assert!((level.get() - 0.5).abs() < 1e-3, "the level is the last chunk's: {}", level.get());
    }
}
