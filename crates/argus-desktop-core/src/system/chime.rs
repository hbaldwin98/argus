//! The sound an agent coming to need you makes, as numbers: two soft sine notes, rising, short
//! enough to hear without being asked to stop.
//!
//! Synthesized rather than a file, so there is no asset to ship, and kept as a pure function so
//! every window plays the same sound whatever it plays it through: the page's Web Audio graph
//! (`page/src/system/chime.ts`) and a native window's output are the same envelope. Playing it is
//! the window's, since the audio device is a platform's.

use std::f32::consts::TAU;

/// The notes: when each starts, in seconds, and its pitch.
const NOTES: [(f32, f32); 2] = [(0.0, 660.0), (0.12, 880.0)];

/// The gain a note rises to.
const PEAK: f32 = 0.18;
/// The gain it falls to, which an exponential fall cannot reach zero for.
const FLOOR: f32 = 0.001;
/// A quick rise and a long fall: a click at either end otherwise.
const RISE: f32 = 0.01;
const FALL_BY: f32 = 0.35;
/// When a note is stopped.
const STOP: f32 = 0.4;

/// The gain `t` seconds after a note began: a linear rise to the peak, an exponential fall to the
/// floor, then the floor until the note is stopped, and nothing after.
pub fn gain(t: f32) -> f32 {
    if t < 0.0 || t >= STOP {
        0.0
    } else if t < RISE {
        PEAK * t / RISE
    } else if t < FALL_BY {
        PEAK * (FLOOR / PEAK).powf((t - RISE) / (FALL_BY - RISE))
    } else {
        FLOOR
    }
}

/// How long the whole chime lasts, in seconds.
pub fn length() -> f32 {
    NOTES.iter().map(|(at, _)| at + STOP).fold(0.0, f32::max)
}

/// The chime as mono samples at `rate` Hz, each in `-1.0..=1.0`.
pub fn samples(rate: u32) -> Vec<f32> {
    let count = (length() * rate as f32).ceil() as usize;
    (0..count)
        .map(|i| {
            let now = i as f32 / rate as f32;
            NOTES.iter().map(|&(at, pitch)| gain(now - at) * (TAU * pitch * (now - at).max(0.0)).sin()).sum()
        })
        .collect()
}

/// The chime as a 16-bit mono WAV file, which every platform's simplest player takes whole.
pub fn wav(rate: u32) -> Vec<u8> {
    let pcm: Vec<i16> = samples(rate).into_iter().map(|s| (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16).collect();
    let bytes = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    // PCM, one channel, then the byte rate, the block size and the bits to a sample.
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for sample in pcm {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_rises_in_ten_milliseconds_and_falls_to_a_thousandth_by_0_35_seconds() {
        assert_eq!(gain(0.0), 0.0);
        assert!((gain(0.005) - 0.09).abs() < 1e-6, "halfway up the rise");
        assert!((gain(0.01) - 0.18).abs() < 1e-6, "the peak");
        assert!((gain(0.35) - 0.001).abs() < 1e-6, "the floor");
        assert!(gain(0.1) < gain(0.05) && gain(0.05) < gain(0.01), "falling the whole way");
    }

    #[test]
    fn the_fall_is_exponential_so_equal_steps_lose_an_equal_share() {
        let ratio = |a: f32, b: f32| gain(b) / gain(a);
        assert!((ratio(0.05, 0.10) - ratio(0.10, 0.15)).abs() < 1e-4);
    }

    #[test]
    fn a_note_holds_the_floor_until_it_is_stopped_at_0_4_seconds() {
        assert_eq!(gain(0.38), 0.001);
        assert_eq!(gain(0.4), 0.0);
        assert_eq!(gain(-0.01), 0.0, "nothing before it starts");
    }

    #[test]
    fn the_chime_is_two_notes_the_second_starting_0_12_seconds_later() {
        assert!((length() - 0.52).abs() < 1e-6);
        let rate = 48_000;
        let all = samples(rate);
        assert_eq!(all.len(), (0.52 * rate as f32).ceil() as usize);
        // The first note alone sounds before the second begins: 660 Hz is a sine from zero.
        let at = |seconds: f32| all[(seconds * rate as f32) as usize];
        let quarter = 1.0 / (4.0 * 660.0);
        assert!((at(quarter) - gain(quarter)).abs() < 1e-3, "{} vs {}", at(quarter), gain(quarter));
        // After the first stops at 0.4 only the second is left, and it is still sounding.
        assert!(all[(0.45 * rate as f32) as usize..].iter().any(|s| s.abs() > 1e-4));
        // The tail is the floor, not a click.
        assert!(all.last().unwrap().abs() <= 0.002);
    }

    #[test]
    fn the_second_note_is_pitched_880_hertz() {
        let rate = 48_000;
        let all = samples(rate);
        // Past the first note's end only the 880 Hz note sounds; count rising zero crossings.
        let from = (0.41 * rate as f32) as usize;
        let to = (0.50 * rate as f32) as usize;
        let rising = all[from..to].windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count() as f32;
        let hertz = rising / ((to - from) as f32 / rate as f32);
        assert!((hertz - 880.0).abs() < 25.0, "{hertz}");
    }

    #[test]
    fn the_wav_is_a_sixteen_bit_mono_file_that_says_how_long_it_is() {
        let file = wav(8_000);
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(&file[8..16], b"WAVEfmt ");
        let samples = samples(8_000).len();
        assert_eq!(file.len(), 44 + samples * 2);
        assert_eq!(u32::from_le_bytes(file[4..8].try_into().unwrap()) as usize, file.len() - 8);
        assert_eq!(u32::from_le_bytes(file[40..44].try_into().unwrap()) as usize, samples * 2);
        assert_eq!(u32::from_le_bytes(file[24..28].try_into().unwrap()), 8_000, "the rate");
    }
}
