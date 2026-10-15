//! Which views are being asked to refresh, and the task that asks.
//!
//! A view shown with a `refresh_every` has one timer task. Whatever removes
//! the view removes its timer here, so a timer never outlives what it
//! refreshes; and a view shown again, or patched, keeps the timer it has
//! while its interval is unchanged, so a busy agent that re-shows its view
//! often is not asked sooner for it.

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use argus_protocol::PaneId;
use tokio::task::AbortHandle;

use super::*;

struct Timer {
    every: u32,
    task: AbortHandle,
    /// Which timer this is, so a test can tell one kept from one restarted.
    #[cfg(test)]
    generation: u64,
}

#[derive(Default)]
pub(super) struct Timers {
    held: HashMap<(PaneId, String), Timer>,
    #[cfg(test)]
    started: u64,
}

impl Timers {
    /// Has the view refreshed every `every` seconds, or not at all for
    /// `None`. A timer already running at that interval is left as it is;
    /// otherwise the old one is stopped and `start` makes the new, which
    /// may decline (no runtime to spawn on).
    pub(super) fn keep(
        &mut self,
        pane: PaneId,
        id: &str,
        every: Option<u32>,
        start: impl FnOnce(Duration) -> Option<AbortHandle>,
    ) {
        let key = (pane, id.to_string());
        if let (Some(every), Some(timer)) = (every, self.held.get(&key)) {
            if timer.every == every {
                return;
            }
        }
        if let Some(old) = self.held.remove(&key) {
            old.task.abort();
        }
        let Some(every) = every else { return };
        if let Some(task) = start(Duration::from_secs(every.into())) {
            #[cfg(test)]
            {
                self.started += 1;
            }
            let timer = Timer {
                every,
                task,
                #[cfg(test)]
                generation: self.started,
            };
            self.held.insert(key, timer);
        }
    }

    pub(super) fn stop(&mut self, pane: PaneId, id: &str) {
        if let Some(old) = self.held.remove(&(pane, id.to_string())) {
            old.task.abort();
        }
    }

    /// Whether the view is being asked to refresh, for the tests of what
    /// stops a timer.
    #[cfg(test)]
    pub(super) fn is_refreshing(&self, pane: PaneId, id: &str) -> bool {
        self.held.contains_key(&(pane, id.to_string()))
    }

    /// Which timer the view has, for the tests of what keeps one.
    #[cfg(test)]
    pub(super) fn generation(&self, pane: PaneId, id: &str) -> Option<u64> {
        self.held.get(&(pane, id.to_string())).map(|t| t.generation)
    }
}

/// Asks for a view to be refreshed every `every`, the first time one
/// interval after it was shown, for as long as it is held. The daemon is
/// held weakly so that a timer never keeps it alive; `Timers` aborts the
/// task when the view goes, and the loop also ends on its own should the
/// view be gone first. A delayed tick is not made up, so an agent that was
/// busy is asked once, not once per tick it missed.
pub(super) async fn keep_refreshing(daemon: Weak<Daemon>, pane: PaneId, id: String, every: Duration) {
    let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let Some(daemon) = daemon.upgrade() else { return };
        if !daemon.refresh_view(pane, &id) {
            return;
        }
    }
}

/// Starts the timer task for a view, on the runtime the daemon is on.
pub(super) fn spawn_timer(daemon: &Arc<Daemon>, pane: PaneId, id: &str, every: Duration) -> Option<AbortHandle> {
    let task = keep_refreshing(Arc::downgrade(daemon), pane, id.to_string(), every);
    tokio::runtime::Handle::try_current().ok().map(|rt| rt.spawn(task).abort_handle())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A task that never ends on its own, standing for a refresh timer.
    fn pending() -> Option<AbortHandle> {
        Some(tokio::spawn(std::future::pending::<()>()).abort_handle())
    }

    async fn stopped(timer: &AbortHandle) -> bool {
        for _ in 0..50 {
            if timer.is_finished() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        false
    }

    /// Starts a timer through `keep` and hands back its task to watch.
    fn started(timers: &mut Timers, pane: u64, id: &str, every: Option<u32>) -> Option<AbortHandle> {
        let mut made = None;
        timers.keep(PaneId(pane), id, every, |_| {
            made = pending();
            made.clone()
        });
        made
    }

    #[tokio::test]
    async fn the_same_interval_keeps_the_timer_and_a_new_one_replaces_it() {
        let mut timers = Timers::default();
        let old = started(&mut timers, 1, "a", Some(60)).unwrap();
        let generation = timers.generation(PaneId(1), "a");
        assert!(started(&mut timers, 1, "a", Some(60)).is_none(), "nothing was started");
        assert_eq!(timers.generation(PaneId(1), "a"), generation);
        assert!(!old.is_finished());

        let new = started(&mut timers, 1, "a", Some(120)).unwrap();
        assert!(stopped(&old).await);
        assert!(!new.is_finished());
        assert_ne!(timers.generation(PaneId(1), "a"), generation);

        assert!(started(&mut timers, 1, "a", None).is_none());
        assert!(stopped(&new).await);
        assert!(!timers.is_refreshing(PaneId(1), "a"));
    }

    #[tokio::test]
    async fn stopping_a_view_stops_its_timer_and_no_other() {
        let mut timers = Timers::default();
        let a = started(&mut timers, 1, "a", Some(60)).unwrap();
        let b = started(&mut timers, 1, "b", Some(60)).unwrap();
        let other = started(&mut timers, 2, "a", Some(60)).unwrap();
        timers.stop(PaneId(1), "a");
        assert!(stopped(&a).await);
        assert!(!b.is_finished() && !other.is_finished());
        assert!(timers.is_refreshing(PaneId(1), "b") && timers.is_refreshing(PaneId(2), "a"));
    }
}
