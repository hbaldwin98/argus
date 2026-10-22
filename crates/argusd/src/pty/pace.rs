//! When a busy pane's next frame may go: `FRAME_INTERVAL` after the last
//! one began, not after it was sent. On Windows the wait holds the system
//! timer at 1 ms, since its default 15.6 ms tick turns a 16 ms sleep into
//! 31 and caps a stream at 32 frames a second in every window.

use std::time::Duration;

use tokio::time::Instant;

pub(super) const FRAME_INTERVAL: Duration = Duration::from_millis(16);

/// Waits until a frame begun at `began` has had its interval.
pub(super) async fn next_frame(began: Instant) {
    let _fine = Fine::hold();
    tokio::time::sleep_until(began + FRAME_INTERVAL).await;
}

/// The system timer at 1 ms while held. Windows 10 2004 and later keep it
/// per process, so an idle daemon costs the machine nothing.
struct Fine;

impl Fine {
    fn hold() -> Self {
        #[cfg(windows)]
        // SAFETY: no pointers; each call is paired with the drop's end.
        unsafe {
            windows_sys::Win32::Media::timeBeginPeriod(1);
        }
        Fine
    }
}

impl Drop for Fine {
    fn drop(&mut self) {
        #[cfg(windows)]
        // SAFETY: ends the period `hold` began.
        unsafe {
            windows_sys::Win32::Media::timeEndPeriod(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_frame_that_took_its_interval_goes_at_once() {
        let began = Instant::now() - FRAME_INTERVAL;
        let start = std::time::Instant::now();
        next_frame(began).await;
        assert!(start.elapsed() < Duration::from_millis(5), "waited {:?}", start.elapsed());
    }

    /// The default tick would make this 2 ms wait about 15.6; the median of
    /// several keeps a loaded machine's odd slow wake out of it.
    #[tokio::test]
    async fn the_wait_is_the_rest_of_the_interval_to_the_millisecond() {
        let mut waits = Vec::new();
        for _ in 0..7 {
            let began = Instant::now() - (FRAME_INTERVAL - Duration::from_millis(2));
            let start = std::time::Instant::now();
            next_frame(began).await;
            waits.push(start.elapsed());
        }
        waits.sort();
        assert!(waits[3] < Duration::from_millis(8), "median wait {:?} of {waits:?}", waits[3]);
    }
}
