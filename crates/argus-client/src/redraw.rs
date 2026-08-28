//! Which of the event loop's wake-ups is worth a frame.

/// Shortest gap between two frames presented for input. A keystroke still
/// presents on the spot, but a stream of them arriving faster than this
/// rides the frame tick: a frame costs several milliseconds, and drawing
/// one per event let a fast paste spend a whole second painting frames
/// that were replaced before anyone saw them.
const INPUT_PRESENT_GAP: std::time::Duration = std::time::Duration::from_millis(8);

/// Decides which of the loop's wake-ups is worth a frame.
///
/// Damage from the daemon is coalesced onto `FRAME_INTERVAL`: an agent
/// painting a spinner would otherwise redraw the whole screen for every
/// chunk it produces. Input is not, up to `INPUT_PRESENT_GAP` — the person
/// who pressed the key is waiting on the echo.
#[derive(Default)]
pub struct RedrawScheduler {
    dirty: bool,
    present: bool,
    last_present: Option<std::time::Instant>,
}

impl RedrawScheduler {
    /// Something changed, but nobody is waiting on it — the next frame.
    pub fn changed(&mut self) {
        self.dirty = true;
    }

    /// Something changed and someone is waiting on it — this frame, unless
    /// one has just gone out, in which case the tick is close enough.
    pub fn input(&mut self, now: std::time::Instant) {
        self.dirty = true;
        if self
            .last_present
            .is_none_or(|last| now.duration_since(last) >= INPUT_PRESENT_GAP)
        {
            self.present = true;
        }
    }

    /// A frame has come due for whatever was already dirty.
    pub fn due(&mut self) {
        self.present = true;
    }

    pub fn pending(&self) -> bool {
        self.dirty
    }

    pub fn take_frame(&mut self, now: std::time::Instant) -> bool {
        if !(self.dirty && self.present) {
            return false;
        }
        self.dirty = false;
        self.present = false;
        self.last_present = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn many_damage_messages_request_one_draw_on_the_next_frame() {
        let mut redraw = RedrawScheduler::default();
        for _ in 0..10_000 {
            redraw.changed();
        }

        let now = std::time::Instant::now();
        assert!(
            !redraw.take_frame(now),
            "damage drew before its frame came due"
        );
        redraw.due();
        assert!(redraw.take_frame(now));
        assert!(!redraw.take_frame(now));
    }
    #[test]
    fn a_keystroke_draws_without_waiting_for_the_next_frame() {
        // Regression: input used to be folded into the same 16ms grid as
        // damage, so every character typed into a prompt or a pane waited
        // out the rest of a frame it had no part in starting.
        let mut redraw = RedrawScheduler::default();
        let now = std::time::Instant::now();
        redraw.input(now);

        assert!(redraw.take_frame(now));
        assert!(!redraw.take_frame(now));
    }
    #[test]
    fn keys_arriving_faster_than_a_frame_do_not_each_get_one() {
        // Regression: 166 key events in a second drew 166 frames at ~6ms
        // apiece, and the echo the person was waiting on queued behind
        // frames that were overwritten before anyone saw them.
        let mut redraw = RedrawScheduler::default();
        let now = std::time::Instant::now();
        redraw.input(now);
        assert!(redraw.take_frame(now));

        redraw.input(now + std::time::Duration::from_millis(1));

        assert!(
            !redraw.take_frame(now),
            "a second key within the gap must ride the tick"
        );
        redraw.due();
        assert!(redraw.take_frame(now), "and the tick must still present it");
    }
    #[test]
    fn a_keystroke_after_the_gap_still_presents_on_the_spot() {
        let mut redraw = RedrawScheduler::default();
        let now = std::time::Instant::now();
        redraw.input(now);
        assert!(redraw.take_frame(now));

        let later = now + INPUT_PRESENT_GAP;
        redraw.input(later);

        assert!(redraw.take_frame(later));
    }
    #[test]
    fn a_keystroke_carries_pending_damage_with_it() {
        // The frame it presents is the whole screen, so damage that was
        // waiting for the next tick is already on it and must not ask for
        // a second frame of its own.
        let mut redraw = RedrawScheduler::default();
        let now = std::time::Instant::now();
        redraw.changed();
        redraw.input(now);

        assert!(redraw.take_frame(now));
        redraw.due();
        assert!(!redraw.take_frame(now));
    }
}
