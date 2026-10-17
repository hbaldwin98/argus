//! What time it is: the instant for the durations the daemon decides on —
//! how long a resumed agent ran, how close together restarts came, how
//! long a child agent has been quiet — which a test moves on rather than
//! waiting, and the wall-clock second a stored record is stamped with.

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

/// Seconds since the epoch, as the store's columns hold them.
pub(in crate::state) fn epoch_secs() -> i64 {
    argus_protocol::ago::now() as i64
}
