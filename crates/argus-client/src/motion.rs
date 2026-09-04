//! Time as the renderer reads it: how far along an animation is, and what
//! colour that puts on the screen.
//!
//! Nothing here holds state or a clock of its own. A frame is drawn for an
//! `Instant`, every animated thing is a start plus a duration, and this is
//! the arithmetic between them. That keeps the whole visual layer a pure
//! function of the model and the time, which is what lets the render tests
//! pin an animation mid-flight by naming the instant rather than sleeping.
//!
//! The client does not run a frame clock of its own (see [`crate::redraw`]);
//! it wakes on deadlines. So an animation has to be able to say when it next
//! needs a frame, and a repeating one — a spinner — has to say that forever
//! while it spins. [`Animation::deadline`] and [`spinner_deadline`] are those
//! answers, and the event loop takes the earliest of them.

use std::time::{Duration, Instant};

use ratatui::style::Color;

/// A one-shot animation: when it began and how long it runs.
///
/// Held by value rather than as a deadline alone because the curve needs
/// the whole span, not just the end of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Animation {
    start: Instant,
    duration: Duration,
}

impl Animation {
    pub fn starting(now: Instant, duration: Duration) -> Self {
        Animation {
            start: now,
            duration,
        }
    }

    /// How far along, `0.0` at the start and `1.0` at the end. `None` once
    /// it has finished, so a caller drops it rather than drawing the last
    /// frame forever.
    pub fn progress(&self, now: Instant) -> Option<f32> {
        if self.duration.is_zero() {
            return None;
        }
        let elapsed = now.checked_duration_since(self.start)?;
        if elapsed >= self.duration {
            return None;
        }
        Some(elapsed.as_secs_f32() / self.duration.as_secs_f32())
    }

    /// When this animation stops needing frames.
    pub fn deadline(&self) -> Instant {
        self.start + self.duration
    }
}

/// Decelerating: fast to begin with, easing into its resting state.
///
/// The one curve the client uses. A fade that leaves at a constant rate
/// reads as mechanical, and the ease is most of why a transition feels
/// like something settling rather than a value being stepped.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let inverted = 1.0 - t;
    1.0 - inverted * inverted * inverted
}

/// `a` mixed toward `b`, `t` of the way.
///
/// Only truecolor mixes. An indexed or named colour has no components to
/// interpolate, so it snaps at the midpoint rather than being approximated
/// — a theme that has degraded to 256 colours should look like itself
/// throughout, not like a third colour nobody chose. Mixing happens in
/// sRGB rather than a perceptual space: the pairs it runs between are
/// neighbouring shades from one Catppuccin flavour, close enough that the
/// banding a perceptual blend would fix is not visible over 900ms.
pub fn blend(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) = (a, b) else {
        return if t < 0.5 { a } else { b };
    };
    let mix = |from: u8, to: u8| -> u8 {
        let from = from as f32;
        (from + (to as f32 - from) * t).round().clamp(0.0, 255.0) as u8
    };
    Color::Rgb(mix(ar, br), mix(ag, bg), mix(ab, bb))
}

/// How long one frame of the spinner holds.
///
/// Slower than the 16ms frame interval by an order of magnitude, and
/// deliberately: the spinner is the one thing that asks for frames when
/// nothing has changed, so its rate is the idle cost of an agent being
/// busy. Twelve frames a second is legible as motion and cheap enough that
/// a screen full of working panes still spends most of its time asleep.
pub const SPINNER_FRAME: Duration = Duration::from_millis(80);

/// The frames themselves. Braille rather than the ASCII `|/-\`, which
/// wobbles: every glyph here has the same weight and the same width, so
/// the dot appears to travel instead of the character appearing to change.
const SPINNER_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

/// The spinner glyph for this instant.
///
/// Phased off a fixed epoch rather than off when each pane started
/// working, so every spinner on screen turns together. Separately-phased
/// spinners read as several unrelated things happening; together they read
/// as one system that is busy.
pub fn spinner(now: Instant, epoch: Instant) -> &'static str {
    let elapsed = now.checked_duration_since(epoch).unwrap_or_default();
    let frame = elapsed.as_millis() / SPINNER_FRAME.as_millis();
    SPINNER_FRAMES[(frame as usize) % SPINNER_FRAMES.len()]
}

/// When the spinner next changes glyph, given the epoch it is phased off.
pub fn spinner_deadline(now: Instant, epoch: Instant) -> Instant {
    let elapsed = now.checked_duration_since(epoch).unwrap_or_default();
    let frame = elapsed.as_millis() / SPINNER_FRAME.as_millis();
    epoch + SPINNER_FRAME * (frame as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epoch() -> Instant {
        Instant::now()
    }

    #[test]
    fn an_animation_runs_from_zero_to_one_and_then_stops() {
        let start = epoch();
        let anim = Animation::starting(start, Duration::from_millis(100));

        assert_eq!(anim.progress(start), Some(0.0));
        assert_eq!(anim.progress(start + Duration::from_millis(50)), Some(0.5));
        assert_eq!(
            anim.progress(start + Duration::from_millis(100)),
            None,
            "an animation that has reached its end is over, not held at 1.0"
        );
        assert_eq!(anim.progress(start + Duration::from_secs(10)), None);
    }

    #[test]
    fn an_animation_asked_about_a_moment_before_it_began_has_not_started() {
        let start = epoch() + Duration::from_secs(1);
        let anim = Animation::starting(start, Duration::from_millis(100));
        assert_eq!(anim.progress(start - Duration::from_millis(1)), None);
    }

    #[test]
    fn the_ease_holds_its_endpoints_and_front_loads_the_middle() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert!(
            ease_out(0.5) > 0.5,
            "an ease-out is past halfway at the halfway point"
        );
        // Out of range rather than panicking: a caller doing its own
        // arithmetic on progress should not be able to crash the renderer.
        assert_eq!(ease_out(-1.0), 0.0);
        assert_eq!(ease_out(2.0), 1.0);
    }

    #[test]
    fn blending_truecolor_walks_between_the_two() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(100, 200, 40);
        assert_eq!(blend(a, b, 0.0), a);
        assert_eq!(blend(a, b, 1.0), b);
        assert_eq!(blend(a, b, 0.5), Color::Rgb(50, 100, 20));
    }

    #[test]
    fn a_colour_with_no_components_snaps_rather_than_being_approximated() {
        let a = Color::Indexed(4);
        let b = Color::Rgb(255, 255, 255);
        assert_eq!(blend(a, b, 0.25), a);
        assert_eq!(blend(a, b, 0.75), b);
    }

    #[test]
    fn the_spinner_advances_a_frame_at_a_time_and_comes_back_round() {
        let start = epoch();
        let at = |ms| spinner(start + Duration::from_millis(ms), start);

        assert_eq!(at(0), SPINNER_FRAMES[0]);
        assert_eq!(at(79), SPINNER_FRAMES[0], "a frame holds for its whole span");
        assert_eq!(at(80), SPINNER_FRAMES[1]);
        assert_eq!(
            at(80 * SPINNER_FRAMES.len() as u64),
            SPINNER_FRAMES[0],
            "the cycle closes"
        );
    }

    #[test]
    fn every_spinner_on_screen_shows_the_same_frame() {
        let start = epoch();
        let now = start + Duration::from_millis(345);
        // Two panes that started working at different times still share the
        // epoch, so they turn together.
        assert_eq!(spinner(now, start), spinner(now, start));
    }

    #[test]
    fn the_spinner_asks_for_a_frame_when_its_glyph_would_change() {
        let start = epoch();
        let deadline = spinner_deadline(start + Duration::from_millis(10), start);

        assert_eq!(deadline, start + SPINNER_FRAME);
        assert_eq!(
            spinner(deadline, start),
            SPINNER_FRAMES[1],
            "the frame the deadline asks for is the one that shows a new glyph"
        );
        assert!(
            spinner_deadline(deadline, start) > deadline,
            "a deadline landing exactly on a boundary asks for the next one, not itself"
        );
    }
}
