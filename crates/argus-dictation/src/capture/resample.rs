//! How a microphone's own rate becomes the 16 kHz the models hear: a windowed-sinc interpolator
//! that low-passes as it goes, so a 48 kHz mix does not fold its highs into speech.
//!
//! It runs on the audio callback's chunks one after another and keeps the few samples either side
//! of the next output, so chunking never changes what comes out.

use std::f64::consts::PI;

/// Taps either side of an output sample.
const HALF: usize = 16;
/// How far under the output's Nyquist the filter starts to fall.
const ROLL_OFF: f64 = 0.9;

pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// The cutoff, in cycles per input sample.
    cutoff: f64,
    /// The input not yet past, from the absolute index `start`.
    held: Vec<f32>,
    start: u64,
    /// Where the next output sample falls, in absolute input samples.
    next: f64,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        let step = f64::from(from) / f64::from(to);
        Self { step, cutoff: 0.5 * ROLL_OFF * step.recip().min(1.0), held: Vec::new(), start: 0, next: 0.0 }
    }

    /// Whether it changes the rate at all: one that does not passes its input through.
    pub fn passes_through(&self) -> bool {
        self.step == 1.0
    }

    /// The output the input so far makes, past what was made already.
    pub fn push(&mut self, input: &[f32]) -> Vec<f32> {
        if self.passes_through() {
            return input.to_vec();
        }
        self.held.extend_from_slice(input);
        let end = self.start + self.held.len() as u64;
        let mut out = Vec::with_capacity((input.len() as f64 / self.step) as usize + 1);
        loop {
            let centre = self.next.floor() as i64;
            if centre + HALF as i64 >= end as i64 {
                break;
            }
            out.push(self.sample_at(centre));
            self.next += self.step;
        }
        let keep_from = (self.next.floor() as i64 - HALF as i64).max(self.start as i64) as u64;
        self.held.drain(..(keep_from - self.start) as usize);
        self.start = keep_from;
        out
    }

    /// The filtered value at `self.next`, from the taps around `centre`; before the first input the
    /// signal is silence. The taps are normalised, so a constant comes through as itself.
    fn sample_at(&self, centre: i64) -> f32 {
        let (mut sum, mut weight) = (0.0f64, 0.0f64);
        for at in (centre - HALF as i64 + 1)..=(centre + HALF as i64) {
            let distance = self.next - at as f64;
            let tap = taper(distance, self.cutoff);
            weight += tap;
            if at >= self.start as i64 {
                sum += f64::from(self.held[(at - self.start as i64) as usize]) * tap;
            }
        }
        if weight == 0.0 { 0.0 } else { (sum / weight) as f32 }
    }
}

/// The low-pass's tap `distance` input samples from the output: a sinc at `cutoff` under a Hann
/// window `HALF` wide.
fn taper(distance: f64, cutoff: f64) -> f64 {
    if distance.abs() >= HALF as f64 {
        return 0.0;
    }
    let x = 2.0 * cutoff * distance;
    let sinc = if x == 0.0 { 1.0 } else { (PI * x).sin() / (PI * x) };
    let window = 0.5 * (1.0 + (PI * distance / HALF as f64).cos());
    2.0 * cutoff * sinc * window
}

/// One frame of `channels` interleaved samples as their mean.
pub fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f64, rate: u32, seconds: f64) -> Vec<f32> {
        let n = (f64::from(rate) * seconds) as usize;
        (0..n).map(|i| (2.0 * PI * hz * i as f64 / f64::from(rate)).sin() as f32).collect()
    }

    /// The loudest sample past the filter's start, where the signal is whole.
    fn peak(samples: &[f32]) -> f32 {
        samples[samples.len() / 4..].iter().fold(0.0f32, |most, s| most.max(s.abs()))
    }

    #[test]
    fn sixteen_kilohertz_passes_through_untouched() {
        let mut same = Resampler::new(16_000, 16_000);
        let input = tone(440.0, 16_000, 0.1);
        assert_eq!(same.push(&input), input);
    }

    #[test]
    fn a_second_of_a_mix_rate_becomes_a_second_at_sixteen_kilohertz() {
        for from in [48_000, 44_100, 32_000, 22_050] {
            let mut resampler = Resampler::new(from, 16_000);
            let out = resampler.push(&vec![0.0; from as usize]);
            // Short only by the taps the last output still waits for.
            assert!((15_980..=16_000).contains(&out.len()), "{from}: {}", out.len());
        }
    }

    #[test]
    fn a_constant_comes_through_as_itself() {
        let mut resampler = Resampler::new(48_000, 16_000);
        let out = resampler.push(&vec![0.5; 4800]);
        for sample in &out[HALF..] {
            assert!((sample - 0.5).abs() < 1e-5, "{sample}");
        }
    }

    #[test]
    fn speech_band_tones_keep_their_strength_and_those_past_eight_kilohertz_are_cut() {
        let mut low = Resampler::new(48_000, 16_000);
        let kept = peak(&low.push(&tone(1_000.0, 48_000, 0.5)));
        assert!((0.95..=1.05).contains(&kept), "a 1 kHz tone comes out at {kept}");
        let mut high = Resampler::new(48_000, 16_000);
        let folded = peak(&high.push(&tone(12_000.0, 48_000, 0.5)));
        assert!(folded < 0.05, "a 12 kHz tone would fold to 4 kHz; it comes out at {folded}");
    }

    #[test]
    fn the_output_does_not_depend_on_how_the_input_was_chunked() {
        let input = tone(700.0, 44_100, 0.3);
        let whole = Resampler::new(44_100, 16_000).push(&input);
        let mut chunked = Resampler::new(44_100, 16_000);
        let pieces: Vec<f32> = input.chunks(441).flat_map(|chunk| chunked.push(chunk)).collect();
        assert_eq!(pieces.len(), whole.len());
        for (a, b) in pieces.iter().zip(&whole) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn a_stereo_frame_is_the_mean_of_its_channels() {
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2), vec![0.5, 0.5, 0.0]);
        assert_eq!(downmix(&[0.25, 0.75], 1), vec![0.25, 0.75]);
    }
}
