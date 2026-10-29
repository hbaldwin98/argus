//! A row of thin level bars: how loud a microphone is now, the dictation panel's meter (the Argus
//! Dictation design's `.meter`), or a still ramp while nothing is heard yet, or flat once listening
//! is over.

use gpui_kit::component::h_flex;
use gpui_kit::{div, prelude::*, Hsla, Role, SharedString, TestSupportExt as _};

use crate::theme::scale::rem;
use crate::theme::Tokens;

/// What the bars show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bars {
    /// The level now, the root mean square of the last chunk, and the time in seconds, which keeps
    /// the bars moving while a voice holds one level.
    Live { level: f32, at: f32 },
    /// A still ramp: listening, but the model is not up yet.
    Waiting,
    /// Every bar at its floor.
    Flat,
}

/// How far a speaking voice's level lifts the bars: an RMS of about 0.12 fills them, and the square
/// root gives a quiet voice some height.
const GAIN: f32 = 8.0;

/// Each bar's height, from `floor` to `top`: a hump across the row (the design's sine), scaled by
/// the level and stirred by the time.
pub fn heights(bars: Bars, count: usize, floor: f32, top: f32) -> Vec<f32> {
    (0..count)
        .map(|at| match bars {
            Bars::Flat => floor,
            Bars::Waiting => floor + (at % 4) as f32 * 2.0,
            Bars::Live { level, at: time } => {
                let hump = ((at + 1) as f32 / (count + 1) as f32 * std::f32::consts::PI).sin();
                let stir = 0.6 + 0.4 * (time * 7.0 + at as f32 * 1.7).sin().abs();
                let lift = (level.max(0.0) * GAIN).sqrt().min(1.0);
                floor + (top - floor) * lift * hump * stir
            }
        })
        .collect()
}

/// The bars, in `colour`, centred on the row's middle.
pub fn bars(id: &'static str, bars: Bars, colour: Hsla, tokens: &Tokens) -> impl IntoElement {
    let label = match bars {
        Bars::Live { .. } => "Microphone level",
        Bars::Waiting => "Waiting for the model",
        Bars::Flat => "Not listening",
    };
    h_flex()
        .id(id)
        .role(Role::Meter)
        .test_support()
        .aria_label(SharedString::from(label))
        .flex_none()
        .h(rem(tokens.dictate_bar_h))
        .items_center()
        .gap(rem(tokens.sp_2))
        .children(
            heights(bars, tokens.dictate_bars, tokens.dictate_bar_floor, tokens.dictate_bar_h)
                .into_iter()
                .map(|height| div().w(rem(tokens.dictate_bar_w)).h(rem(height)).rounded(rem(tokens.dictate_bar_w / 2.0)).bg(colour)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_and_a_flat_meter_sit_at_the_floor_and_a_voice_lifts_the_middle_most() {
        assert!(heights(Bars::Flat, 14, 3.0, 14.0).iter().all(|h| *h == 3.0));
        assert!(heights(Bars::Live { level: 0.0, at: 1.0 }, 14, 3.0, 14.0).iter().all(|h| *h == 3.0));
        let loud = heights(Bars::Live { level: 0.5, at: 0.0 }, 14, 3.0, 14.0);
        assert!(loud.iter().all(|h| (3.0..=14.0).contains(h)), "{loud:?}");
        let hump = |at: usize| ((at + 1) as f32 / 15.0 * std::f32::consts::PI).sin();
        assert!(hump(6) > hump(0) && hump(6) > hump(13), "the middle is the tallest place");
    }

    #[test]
    fn waiting_is_a_still_ramp() {
        let ramp = heights(Bars::Waiting, 8, 3.0, 14.0);
        assert_eq!(ramp, vec![3.0, 5.0, 7.0, 9.0, 3.0, 5.0, 7.0, 9.0]);
    }
}
