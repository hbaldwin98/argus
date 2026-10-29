//! One run through the loaded model: the microphone's chunks past the Silero gate into a stream, the
//! text so far as it changes, and the final transcript when it stops.
//!
//! Handy's rules (`managers/transcription/live.rs`, by way of learn-harness's worker): the final text
//! is the stream's whole transcript, a feed that fails is logged and never ends the run, and a stream
//! that finishes with nothing, or fails, gives way to one batch pass over what it was fed.

use std::sync::mpsc::Receiver;

use transcribe_cpp::{RunOptions, StreamState, VadSession};

use super::load::{stream_options, Loaded};
use super::{Ask, Said, Start};
use crate::vad::{SmoothedVad, SAMPLE_RATE, SILERO_THRESHOLD};

/// Silero over the chunks as they come, the smoother deciding which frames go on.
struct Gate {
    smoother: SmoothedVad,
    frame: usize,
    /// Samples Silero has not scored a whole frame of yet.
    pending: Vec<f32>,
}

impl Gate {
    fn pass(&mut self, vad: &mut VadSession, pcm: &[f32]) -> Vec<f32> {
        let scored = match vad.stream_feed(pcm) {
            Ok(scored) => scored,
            // A gate that cannot score lets the voice through rather than lose it.
            Err(err) => {
                tracing::warn!("dictation: the voice gate failed ({err}); passing the audio on");
                return pcm.to_vec();
            }
        };
        self.pending.extend_from_slice(pcm);
        let frames = scored.probs.len().min(self.pending.len() / self.frame);
        let mut out = Vec::new();
        for (at, prob) in scored.probs.iter().take(frames).enumerate() {
            let frame = &self.pending[at * self.frame..(at + 1) * self.frame];
            for piece in self.smoother.push(frame, *prob >= SILERO_THRESHOLD) {
                out.extend(piece);
            }
        }
        self.pending.drain(..frames * self.frame);
        out
    }
}

/// Serves `start` until it is stopped or cancelled. A `Configure` or another `Start` that arrives
/// first ends it, unsaid, and is handed back to be dealt with next.
pub(crate) fn serve(loaded: &mut Loaded, start: &Start, asks: &Receiver<Ask>, say: &dyn Fn(Said)) -> Option<Ask> {
    let run = start.run;
    let options = RunOptions { language: start.language.clone(), ..RunOptions::default() };
    let Loaded { session, vad, frame_samples, config, .. } = loaded;
    let mut gate = match (start.gate, vad.as_mut()) {
        (true, Some(vad)) => {
            vad.stream_reset();
            Some(Gate { smoother: SmoothedVad::new(*frame_samples), frame: *frame_samples, pending: Vec::new() })
        }
        _ => None,
    };
    let mut stream = match session.stream(&options, &stream_options(config.shape)) {
        Ok(stream) => stream,
        Err(err) => {
            say(Said::Failed { run, reason: err.to_string() });
            return None;
        }
    };
    say(Said::Started { run });
    let mut recorded: Vec<f32> = Vec::new();
    let (mut committed, mut tentative) = (String::new(), String::new());
    loop {
        let Ok(ask) = asks.recv() else { return None };
        match ask {
            Ask::Audio { run: of, pcm } if of == run => {
                let fed = match (&mut gate, vad.as_mut()) {
                    (Some(gate), Some(vad)) => gate.pass(vad, &pcm),
                    _ => pcm,
                };
                if fed.is_empty() {
                    continue;
                }
                recorded.extend_from_slice(&fed);
                if stream.state() != StreamState::Active {
                    continue;
                }
                if let Err(err) = stream.feed(&fed) {
                    tracing::warn!("dictation: a feed failed ({err}); the run goes on");
                    continue;
                }
                let text = stream.text();
                if text.committed != committed || text.tentative != tentative {
                    committed.clone_from(&text.committed);
                    tentative.clone_from(&text.tentative);
                    say(Said::Text { run, committed: text.committed, tentative: text.tentative });
                }
            }
            Ask::Stop { run: of } if of == run => break,
            Ask::Cancel { run: of } if of == run => return None,
            Ask::Audio { .. } | Ask::Stop { .. } | Ask::Cancel { .. } => {}
            interrupting @ (Ask::Configure(_) | Ask::Start(_)) => return Some(interrupting),
        }
    }
    let (mut text, mut language) = (String::new(), None);
    match stream.finalize() {
        Ok(_) => {
            text = stream.text().full;
            if start.language.is_none() {
                language = stream.snapshot().language;
            }
        }
        Err(err) => tracing::warn!("dictation: finishing the stream failed ({err})"),
    }
    drop(stream);
    if text.trim().is_empty() && recorded.len() > SAMPLE_RATE as usize / 4 {
        tracing::info!("dictation: the stream heard nothing; transcribing the run whole");
        match session.run(&recorded, &options) {
            Ok(whole) => {
                text = whole.text;
                language = language.or(whole.language);
            }
            Err(err) => tracing::warn!("dictation: the batch pass failed ({err})"),
        }
    }
    say(Said::Final { run, text: text.trim().to_owned(), language: language.filter(|language| !language.is_empty()) });
    None
}
