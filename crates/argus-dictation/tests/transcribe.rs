//! A short spoken sentence through the real engine: Parakeet's buffered stream behind the Silero
//! gate, fed as the microphone would feed it, finished, and cleaned as a run's text is.
//!
//! It needs the model files, so it is ignored by default. Point `ARGUS_DICTATION_MODELS` at a folder
//! holding `parakeet-unified-en-0.6b-Q8_0.gguf` and `silero-vad-v6.2-F32.gguf` and run it by name:
//!
//!   ARGUS_DICTATION_MODELS=<dir> cargo test -p argus-dictation --test transcribe -- --ignored --nocapture
//!
//! `fixtures/dictation.wav` is Windows' own speech synthesizer saying "Paste this into the focused
//! pane." at 16 kHz mono.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use argus_dictation::engine::load::{Config, Device};
use argus_dictation::engine::{Ask, Engine, Said, Start, Status};
use argus_dictation::models::catalog::{PARAKEET, VAD};
use argus_dictation::text::{clean, Clean};

fn models() -> PathBuf {
    std::env::var_os("ARGUS_DICTATION_MODELS").map(PathBuf::from).expect("ARGUS_DICTATION_MODELS names the folder the model files are in")
}

fn fixture() -> Vec<f32> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dictation.wav");
    let mut reader = hound::WavReader::open(path).expect("the fixture");
    assert_eq!((reader.spec().sample_rate, reader.spec().channels), (16_000, 1));
    reader.samples::<i16>().map(|s| f32::from(s.unwrap()) / 32_768.0).collect()
}

/// What the engine says, until it says what `done` looks for or a minute passes.
fn hear_until(heard: &mpsc::Receiver<Said>, done: impl Fn(&Said) -> bool) -> Vec<Said> {
    let mut said = Vec::new();
    while let Ok(next) = heard.recv_timeout(Duration::from_secs(60)) {
        let last = done(&next);
        said.push(next);
        if last {
            return said;
        }
    }
    panic!("the engine went quiet; it said {said:?}");
}

fn transcribe(device: Device) {
    let dir = models();
    let (tell, heard) = mpsc::channel();
    let engine = Engine::spawn(move |said| {
        let _ = tell.send(said);
    })
    .unwrap();
    let config = Config { model: dir.join(PARAKEET.entry.file), shape: PARAKEET.shape, vad: Some(dir.join(VAD.file)), device };
    engine.ask(Ask::Configure(Some(config)));
    let loading = hear_until(&heard, |said| matches!(said, Said::Status(Status::Ready { .. } | Status::Failed(_))));
    println!("{loading:?}");
    assert!(matches!(loading.last(), Some(Said::Status(Status::Ready { .. }))), "{loading:?}");

    engine.ask(Ask::Start(Start { run: 1, language: None, gate: true }));
    let mut audio = fixture();
    // The hangover's worth of silence after the voice, as a person pauses before letting go.
    audio.extend(std::iter::repeat_n(0.0, 16_000 * 2));
    for chunk in audio.chunks(800) {
        engine.ask(Ask::Audio { run: 1, pcm: chunk.to_vec() });
    }
    engine.ask(Ask::Stop { run: 1 });
    let said = hear_until(&heard, |said| matches!(said, Said::Final { .. } | Said::Failed { .. }));
    assert!(said.iter().any(|said| matches!(said, Said::Started { run: 1 })), "{said:?}");
    assert!(said.iter().any(|said| matches!(said, Said::Text { run: 1, .. })), "the caption moved while it listened: {said:?}");
    let Some(Said::Final { text, .. }) = said.last() else { panic!("{said:?}") };
    println!("heard: {text:?}");
    let cleaned = clean(text, Clean { remove_fillers: true, language: Some("en"), final_text: true }).to_lowercase();
    for word in ["paste", "this", "into", "the", "focused", "pane"] {
        assert!(cleaned.contains(word), "{word:?} is missing from {cleaned:?}");
    }
}

#[test]
#[ignore = "needs the Parakeet and Silero files; run it by name with ARGUS_DICTATION_MODELS"]
fn a_spoken_sentence_comes_back_as_its_words_on_the_cpu() {
    transcribe(Device::Cpu);
}

#[test]
#[ignore = "needs the Parakeet and Silero files; run it by name with ARGUS_DICTATION_MODELS"]
fn a_spoken_sentence_comes_back_as_its_words_on_auto() {
    transcribe(Device::Auto);
}
