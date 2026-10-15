//! What time it is, for the durations the daemon decides on: how long a
//! resumed agent ran, how close together restarts came, how long a child
//! agent has been quiet. A test moves it on rather than waiting.

use std::time::Instant;

pub(super) trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

pub(super) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
