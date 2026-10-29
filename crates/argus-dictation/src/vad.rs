//! Which of the microphone's frames the model hears: Silero's per-frame speech probability, smoothed
//! so a word's first syllable and its last are not cut off.
//!
//! Handy's `SmoothedVad` (cjpais/Handy, `audio_toolkit/vad/smoothed.rs`, by way of learn-harness's
//! `electron/dictation-worker.mjs`): speech must hold for the onset before the gate opens, and
//! opening lets through the pre-roll kept from just before it; once speech stops, the hangover keeps
//! frames flowing so a streaming model can still commit its last words. Its constants are Handy's
//! (`audio_toolkit/vad/mod.rs`, `managers/audio.rs`).
//!
//! Ported from Handy, Copyright (c) 2025 CJ Pais, MIT License
//! (<https://github.com/cjpais/Handy/blob/main/LICENSE>).

use std::collections::VecDeque;

/// What every model here hears: 16 kHz mono.
pub const SAMPLE_RATE: u32 = 16_000;
/// A frame at or over this probability is speech.
pub const SILERO_THRESHOLD: f32 = 0.3;
pub const PREFILL_MS: u32 = 450;
pub const ONSET_MS: u32 = 60;
pub const STREAMING_HANGOVER_MS: u32 = 1650;

/// Handy's `frames_for_duration_ms`: how many frames of `frame_samples` cover `ms`, rounded up.
pub fn frames_for(ms: u32, frame_samples: usize) -> usize {
    let samples = u64::from(ms) * u64::from(SAMPLE_RATE);
    samples.div_ceil(frame_samples as u64 * 1000) as usize
}

struct Frame {
    samples: Vec<f32>,
    emitted: bool,
}

pub struct SmoothedVad {
    prefill: usize,
    onset: usize,
    hangover: usize,
    buffer: VecDeque<Frame>,
    hangover_left: usize,
    onset_seen: usize,
    in_speech: bool,
}

impl SmoothedVad {
    pub fn new(frame_samples: usize) -> Self {
        Self::with(frames_for(PREFILL_MS, frame_samples), frames_for(ONSET_MS, frame_samples), frames_for(STREAMING_HANGOVER_MS, frame_samples))
    }

    /// The smoother with its spans in frames.
    pub fn with(prefill: usize, onset: usize, hangover: usize) -> Self {
        Self { prefill, onset, hangover, buffer: VecDeque::new(), hangover_left: 0, onset_seen: 0, in_speech: false }
    }

    /// One frame in, and whether Silero heard a voice in it; the frames to feed the model, oldest
    /// first, which may be none.
    pub fn push(&mut self, samples: &[f32], voice: bool) -> Vec<Vec<f32>> {
        self.buffer.push_back(Frame { samples: samples.to_vec(), emitted: false });
        while self.buffer.len() > self.prefill + 1 {
            self.buffer.pop_front();
        }
        if !self.in_speech && voice {
            self.onset_seen += 1;
            if self.onset_seen < self.onset {
                return Vec::new();
            }
            self.in_speech = true;
            self.hangover_left = self.hangover;
            self.onset_seen = 0;
            return self.buffer.iter_mut().filter(|frame| !frame.emitted).map(|frame| {
                frame.emitted = true;
                frame.samples.clone()
            }).collect();
        }
        if self.in_speech && voice {
            self.hangover_left = self.hangover;
            return self.emit_last();
        }
        if self.in_speech {
            if self.hangover_left > 0 {
                self.hangover_left -= 1;
                return self.emit_last();
            }
            self.in_speech = false;
            return Vec::new();
        }
        self.onset_seen = 0;
        Vec::new()
    }

    fn emit_last(&mut self) -> Vec<Vec<f32>> {
        match self.buffer.back_mut() {
            Some(last) => {
                last.emitted = true;
                vec![last.samples.clone()]
            }
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame whose every sample is `n`, so a test can say which frames came out.
    fn frame(n: usize) -> Vec<f32> {
        vec![n as f32; 4]
    }

    fn names(out: &[Vec<f32>]) -> Vec<usize> {
        out.iter().map(|frame| frame[0] as usize).collect()
    }

    #[test]
    fn handys_spans_are_whole_silero_frames_rounded_up() {
        // Silero scores 512-sample frames: 32 ms at 16 kHz.
        assert_eq!(frames_for(PREFILL_MS, 512), 15, "450 ms is 14.06 frames");
        assert_eq!(frames_for(ONSET_MS, 512), 2, "60 ms is 1.9 frames");
        assert_eq!(frames_for(STREAMING_HANGOVER_MS, 512), 52, "1650 ms is 51.6 frames");
    }

    #[test]
    fn a_voice_shorter_than_the_onset_opens_nothing() {
        let mut vad = SmoothedVad::with(3, 2, 1);
        assert!(vad.push(&frame(0), true).is_empty(), "one frame of voice is under the onset");
        assert!(vad.push(&frame(1), false).is_empty(), "and silence starts the count again");
        assert!(vad.push(&frame(2), true).is_empty());
        assert!(vad.push(&frame(3), false).is_empty());
    }

    #[test]
    fn opening_lets_through_the_pre_roll_kept_from_before_it() {
        let mut vad = SmoothedVad::with(3, 2, 1);
        for n in 0..5 {
            assert!(vad.push(&frame(n), false).is_empty(), "silence is held, not fed");
        }
        assert!(vad.push(&frame(5), true).is_empty());
        // The second frame of voice opens the gate: the three frames of pre-roll and it, oldest first.
        assert_eq!(names(&vad.push(&frame(6), true)), [3, 4, 5, 6]);
        assert_eq!(names(&vad.push(&frame(7), true)), [7], "then each frame of voice as it comes");
    }

    #[test]
    fn the_hangover_keeps_frames_flowing_after_the_voice_stops_then_closes() {
        let mut vad = SmoothedVad::with(0, 1, 2);
        assert_eq!(names(&vad.push(&frame(0), true)), [0]);
        assert_eq!(names(&vad.push(&frame(1), false)), [1], "the hangover's first frame");
        assert_eq!(names(&vad.push(&frame(2), false)), [2], "and its second");
        assert!(vad.push(&frame(3), false).is_empty(), "past it, closed");
        assert!(vad.push(&frame(4), false).is_empty());
    }

    #[test]
    fn a_voice_inside_the_hangover_starts_it_again() {
        let mut vad = SmoothedVad::with(0, 1, 2);
        vad.push(&frame(0), true);
        vad.push(&frame(1), false);
        assert_eq!(names(&vad.push(&frame(2), true)), [2]);
        assert_eq!(names(&vad.push(&frame(3), false)), [3]);
        assert_eq!(names(&vad.push(&frame(4), false)), [4], "two frames again from the last voice");
        assert!(vad.push(&frame(5), false).is_empty());
    }

    #[test]
    fn a_reopened_gate_never_feeds_a_frame_twice() {
        let mut vad = SmoothedVad::with(3, 1, 0);
        assert_eq!(names(&vad.push(&frame(0), true)), [0]);
        assert!(vad.push(&frame(1), false).is_empty(), "no hangover: closed at once");
        // The pre-roll still holds frame 0, which was fed already.
        assert_eq!(names(&vad.push(&frame(2), true)), [1, 2]);
    }
}
